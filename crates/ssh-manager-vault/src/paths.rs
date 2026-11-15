//! Emplacements des données sur disque.

use std::path::PathBuf;

/// Dossier de données de la nouvelle application : `%APPDATA%\SSH Manager`.
/// Distinct de celui de l'app Electron (`%APPDATA%\ssh-manager`), que la
/// migration lit sans jamais le modifier.
pub fn data_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("SSH Manager")
}

pub fn vault_file() -> PathBuf {
    data_dir().join("vault.json")
}

/// Enregistrements chiffrés des sessions terminal.
pub fn recordings_dir() -> PathBuf {
    data_dir().join("recordings")
}

/// Dossier de l'application Electron.
pub fn legacy_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("ssh-manager")
}
