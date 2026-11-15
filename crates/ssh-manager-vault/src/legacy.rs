//! Lecture du coffre de l'application Electron, pour la migration.
//!
//! Le coffre Electron est réparti en deux fichiers dans `%APPDATA%\ssh-manager` :
//!
//! - `ssh-manager-meta.json`, en clair : sel, empreinte de contrôle de la
//!   phrase, et une copie de la clé protégée par `safeStorage` ;
//! - `ssh-manager-connections.json`, chiffré par la bibliothèque `conf`.
//!
//! La clé peut être retrouvée de deux façons : automatiquement, en déchiffrant
//! la copie `safeStorage` (voir [`os_crypt`]) ; ou à partir de la phrase de
//! récupération. Ce module ne modifie jamais les fichiers de l'ancienne app.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use aes::Aes256;
use cbc::cipher::{BlockModeDecrypt, KeyIvInit, block_padding::Pkcs7};
use serde::{Deserialize, Serialize};
use sha2::Sha512;
use ssh_manager_core::{Connection, ConnectionKind, ConnectionLibrary};
use zeroize::Zeroizing;

use crate::crypto::{self, KEY_LENGTH, VaultKey};
use crate::error::VaultError;
use crate::vault::VaultData;

const META_FILE: &str = "ssh-manager-meta.json";
const CONNECTIONS_FILE: &str = "ssh-manager-connections.json";

/// Paramètres de chiffrement de la bibliothèque `conf` 10.x.
const CONF_IV_LENGTH: usize = 16;
const CONF_SEPARATOR: u8 = b':';
const CONF_KEY_DERIVATION_ROUNDS: u32 = 10_000;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyMeta {
    #[serde(default)]
    initialized: bool,
    #[serde(default)]
    encryption_disabled: bool,
    salt: Option<String>,
    verify_hash: Option<String>,
    encrypted_key: Option<String>,
    #[serde(default)]
    host_keys: BTreeMap<String, String>,
}

/// Une connexion au format de l'application Electron. Sert aussi aux
/// sauvegardes `.enc`, pour qu'elles restent lisibles par les deux applications.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LegacyConnection {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(rename = "type")]
    kind: ConnectionKind,
    host: String,
    #[serde(default)]
    port: Option<u16>,
    #[serde(default)]
    username: String,
    password: Option<String>,
    private_key_path: Option<String>,
    /// Absent des fichiers Electron ; transporté par les sauvegardes de la version Rust.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    key_passphrase: Option<String>,
    #[serde(default)]
    ftp_secure: Option<bool>,
    group: Option<String>,
    #[serde(default)]
    favorite: Option<bool>,
    color: Option<String>,
    #[serde(default)]
    created_at: Option<f64>,
    #[serde(default)]
    last_used_at: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct LegacyStore {
    #[serde(default)]
    connections: Vec<LegacyConnection>,
}

/// Coffre Electron trouvé sur le poste.
pub struct LegacyVault {
    dir: PathBuf,
    meta: LegacyMeta,
}

/// Résultat d'une migration réussie.
pub struct LegacyImport {
    pub data: VaultData,
    /// `None` si l'ancien coffre n'était pas chiffré (mode portable sans chiffrement).
    pub key: Option<VaultKey>,
    pub salt: Option<Vec<u8>>,
    pub verify_hash: Option<String>,
}

impl LegacyVault {
    /// Cherche un coffre Electron initialisé dans `dir`.
    pub fn locate(dir: impl Into<PathBuf>) -> Option<Self> {
        let dir = dir.into();
        let bytes = std::fs::read(dir.join(META_FILE)).ok()?;
        let meta: LegacyMeta = match serde_json::from_slice(&bytes) {
            Ok(meta) => meta,
            Err(error) => {
                tracing::warn!(dir = %dir.display(), %error, "legacy vault metadata is unreadable — migration skipped");
                return None;
            }
        };
        meta.initialized.then_some(Self { dir, meta })
    }

    pub fn is_encrypted(&self) -> bool {
        !self.meta.encryption_disabled
    }

    fn salt(&self) -> Result<Vec<u8>, VaultError> {
        let salt = self
            .meta
            .salt
            .as_deref()
            .ok_or(VaultError::IncompleteLegacyVault("sel absent"))?;
        crypto::from_hex(salt).ok_or(VaultError::IncompleteLegacyVault("sel illisible"))
    }

    /// Retrouve la clé sans intervention, via la copie protégée par `safeStorage`.
    #[cfg(windows)]
    pub fn recover_key_automatically(&self) -> Result<VaultKey, VaultError> {
        let protected = self
            .meta
            .encrypted_key
            .as_deref()
            .ok_or(VaultError::IncompleteLegacyVault("clé protégée absente"))?;
        let key_hex = os_crypt::decrypt_safe_storage(&self.dir, protected)?;
        key_from_hex(&key_hex)
    }

