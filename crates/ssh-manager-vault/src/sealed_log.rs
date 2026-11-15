//! Journal chiffré en ajout seul, pour les enregistrements de session.
//!
//! Le fichier est une suite d'enregistrements indépendants, chacun chiffré et
//! authentifié en AES-256-GCM :
//!
//! ```text
//! SMLOG1\n
//! [longueur u32 LE][nonce 12 octets][chiffré + étiquette 16 octets]
//! [longueur u32 LE][nonce 12 octets][chiffré + étiquette 16 octets]
//! …
//! ```
//!
//! Ajouter un enregistrement ne réécrit jamais les précédents : un arrêt brutal
//! ne coûte au pire que le dernier, tronqué, que la lecture ignore.

use std::fs::{File, OpenOptions};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};

use aes_gcm::aead::KeyInit;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::crypto::{self, KEY_LENGTH, NONCE_LENGTH, VaultKey};
use crate::error::VaultError;

const MAGIC: &[u8] = b"SMLOG1\n";
/// Au-delà, l'en-tête de longueur est forcément corrompu : on arrête la lecture
/// plutôt que d'allouer une taille absurde.
const MAX_RECORD_LENGTH: usize = 64 * 1024 * 1024;

/// Clé propre aux journaux, dérivée de celle du coffre : une même clé ne sert
/// ainsi jamais à deux usages différents.
pub fn derive_log_key(vault_key: &[u8; KEY_LENGTH]) -> VaultKey {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(vault_key).expect("HMAC accepte toute longueur de clé");
    mac.update(b"ssh-manager/session-recordings/v1");
    let mut key = Zeroizing::new([0u8; KEY_LENGTH]);
    key.copy_from_slice(&mac.finalize().into_bytes());
    key
}

/// Écrit les enregistrements d'un journal, à la suite les uns des autres.
pub struct SealedLogWriter {
    path: PathBuf,
    file: File,
    key: VaultKey,
}

impl SealedLogWriter {
    /// Crée un nouveau journal ; échoue si le fichier existe déjà.
    pub fn create(path: impl Into<PathBuf>, key: VaultKey) -> Result<Self, VaultError> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| VaultError::io(parent, source))?;
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|source| VaultError::io(&path, source))?;
        file.write_all(MAGIC)
            .map_err(|source| VaultError::io(&path, source))?;
        Ok(Self { path, file, key })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Chiffre et ajoute un enregistrement, écrit d'un seul bloc.
    pub fn append(&mut self, plaintext: &[u8]) -> Result<(), VaultError> {
        let (nonce, ciphertext) = crypto::seal(&self.key, plaintext);
        let length = u32::try_from(NONCE_LENGTH + ciphertext.len())
            .expect("enregistrement de moins de 4 Go");
        let mut record = Vec::with_capacity(4 + length as usize);
        record.extend_from_slice(&length.to_le_bytes());
        record.extend_from_slice(&nonce);
        record.extend_from_slice(&ciphertext);
        self.file
            .write_all(&record)
            .and_then(|()| self.file.flush())
            .map_err(|source| VaultError::io(&self.path, source))
    }
}

/// Résultat de lecture d'un journal.
#[derive(Debug, Default)]
pub struct SealedLog {
    pub records: Vec<Zeroizing<Vec<u8>>>,
    /// Vrai si la fin du fichier était tronquée (arrêt brutal pendant l'écriture).
    pub truncated: bool,
}

