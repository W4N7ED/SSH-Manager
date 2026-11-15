//! Accès aux fichiers par FTP, ou FTPS (TLS explicite, certificats vérifiés
//! par le magasin de Windows).

use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

use ssh_manager_core::Connection;
use suppaftp::list::{File as ListedFile, PosixPexQuery};
use suppaftp::tokio::{AsyncRustlsConnector, AsyncRustlsFtpStream};
use suppaftp::types::FileType;

use super::transfer::{self, CopyOutcome, TransferHooks};
use super::{FsError, RemoteEntry};
use crate::session::transport::SessionError;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

pub struct FtpBackend {
    stream: AsyncRustlsFtpStream,
}

fn failed(context: &str, error: impl std::fmt::Display) -> FsError {
    FsError::Operation(format!("{context} : {error}"))
}

fn tls_connector() -> Result<AsyncRustlsConnector, FsError> {
    let config = crate::tls::client_config()
        .map_err(|error| failed("configuration TLS impossible", error))?;
    Ok(AsyncRustlsConnector::from(
        tokio_rustls::TlsConnector::from(config),
    ))
}

/// Droits « rwxr-x--- » d'une entrée de listing, quand le serveur les fournit.
fn permissions(file: &ListedFile) -> String {
    [
        PosixPexQuery::Owner,
        PosixPexQuery::Group,
        PosixPexQuery::Others,
    ]
    .into_iter()
    .flat_map(|who| {
        [
            if file.can_read(who) { 'r' } else { '-' },
            if file.can_write(who) { 'w' } else { '-' },
            if file.can_execute(who) { 'x' } else { '-' },
        ]
    })
    .collect()
}

impl FtpBackend {
    pub async fn connect(connection: &Connection) -> Result<Self, FsError> {
        let address = (connection.host.as_str(), connection.port);
        let mut stream =
            tokio::time::timeout(CONNECT_TIMEOUT, AsyncRustlsFtpStream::connect(address))
                .await
                .map_err(|_| {
                    FsError::Session(SessionError::Connect(format!(
                        "aucune réponse après {} s",
                        CONNECT_TIMEOUT.as_secs()
                    )))
                })?
                .map_err(|error| FsError::Session(SessionError::Connect(error.to_string())))?;

        if connection.ftp_secure {
            stream = stream
                .into_secure(tls_connector()?, &connection.host)
                .await
                .map_err(|error| {
                    FsError::Session(SessionError::Connect(format!("FTPS refusé : {error}")))
                })?;
        }
        stream
            .login(
                connection.username.as_str(),
                connection.password.as_deref().unwrap_or_default(),
            )
            .await
            .map_err(|_| FsError::Session(SessionError::Authentication))?;
        // Le mode ASCII par défaut convertirait les fins de ligne et corromprait
        // tout fichier binaire (archives, images…).
        stream
            .transfer_type(FileType::Binary)
            .await
            .map_err(|error| failed("passage en mode binaire refusé", error))?;
        Ok(Self { stream })
    }

    pub async fn home(&mut self) -> Result<String, FsError> {
        self.stream
            .pwd()
            .await
            .map_err(|error| failed("dossier courant inconnu", error))
    }

    pub async fn list(&mut self, path: &str) -> Result<Vec<RemoteEntry>, FsError> {
        let lines = self
            .stream
            .list(Some(path))
            .await
            .map_err(|error| failed("lecture du dossier impossible", error))?;
        Ok(lines
            .iter()
            .filter_map(|line| line.parse::<ListedFile>().ok())
            .filter(|file| file.name() != "." && file.name() != "..")
            .map(|file| RemoteEntry {
                name: file.name().to_owned(),
                is_dir: file.is_directory(),
                size: file.size() as u64,
                modified: file
                    .modified()
                    .duration_since(UNIX_EPOCH)
                    .ok()
                    .map(|elapsed| elapsed.as_secs() as i64),
                permissions: Some(permissions(&file)),
            })
            .collect())
    }

