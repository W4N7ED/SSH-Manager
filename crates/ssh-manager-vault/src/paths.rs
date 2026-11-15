//! Emplacements des données sur disque.
//!
//! - Installée : `%APPDATA%\SSH Manager`.
//! - Portable : un dossier `SSH Manager Data` à côté de l'exécutable, pour que
//!   tout voyage avec lui (clé USB). L'exécutable est portable si son nom
//!   contient « portable », ou si ce dossier existe déjà à côté de lui.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub const PORTABLE_DATA_FOLDER: &str = "SSH Manager Data";

/// Règle de détection, isolée pour être testée sans dépendre du disque.
pub fn is_portable_executable(executable: &Path, data_folder_exists: bool) -> bool {
    let named_portable = executable
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_lowercase().contains("portable"))
        .unwrap_or(false);
    named_portable || data_folder_exists
}

/// Dossier de données de la version portable, s'il y a lieu (calculé une fois).
pub fn portable_data_dir() -> Option<&'static Path> {
    static PORTABLE: OnceLock<Option<PathBuf>> = OnceLock::new();
    PORTABLE
        .get_or_init(|| {
            let executable = std::env::current_exe().ok()?;
            let folder = executable.parent()?.join(PORTABLE_DATA_FOLDER);
            is_portable_executable(&executable, folder.is_dir()).then_some(folder)
        })
        .as_deref()
}

pub fn is_portable() -> bool {
    portable_data_dir().is_some()
}

/// Dossier de données de l'application. Distinct de celui de l'app Electron
/// (`%APPDATA%\ssh-manager`), que la migration lit sans jamais le modifier.
pub fn data_dir() -> PathBuf {
    match portable_data_dir() {
        Some(folder) => folder.to_owned(),
        None => dirs::config_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("SSH Manager"),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_detect_a_portable_executable_by_its_name() {
        assert!(is_portable_executable(
            Path::new(r"E:\SSH-Manager-2.0.0-Portable.exe"),
            false
        ));
        assert!(!is_portable_executable(
            Path::new(r"C:\Program Files\SSH Manager\ssh-manager.exe"),
            false
        ));
    }

    #[test]
    fn should_stay_portable_once_its_data_folder_exists() {
        // Renommer l'exécutable ne doit pas faire perdre les données qui l'accompagnent.
        assert!(is_portable_executable(
            Path::new(r"E:\outils\ssh.exe"),
            true
        ));
    }
}
