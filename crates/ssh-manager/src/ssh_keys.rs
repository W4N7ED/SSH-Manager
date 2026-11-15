//! Création de paires de clés SSH au format OpenSSH, dans `~/.ssh`.
//!
//! Une clé existante n'est jamais écrasée : perdre une clé privée est
//! irréversible (accès aux serveurs qui ne connaissent qu'elle).

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use russh::keys::ssh_key::LineEnding;
use russh::keys::{Algorithm, PrivateKey};

/// Le nom devient un nom de fichier : ni séparateur ni remontée d'arborescence.
const NAME_MAX_LENGTH: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyType {
    Ed25519,
    Rsa4096,
}

impl KeyType {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ed25519 => "Ed25519",
            Self::Rsa4096 => "RSA 4096",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::Ed25519 => "Moderne, court, recommandé",
            Self::Rsa4096 => "Compatible avec les serveurs anciens",
        }
    }

    fn file_prefix(self) -> &'static str {
        match self {
            Self::Ed25519 => "id_ed25519",
            Self::Rsa4096 => "id_rsa",
        }
    }

    fn algorithm(self) -> Algorithm {
        match self {
            Self::Ed25519 => Algorithm::Ed25519,
            Self::Rsa4096 => Algorithm::Rsa { hash: None },
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum KeyGenerationError {
    #[error(
        "nom de fichier invalide : lettres, chiffres, point, tiret et souligné uniquement (64 au plus)"
    )]
    InvalidName,
    #[error("un fichier « {0} » existe déjà : choisissez un autre nom")]
    AlreadyExists(String),
    #[error("génération de la clé impossible : {0}")]
    Generation(String),
    #[error("écriture de {path} impossible : {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub struct GeneratedKey {
    pub private_key_path: PathBuf,
    /// Ligne à ajouter dans `~/.ssh/authorized_keys` sur le serveur.
    pub public_key: String,
}

pub fn is_valid_key_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= NAME_MAX_LENGTH
        && name != "."
        && name != ".."
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "._-".contains(character))
}

/// « id_ed25519_serveur-web » à partir du nom de la connexion.
pub fn suggest_key_name(key_type: KeyType, connection_label: &str) -> String {
    let mut slug = String::new();
    for character in connection_label.to_lowercase().chars() {
        let character = strip_accent(character);
        if character.is_ascii_alphanumeric() {
            slug.push(character);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug: String = slug.trim_matches('-').chars().take(40).collect();
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() {
        key_type.file_prefix().to_owned()
    } else {
        format!("{}_{slug}", key_type.file_prefix())
    }
}

fn strip_accent(character: char) -> char {
    match character {
        'à' | 'â' | 'ä' | 'á' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'î' | 'ï' | 'í' => 'i',
        'ô' | 'ö' | 'ó' => 'o',
        'ù' | 'û' | 'ü' | 'ú' => 'u',
        'ç' => 'c',
        'ñ' => 'n',
        other => other,
    }
}

/// Dossier `~/.ssh` de l'utilisateur.
pub fn ssh_directory() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".ssh")
}

/// Crée la paire `name` / `name.pub` dans `directory`, sans phrase de passe.
pub fn generate(
    directory: &Path,
    name: &str,
    key_type: KeyType,
) -> Result<GeneratedKey, KeyGenerationError> {
    if !is_valid_key_name(name) {
        return Err(KeyGenerationError::InvalidName);
    }
    let private_path = directory.join(name);
    let public_path = directory.join(format!("{name}.pub"));
    if private_path.exists() || public_path.exists() {
        return Err(KeyGenerationError::AlreadyExists(name.to_owned()));
    }

    let mut key = PrivateKey::random(&mut rand::rng(), key_type.algorithm())
        .map_err(|error| KeyGenerationError::Generation(error.to_string()))?;
    key.set_comment(default_comment());
    let private_text = key
        .to_openssh(LineEnding::LF)
        .map_err(|error| KeyGenerationError::Generation(error.to_string()))?;
    let public_text = key
        .public_key()
        .to_openssh()
        .map_err(|error| KeyGenerationError::Generation(error.to_string()))?;

    std::fs::create_dir_all(directory).map_err(|source| KeyGenerationError::Write {
        path: directory.to_owned(),
        source,
    })?;
    write_new_file(&private_path, private_text.as_bytes())?;
    if let Err(error) = write_new_file(&public_path, format!("{public_text}\n").as_bytes()) {
        // Sans sa clé publique, la clé privée reste utilisable : on la garde.
        tracing::error!(path = %public_path.display(), %error, "public key file could not be written");
    }
    tracing::info!(key_type = key_type.label(), path = %private_path.display(), "ssh key pair generated");
    Ok(GeneratedKey {
        private_key_path: private_path,
        public_key: public_text,
    })
}

/// « utilisateur@poste », comme ssh-keygen.
fn default_comment() -> String {
    let user = std::env::var("USERNAME").unwrap_or_else(|_| "user".into());
    let host = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "windows".into());
    format!("{user}@{}", host.to_lowercase())
}

/// Écrit un fichier qui ne doit pas exister : échoue plutôt que d'écraser.
fn write_new_file(path: &Path, content: &[u8]) -> Result<(), KeyGenerationError> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .and_then(|mut file| file.write_all(content))
        .map_err(|source| match source.kind() {
            std::io::ErrorKind::AlreadyExists => KeyGenerationError::AlreadyExists(
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            ),
            _ => KeyGenerationError::Write {
                path: path.to_owned(),
                source,
            },
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("ssh-manager-keys-{:x}", rand::random::<u64>()));
        std::fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn should_suggest_a_file_name_from_the_connection_name() {
        assert_eq!(
            suggest_key_name(KeyType::Ed25519, "Serveur Web — Prod"),
            "id_ed25519_serveur-web-prod"
        );
        assert_eq!(suggest_key_name(KeyType::Rsa4096, "été"), "id_rsa_ete");
        assert_eq!(suggest_key_name(KeyType::Ed25519, "  ***  "), "id_ed25519");
    }

    #[test]
    fn should_reject_names_that_could_escape_the_ssh_folder() {
        assert!(is_valid_key_name("id_ed25519_web"));
        assert!(!is_valid_key_name("../id_rsa"));
        assert!(!is_valid_key_name("sub\\key"));
        assert!(!is_valid_key_name(".."));
        assert!(!is_valid_key_name(""));
    }

    #[test]
    fn should_write_a_usable_ed25519_key_pair() {
        let directory = temp_dir();
        let generated = generate(&directory, "id_test", KeyType::Ed25519).unwrap();
        let private = std::fs::read_to_string(&generated.private_key_path).unwrap();
        assert!(private.starts_with("-----BEGIN OPENSSH PRIVATE KEY-----"));
        assert!(
            PrivateKey::from_openssh(&private).is_ok(),
            "la clé écrite doit se relire"
        );
        assert!(generated.public_key.starts_with("ssh-ed25519 "));
        let public = std::fs::read_to_string(directory.join("id_test.pub")).unwrap();
        assert_eq!(public.trim_end(), generated.public_key);
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn should_never_overwrite_an_existing_key() {
        let directory = temp_dir();
        std::fs::write(directory.join("id_test"), b"precious").unwrap();
        assert!(matches!(
            generate(&directory, "id_test", KeyType::Ed25519),
            Err(KeyGenerationError::AlreadyExists(_))
        ));
        assert_eq!(
            std::fs::read(directory.join("id_test")).unwrap(),
            b"precious"
        );
        std::fs::remove_dir_all(&directory).unwrap();
    }
}