    pub async fn create_dir(&mut self, path: &str) -> Result<(), FsError> {
        self.stream
            .mkdir(path)
            .await
            .map_err(|error| failed("création du dossier impossible", error))
    }

    pub async fn remove_file(&mut self, path: &str) -> Result<(), FsError> {
        self.stream
            .rm(path)
            .await
            .map_err(|error| failed("suppression impossible", error))
    }

    pub async fn remove_dir(&mut self, path: &str) -> Result<(), FsError> {
        self.stream
            .rmdir(path)
            .await
            .map_err(|error| failed("suppression du dossier impossible", error))
    }

    pub async fn rename(&mut self, from: &str, to: &str) -> Result<(), FsError> {
        self.stream
            .rename(from, to)
            .await
            .map_err(|error| failed("renommage impossible", error))
    }

    /// `SITE CHMOD` : extension répandue (vsftpd, ProFTPD, Pure-FTPd), mais
    /// facultative ; un refus du serveur est rapporté tel quel.
    pub async fn set_permissions(&mut self, path: &str, mode: u32) -> Result<(), FsError> {
        self.stream
            .site(format!("CHMOD {mode:o} {path}"))
            .await
            .map(|_| ())
            .map_err(|error| failed("le serveur refuse la modification des droits", error))
    }

    pub async fn download(
        &mut self,
        remote: &str,
        local: &Path,
        hooks: TransferHooks<'_>,
    ) -> Result<u64, FsError> {
        let mut source = self
            .stream
            .retr_as_stream(remote)
            .await
            .map_err(|error| failed("ouverture du fichier distant impossible", error))?;
        let mut target = tokio::fs::File::create(local)
            .await
            .map_err(|error| failed("création du fichier local impossible", error))?;
        let outcome = transfer::copy(&mut source, &mut target, hooks.progress, hooks.cancelled)
            .await
            .map_err(|error| failed("transfert interrompu", error))?;
        match outcome {
            CopyOutcome::Completed(bytes) => {
                // Seule la réponse finale du serveur dit si le transfert a vraiment réussi.
                source
                    .finish()
                    .await
                    .map_err(|error| failed("le serveur signale un transfert incomplet", error))?;
                Ok(bytes)
            }
            CopyOutcome::Cancelled => {
                if let Err(error) = self.stream.abort(source).await {
                    tracing::warn!(%error, "FTP transfer abort was not acknowledged");
                }
                drop(target);
                transfer::discard_partial_local(local).await;
                Err(FsError::Cancelled)
            }
        }
    }

    pub async fn upload(
        &mut self,
        local: &Path,
        remote: &str,
        hooks: TransferHooks<'_>,
    ) -> Result<u64, FsError> {
        let mut source = tokio::fs::File::open(local)
            .await
            .map_err(|error| failed("lecture du fichier local impossible", error))?;
        let mut target = self
            .stream
            .put_with_stream(remote)
            .await
            .map_err(|error| failed("envoi refusé", error))?;
        let outcome = transfer::copy(&mut source, &mut target, hooks.progress, hooks.cancelled)
            .await
            .map_err(|error| failed("transfert interrompu", error))?;
        match outcome {
            CopyOutcome::Completed(bytes) => {
                target
                    .finish()
                    .await
                    .map_err(|error| failed("le serveur signale un envoi incomplet", error))?;
                Ok(bytes)
            }
            CopyOutcome::Cancelled => {
                if let Err(error) = self.stream.abort(target).await {
                    tracing::warn!(%error, "FTP upload abort was not acknowledged");
                }
                if let Err(error) = self.remove_file(remote).await {
                    tracing::warn!(%remote, %error, "partial upload could not be removed");
                }
                Err(FsError::Cancelled)
            }
        }
    }

    pub async fn close(mut self) {
        // Fermeture de politesse : la connexion se termine de toute façon.
        if let Err(error) = self.stream.quit().await {
            tracing::warn!(%error, "FTP session did not close cleanly");
        }
    }
}
