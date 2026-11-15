//! Lecture d'une clé privée (OpenSSH, PEM, PKCS#8 ou PuTTY .ppk), chiffrée ou
//! non par une phrase de passe.

use std::path::Path;

use russh::keys::{Error as KeyError, PrivateKey, load_secret_key};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeyLoadError {
    #[error("la clé privée est protégée par une phrase de passe")]
    PassphraseRequired,
    #[error("phrase de passe de la clé incorrecte")]
    WrongPassphrase,
    #[error("clé privée illisible : {0}")]
    Unreadable(String),
}

/// Charge la clé ; `passphrase` n'est utilisée que si la clé est chiffrée.
pub fn load(path: &Path, passphrase: Option<&str>) -> Result<PrivateKey, KeyLoadError> {
    match load_secret_key(path, passphrase) {
        Ok(key) => Ok(key),
        Err(KeyError::KeyIsEncrypted) => Err(KeyLoadError::PassphraseRequired),
        // Avec une phrase, l'échec du déchiffrement ne dit pas toujours son nom :
        // on vérifie que la clé est bien chiffrée pour accuser la phrase.
        Err(error) if passphrase.is_some() && requires_passphrase(path) => {
            tracing::debug!(%error, path = %path.display(), "private key decryption failed");
            Err(KeyLoadError::WrongPassphrase)
        }
        Err(error) => Err(KeyLoadError::Unreadable(error.to_string())),
    }
}

/// Vrai si la clé est chiffrée : il faudra une phrase de passe pour l'utiliser.
/// Vérification rapide, sans déchiffrer.
pub fn requires_passphrase(path: &Path) -> bool {
    matches!(load_secret_key(path, None), Err(KeyError::KeyIsEncrypted))
}

#[cfg(test)]
mod tests {
    use super::*;
    use russh::keys::Algorithm;
    use russh::keys::ssh_key::LineEnding;

    const PASSPHRASE: &str = "correct horse";

    fn write_key(name: &str, encrypted: bool) -> std::path::PathBuf {
        let mut key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap();
        if encrypted {
            key = key.encrypt(&mut rand::rng(), PASSPHRASE).unwrap();
        }
        let path = std::env::temp_dir().join(format!(
            "ssh-manager-key-{name}-{:x}",
            rand::random::<u64>()
        ));
        std::fs::write(&path, key.to_openssh(LineEnding::LF).unwrap().as_bytes()).unwrap();
        path
    }

    #[test]
    fn should_load_a_plain_key_without_passphrase() {
        let path = write_key("plain", false);
        assert!(!requires_passphrase(&path));
        assert!(load(&path, None).is_ok());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn should_ask_for_the_passphrase_of_an_encrypted_key() {
        let path = write_key("needs", true);
        assert!(requires_passphrase(&path));
        assert_eq!(
            load(&path, None).unwrap_err(),
            KeyLoadError::PassphraseRequired
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn should_decrypt_with_the_right_passphrase() {
        let path = write_key("right", true);
        assert!(load(&path, Some(PASSPHRASE)).is_ok());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn should_report_a_wrong_passphrase() {
        let path = write_key("wrong", true);
        assert_eq!(
            load(&path, Some("nope")).unwrap_err(),
            KeyLoadError::WrongPassphrase
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn should_report_a_missing_file_as_unreadable() {
        let missing = std::env::temp_dir().join("ssh-manager-no-such-key");
        assert!(matches!(
            load(&missing, None),
            Err(KeyLoadError::Unreadable(_))
        ));
    }
}