/// Lit au plus `limit` enregistrements (tous si `None`).
pub fn read(
    path: &Path,
    key: &[u8; KEY_LENGTH],
    limit: Option<usize>,
) -> Result<SealedLog, VaultError> {
    let file = File::open(path).map_err(|source| VaultError::io(path, source))?;
    let mut reader = BufReader::new(file);
    let mut magic = [0u8; MAGIC.len()];
    reader
        .read_exact(&mut magic)
        .map_err(|source| VaultError::io(path, source))?;
    if magic != MAGIC {
        return Err(VaultError::Decryption {
            context: "en-tête du journal de session",
        });
    }

    let mut log = SealedLog::default();
    while limit.is_none_or(|limit| log.records.len() < limit) {
        let mut length = [0u8; 4];
        match read_fully(&mut reader, &mut length).map_err(|source| VaultError::io(path, source))? {
            0 => break,
            4 => {}
            _ => {
                log.truncated = true;
                break;
            }
        }
        let length = u32::from_le_bytes(length) as usize;
        if !(NONCE_LENGTH..=MAX_RECORD_LENGTH).contains(&length) {
            log.truncated = true;
            break;
        }
        let mut body = vec![0u8; length];
        if read_fully(&mut reader, &mut body).map_err(|source| VaultError::io(path, source))?
            < length
        {
            log.truncated = true;
            break;
        }
        let (nonce, ciphertext) = body.split_at(NONCE_LENGTH);
        log.records.push(crypto::open(
            key,
            nonce,
            ciphertext,
            "enregistrement de session",
        )?);
    }
    Ok(log)
}

/// Lit jusqu'à remplir `buffer` ou atteindre la fin ; renvoie le nombre d'octets lus.
fn read_fully(reader: &mut impl Read, buffer: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        match reader.read(&mut buffer[filled..])? {
            0 => break,
            read => filled += read,
        }
    }
    Ok(filled)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let unique: u64 = rand::random();
        std::env::temp_dir().join(format!("ssh-manager-sealed-log-{name}-{unique:x}.smlog"))
    }

    fn key() -> VaultKey {
        derive_log_key(&[9u8; KEY_LENGTH])
    }

    fn texts(log: &SealedLog) -> Vec<String> {
        log.records
            .iter()
            .map(|record| String::from_utf8(record.to_vec()).unwrap())
            .collect()
    }

    #[test]
    fn should_read_back_records_in_order() {
        let path = temp_path("order");
        let mut writer = SealedLogWriter::create(&path, key()).unwrap();
        writer.append(b"first").unwrap();
        writer.append(b"second").unwrap();
        let log = read(&path, &key(), None).unwrap();
        assert_eq!(texts(&log), ["first", "second"]);
        assert!(!log.truncated);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn should_not_store_the_plaintext() {
        let path = temp_path("plaintext");
        let mut writer = SealedLogWriter::create(&path, key()).unwrap();
        writer.append(b"root password is hunter2").unwrap();
        let raw = std::fs::read(&path).unwrap();
        assert!(!raw.windows(7).any(|window| window == b"hunter2"));
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn should_stop_at_the_requested_number_of_records() {
        let path = temp_path("limit");
        let mut writer = SealedLogWriter::create(&path, key()).unwrap();
        writer.append(b"header").unwrap();
        writer.append(b"body").unwrap();
        assert_eq!(texts(&read(&path, &key(), Some(1)).unwrap()), ["header"]);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn should_keep_complete_records_when_the_last_one_is_truncated() {
        let path = temp_path("truncated");
        let mut writer = SealedLogWriter::create(&path, key()).unwrap();
        writer.append(b"kept").unwrap();
        writer.append(b"lost in a crash").unwrap();
        let raw = std::fs::read(&path).unwrap();
        std::fs::write(&path, &raw[..raw.len() - 5]).unwrap();
        let log = read(&path, &key(), None).unwrap();
        assert_eq!(texts(&log), ["kept"]);
        assert!(log.truncated);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn should_reject_another_vault_key() {
        let path = temp_path("wrong-key");
        let mut writer = SealedLogWriter::create(&path, key()).unwrap();
        writer.append(b"secret").unwrap();
        let other = derive_log_key(&[1u8; KEY_LENGTH]);
        assert!(read(&path, &other, None).is_err());
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn should_derive_a_key_distinct_from_the_vault_key() {
        let vault_key = [9u8; KEY_LENGTH];
        assert_ne!(*derive_log_key(&vault_key), vault_key);
    }
}
