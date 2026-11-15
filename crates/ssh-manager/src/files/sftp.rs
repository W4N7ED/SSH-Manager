//! Accès aux fichiers par SFTP, au-dessus d'une connexion SSH authentifiée.

use std::path::Path;

use russh::Disconnect;
use russh_sftp::client::SftpSession;
use ssh_manager_core::Connection;
use tokio::io::AsyncWriteExt;

use super::remote_path::format_permissions;
use super::transfer::{self, CopyOutcome, TransferHooks};
use super::{FsError, RemoteEntry};
use crate::session::transport::{self, SharedStatus, SshHandle, network};

pub struct SftpBackend {
    handle: SshHandle,
    sftp: SftpSession,
}

fn failed(context: &str, error: impl std::fmt::Display) -> FsError {
    FsError::Operation(format!("{context} : {error}"))
}

impl SftpBackend {
    pub async fn connect(
        connection: &Connection,
        pinned: Option<String>,
        status: &SharedStatus,
    ) -> Result<Self, FsError> {
        let handle = transport::open(connection, pinned, status).await?;
        let channel = handle.channel_open_session().await.map_err(network)?;
        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(network)?;
        let sftp = SftpSession::new(channel.into_stream())
            .await
            .map_err(|error| failed("le serveur refuse le sous-système SFTP", error))?;
        Ok(Self { handle, sftp })
    }

    /// Dossier d'accueil de l'utilisateur sur le serveur.
    pub async fn home(&mut self) -> Result<String, FsError> {
        self.sftp
            .canonicalize(".")
            .await
            .map_err(|error| failed("dossier d'accueil introuvable", error))
    }

    pub async fn list(&mut self, path: &str) -> Result<Vec<RemoteEntry>, FsError> {
        let entries = self
            .sftp
            .read_dir(path)
            .await
            .map_err(|error| failed("lecture du dossier impossible", error))?;
        Ok(entries
            .filter(|entry| entry.file_name() != "." && entry.file_name() != "..")
            .map(|entry| {
                let metadata = entry.metadata();
                RemoteEntry {
                    is_dir: entry.file_type().is_dir(),
                    size: metadata.size.unwrap_or(0),
                    modified: metadata.mtime.map(i64::from),
                    permissions: metadata.permissions.map(format_permissions),
                    name: entry.file_name(),
                }
            })
            .collect())
    }

    pub async fn create_dir(&mut self, path: &str) -> Result<(), FsError> {
        self.sftp
            .create_dir(path)
            .await
            .map_err(|error| failed("création du dossier impossible", error))
    }

    pub async fn remove_file(&mut self, path: &str) -> Result<(), FsError> {
        self.sftp
            .remove_file(path)
            .await
            .map_err(|error| failed("suppression impossible", error))
    }

    pub async fn remove_dir(&mut self, path: &str) -> Result<(), FsError> {
        self.sftp
            .remove_dir(path)
            .await
            .map_err(|error| failed("suppression du dossier impossible", error))
    }

    pub async fn rename(&mut self, from: &str, to: &str) -> Result<(), FsError> {
        self.sftp
            .rename(from, to)
            .await
            .map_err(|error| failed("renommage impossible", error))
    }

    pub async fn set_permissions(&mut self, path: &str, mode: u32) -> Result<(), FsError> {
        let attributes = russh_sftp::protocol::FileAttributes {
            permissions: Some(mode),
            ..Default::default()
        };
        self.sftp
            .set_metadata(path, attributes)
            .await
            .map_err(|error| failed("modification des droits impossible", error))
    }

    pub async fn download(
        &mut self,
        remote: &str,
        local: &Path,
        hooks: TransferHooks<'_>,
    ) -> Result<u64, FsError> {
        let mut source = self
            .sftp
            .open(remote)
            .await
            .map_err(|error| failed("ouverture du fichier distant impossible", error))?;
        let mut target = tokio::fs::File::create(local)
            .await
            .map_err(|error| failed("création du fichier local impossible", error))?;
        let outcome = transfer::copy(&mut source, &mut target, hooks.progress, hooks.cancelled)
            .await
            .map_err(|error| failed("transfert interrompu", error))?;
        match outcome {
            CopyOutcome::Completed(bytes) => Ok(bytes),
            CopyOutcome::Cancelled => {
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
            .sftp
            .create(remote)
            .await
            .map_err(|error| failed("création du fichier distant impossible", error))?;
        let outcome = transfer::copy(&mut source, &mut target, hooks.progress, hooks.cancelled)
            .await
            .map_err(|error| failed("transfert interrompu", error))?;
        // La fermeture confirme l'écriture côté serveur : un échec ici est un échec du transfert.
        target
            .shutdown()
            .await
            .map_err(|error| failed("le serveur n'a pas confirmé l'écriture", error))?;
        match outcome {
            CopyOutcome::Completed(bytes) => Ok(bytes),
            CopyOutcome::Cancelled => {
                // Un fichier distant tronqué serait pris pour le bon : on le retire.
                if let Err(error) = self.remove_file(remote).await {
                    tracing::warn!(%remote, %error, "partial upload could not be removed");
                }
                Err(FsError::Cancelled)
            }
        }
    }

    pub async fn close(self) {
        // Fermeture de politesse : la connexion se termine de toute façon, un
        // échec n'est donc qu'un avertissement.
        if let Err(error) = self.sftp.close().await {
            tracing::warn!(%error, "SFTP session did not close cleanly");
        }
        if let Err(error) = self
            .handle
            .disconnect(Disconnect::ByApplication, "", "fr")
            .await
        {
            tracing::warn!(%error, "SSH disconnect after SFTP session failed");
        }
    }
}