    /// Retrouve la clé à partir de la phrase de récupération.
    pub fn recover_key_with_phrase(&self, phrase: &str) -> Result<VaultKey, VaultError> {
        let salt = self.salt()?;
        let expected = self
            .meta
            .verify_hash
            .as_deref()
            .ok_or(VaultError::IncompleteLegacyVault("empreinte absente"))?;
        let actual = crypto::phrase_verification_hash(phrase, &salt);
        if !crypto::constant_time_eq(actual.as_bytes(), expected.as_bytes()) {
            return Err(VaultError::WrongRecoveryPhrase);
        }
        Ok(crypto::derive_key(phrase, &salt))
    }

    /// Lit les connexions. `key` est requise si le coffre est chiffré.
    pub fn read(&self, key: Option<VaultKey>) -> Result<LegacyImport, VaultError> {
        let path = self.dir.join(CONNECTIONS_FILE);
        let raw = match std::fs::read(&path) {
            Ok(raw) => raw,
            // Aucune connexion jamais enregistrée : coffre vide, pas une erreur.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(VaultError::io(&path, error)),
        };

        let json = match (&key, raw.is_empty()) {
            (_, true) => Zeroizing::new(b"{}".to_vec()),
            (Some(key), false) => decrypt_conf_store(&raw, &crypto::to_hex(key.as_ref()))?,
            (None, false) => Zeroizing::new(raw),
        };
        let store: LegacyStore =
            serde_json::from_slice(&json).map_err(|error| VaultError::format(&path, error))?;

        let library = build_library(store.connections);
        tracing::info!(
            dir = %self.dir.display(),
            connections = library.connections.len(),
            groups = library.groups.len(),
            host_keys = self.meta.host_keys.len(),
            "legacy vault read"
        );

        let (salt, verify_hash) = match key {
            Some(_) => (Some(self.salt()?), self.meta.verify_hash.clone()),
            None => (None, None),
        };
        Ok(LegacyImport {
            data: VaultData {
                library,
                host_keys: self.meta.host_keys.clone(),
            },
            key,
            salt,
            verify_hash,
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

fn key_from_hex(key_hex: &str) -> Result<VaultKey, VaultError> {
    let bytes = Zeroizing::new(
        crypto::from_hex(key_hex.trim()).ok_or(VaultError::Decryption {
            context: "clé de l'ancien coffre",
        })?,
    );
    if bytes.len() != KEY_LENGTH {
        return Err(VaultError::Decryption {
            context: "clé de l'ancien coffre",
        });
    }
    let mut key = Zeroizing::new([0u8; KEY_LENGTH]);
    key.copy_from_slice(&bytes);
    Ok(key)
}

/// Les groupes de l'app Electron vivaient dans son localStorage ; on les
/// reconstitue à partir des connexions, dans l'ordre de première apparition.
fn build_library(connections: Vec<LegacyConnection>) -> ConnectionLibrary {
    let mut library = ConnectionLibrary::default();
    for legacy in connections {
        library.upsert(convert_connection(legacy));
    }
    library
}

impl LegacyConnection {
    pub(crate) fn from_connection(connection: &Connection) -> Self {
        Self {
            id: connection.id.clone(),
            name: connection.name.clone(),
            kind: connection.kind,
            host: connection.host.clone(),
            port: Some(connection.port),
            username: connection.username.clone(),
            password: connection.password.clone(),
            private_key_path: connection.private_key_path.clone(),
            key_passphrase: connection.key_passphrase.clone(),
            ftp_secure: Some(connection.ftp_secure),
            group: connection.group.clone(),
            favorite: Some(connection.favorite),
            color: connection.color.clone(),
            created_at: Some(connection.created_at as f64),
            last_used_at: connection.last_used_at.map(|value| value as f64),
        }
    }
}

pub(crate) fn convert_connection(legacy: LegacyConnection) -> Connection {
    Connection {
        id: legacy.id,
        name: if legacy.name.is_empty() {
            legacy.host.clone()
        } else {
            legacy.name
        },
        kind: legacy.kind,
        port: legacy.port.unwrap_or_else(|| legacy.kind.default_port()),
        host: legacy.host,
        username: legacy.username,
        password: legacy.password.filter(|password| !password.is_empty()),
        private_key_path: legacy.private_key_path.filter(|path| !path.is_empty()),
        key_passphrase: legacy
            .key_passphrase
            .filter(|passphrase| !passphrase.is_empty()),
        ftp_secure: legacy.ftp_secure.unwrap_or(false),
        group: legacy.group.filter(|group| !group.is_empty()),
        favorite: legacy.favorite.unwrap_or(false),
        color: legacy.color.filter(|color| !color.is_empty()),
        created_at: legacy.created_at.map(|value| value as i64).unwrap_or(0),
        last_used_at: legacy.last_used_at.map(|value| value as i64),
    }
}

/// Déchiffre un fichier écrit par `conf` 10.x avec `encryptionKey`.
///
/// Format : `IV (16 octets) ':' AES-256-CBC(JSON)`. La clé AES est
/// `PBKDF2-SHA512(encryptionKey, sel, 10 000, 32)` où le sel est **l'IV relu
/// comme texte UTF-8** (`initializationVector.toString()` en Node) : les octets
/// invalides y deviennent U+FFFD. `String::from_utf8_lossy` applique la même
/// règle de remplacement (« sous-partie maximale »), ce que les fixtures
/// vérifient sur une série d'IV aléatoires.
pub fn decrypt_conf_store(
    raw: &[u8],
    encryption_key: &str,
) -> Result<Zeroizing<Vec<u8>>, VaultError> {
    const CONTEXT: &str = "connexions de l'ancien coffre";
    if raw.len() <= CONF_IV_LENGTH || raw[CONF_IV_LENGTH] != CONF_SEPARATOR {
        return Err(VaultError::Decryption { context: CONTEXT });
    }
    let iv = &raw[..CONF_IV_LENGTH];
    let salt = String::from_utf8_lossy(iv);

    let mut aes_key = Zeroizing::new([0u8; 32]);
    pbkdf2::pbkdf2_hmac::<Sha512>(
        encryption_key.as_bytes(),
        salt.as_bytes(),
        CONF_KEY_DERIVATION_ROUNDS,
        aes_key.as_mut(),
    );

    let decryptor = cbc::Decryptor::<Aes256>::new_from_slices(aes_key.as_ref(), iv)
        .map_err(|_| VaultError::Decryption { context: CONTEXT })?;
    decryptor
        .decrypt_padded_vec::<Pkcs7>(&raw[CONF_IV_LENGTH + 1..])
        .map(Zeroizing::new)
        .map_err(|_| VaultError::Decryption { context: CONTEXT })
}

/// Déchiffrement de `safeStorage` d'Electron sous Windows (Chromium OSCrypt).
#[cfg(windows)]
pub mod os_crypt {
    use std::path::Path;

    use base64::Engine;
    use base64::engine::general_purpose::STANDARD as BASE64;
    use serde::Deserialize;
    use zeroize::Zeroizing;

    use crate::crypto;
    use crate::error::VaultError;

    const LOCAL_STATE_FILE: &str = "Local State";
    /// Préfixe ajouté par Chromium devant la clé protégée par DPAPI.
    const DPAPI_PREFIX: &[u8] = b"DPAPI";
    /// Préfixe des chaînes chiffrées en AES-256-GCM par OSCrypt.
    const V10_PREFIX: &[u8] = b"v10";
    const NONCE_LENGTH: usize = 12;

    #[derive(Deserialize)]
    struct LocalState {
        os_crypt: OsCrypt,
    }

    #[derive(Deserialize)]
    struct OsCrypt {
        encrypted_key: String,
    }

    /// `safeStorage.encryptString` sous Windows : `v10 | nonce (12) | AES-256-GCM`,
    /// avec une clé AES stockée dans `Local State`, elle-même protégée par DPAPI.
    pub fn decrypt_safe_storage(
        dir: &Path,
        protected_base64: &str,
    ) -> Result<Zeroizing<String>, VaultError> {
        const CONTEXT: &str = "clé protégée de l'ancien coffre";
        let aes_key = read_os_crypt_key(dir)?;
        let blob = BASE64
            .decode(protected_base64)
            .map_err(|_| VaultError::Decryption { context: CONTEXT })?;
        let body = blob
            .strip_prefix(V10_PREFIX)
            .ok_or(VaultError::Decryption { context: CONTEXT })?;
        if body.len() < NONCE_LENGTH {
            return Err(VaultError::Decryption { context: CONTEXT });
        }
        let (nonce, ciphertext) = body.split_at(NONCE_LENGTH);
        let key: [u8; crypto::KEY_LENGTH] = aes_key
            .as_slice()
            .try_into()
            .map_err(|_| VaultError::Decryption { context: CONTEXT })?;
        let plaintext = crypto::open(&key, nonce, ciphertext, CONTEXT)?;
        let text = String::from_utf8(plaintext.to_vec())
            .map_err(|_| VaultError::Decryption { context: CONTEXT })?;
        Ok(Zeroizing::new(text))
    }

    fn read_os_crypt_key(dir: &Path) -> Result<Zeroizing<Vec<u8>>, VaultError> {
        let path = dir.join(LOCAL_STATE_FILE);
        let bytes = std::fs::read(&path).map_err(|error| VaultError::io(&path, error))?;
        let state: LocalState =
            serde_json::from_slice(&bytes).map_err(|error| VaultError::format(&path, error))?;
        let blob =
            BASE64
                .decode(state.os_crypt.encrypted_key)
                .map_err(|_| VaultError::Decryption {
                    context: "clé OSCrypt",
                })?;
        let protected = blob
            .strip_prefix(DPAPI_PREFIX)
            .ok_or(VaultError::Decryption {
                context: "clé OSCrypt",
            })?;
        crate::dpapi::unprotect(protected)
    }
}
