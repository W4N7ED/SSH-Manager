//! Navigateur de fichiers distants, en SFTP ou en FTP/FTPS.

mod browser;
mod ftp;
pub mod remote_path;
mod sftp;
#[cfg(test)]
mod sftp_tests;
pub mod view;

use std::path::Path;

use ssh_manager_core::{Connection, ConnectionKind};

pub use browser::FileBrowser;
use ftp::FtpBackend;
use sftp::SftpBackend;

use crate::session::transport::{SessionError, SharedStatus};

/// Une entrée d'un dossier distant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    /// Date de modification, en secondes depuis l'époque Unix.
    pub modified: Option<i64>,
    pub permissions: Option<String>,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum FsError {
    /// Échec d'ouverture de la session : connexion, clé d'hôte, authentification.
    #[error(transparent)]
    Session(#[from] SessionError),
    /// Échec d'une opération, la session restant utilisable.
    #[error("{0}")]
    Operation(String),
}

/// Dossiers d'abord, puis ordre alphabétique sans casse.
pub fn sort_entries(entries: &mut [RemoteEntry]) {
    entries.sort_by(|left, right| {
        right
            .is_dir
            .cmp(&left.is_dir)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
}

/// Un client de fichiers, quel que soit son protocole.
enum Backend {
    Sftp(SftpBackend),
    Ftp(FtpBackend),
}

impl Backend {
    async fn connect(
        connection: &Connection,
        pinned: Option<String>,
        status: &SharedStatus,
    ) -> Result<Self, FsError> {
        match connection.kind {
            ConnectionKind::Ftp => Ok(Self::Ftp(FtpBackend::connect(connection).await?)),
            ConnectionKind::Sftp | ConnectionKind::Ssh => Ok(Self::Sftp(
                SftpBackend::connect(connection, pinned, status).await?,
            )),
        }
    }

    async fn home(&mut self) -> Result<String, FsError> {
        match self {
            Self::Sftp(sftp) => sftp.home().await,
            Self::Ftp(ftp) => ftp.home().await,
        }
    }

    async fn list(&mut self, path: &str) -> Result<Vec<RemoteEntry>, FsError> {
        let mut entries = match self {
            Self::Sftp(sftp) => sftp.list(path).await?,
            Self::Ftp(ftp) => ftp.list(path).await?,
        };
        sort_entries(&mut entries);
        Ok(entries)
    }

    async fn create_dir(&mut self, path: &str) -> Result<(), FsError> {
        match self {
            Self::Sftp(sftp) => sftp.create_dir(path).await,
            Self::Ftp(ftp) => ftp.create_dir(path).await,
        }
    }

    async fn remove_file(&mut self, path: &str) -> Result<(), FsError> {
        match self {
            Self::Sftp(sftp) => sftp.remove_file(path).await,
            Self::Ftp(ftp) => ftp.remove_file(path).await,
        }
    }

    async fn remove_dir(&mut self, path: &str) -> Result<(), FsError> {
        match self {
            Self::Sftp(sftp) => sftp.remove_dir(path).await,
            Self::Ftp(ftp) => ftp.remove_dir(path).await,
        }
    }

    /// Supprime un dossier et tout son contenu. Les protocoles ne savent
    /// supprimer que des dossiers vides : on vide d'abord, du plus profond au
    /// plus haut. Un lien symbolique est supprimé, jamais suivi.
    async fn remove_tree(&mut self, root: &str) -> Result<(), FsError> {
        let mut directories = vec![root.to_owned()];
        let mut index = 0;
        while index < directories.len() {
            let current = directories[index].clone();
            for entry in self.list(&current).await? {
                let path = remote_path::join(&current, &entry.name);
                if entry.is_dir {
                    directories.push(path)
                } else {
                    self.remove_file(&path).await?
                }
            }
            index += 1;
        }
        for directory in directories.iter().rev() {
            self.remove_dir(directory).await?;
        }
        Ok(())
    }

    async fn download(&mut self, remote: &str, local: &Path) -> Result<u64, FsError> {
        match self {
            Self::Sftp(sftp) => sftp.download(remote, local).await,
            Self::Ftp(ftp) => ftp.download(remote, local).await,
        }
    }

    async fn upload(&mut self, local: &Path, remote: &str) -> Result<u64, FsError> {
        match self {
            Self::Sftp(sftp) => sftp.upload(local, remote).await,
            Self::Ftp(ftp) => ftp.upload(local, remote).await,
        }
    }

    async fn close(self) {
        match self {
            Self::Sftp(sftp) => sftp.close().await,
            Self::Ftp(ftp) => ftp.close().await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, is_dir: bool) -> RemoteEntry {
        RemoteEntry {
            name: name.into(),
            is_dir,
            size: 0,
            modified: None,
            permissions: None,
        }
    }

    #[test]
    fn should_list_folders_first_then_names_without_case() {
        let mut entries = vec![
            entry("b.txt", false),
            entry("Zeta", true),
            entry("A.txt", false),
            entry("alpha", true),
        ];
        sort_entries(&mut entries);
        let names: Vec<_> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, ["alpha", "Zeta", "A.txt", "b.txt"]);
    }
}
