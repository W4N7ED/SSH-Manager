//! Le coffre de la nouvelle application : un fichier JSON dont tout le contenu
//! utile est chiffré et authentifié.
//!
//! ```text
//! vault.json
//! ├─ version, salt, verifyHash      en clair : nécessaires pour vérifier la phrase
//! ├─ protectedKey                    clé du coffre protégée par DPAPI
//! └─ nonce, payload                  VaultData chiffré en AES-256-GCM
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::{Deserialize, Serialize};
use ssh_manager_core::ConnectionLibrary;
use zeroize::Zeroizing;

use crate::crypto::{self, KEY_LENGTH, SALT_LENGTH, VaultKey};
use crate::error::VaultError;

const FORMAT_VERSION: u32 = 1;

/// Contenu déchiffré du coffre.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultData {
    pub library: ConnectionLibrary,
    /// Empreinte SHA-256 épinglée de chaque serveur SSH, indexée par `hôte:port`.
    /// Dans la partie chiffrée : une empreinte altérée équivaudrait à accepter
    /// un serveur usurpé, son intégrité doit donc être authentifiée.
    #[serde(default)]
    pub host_keys: BTreeMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VaultFile {
    version: u32,
    salt: String,
    verify_hash: String,
    protected_key: String,
    nonce: String,
    payload: String,
}

/// Coffre ouvert : la clé est en mémoire (effacée à la libération).
pub struct Vault {
    path: PathBuf,
    key: VaultKey,
    salt: Vec<u8>,
    verify_hash: String,
}

impl Vault {
    /// Crée un coffre neuf protégé par `phrase`.
    pub fn create(path: impl Into<PathBuf>, phrase: &str) -> Self {
        let salt = crypto::random_bytes::<SALT_LENGTH>().to_vec();
        let key = crypto::derive_key(phrase, &salt);
        let verify_hash = crypto::phrase_verification_hash(phrase, &salt);
        Self {
            path: path.into(),
            key,
            salt,
            verify_hash,
        }
    }

    /// Crée un coffre à partir d'une clé déjà dérivée — c'est le cas de la
    /// migration : le sel et l'empreinte de l'ancien coffre sont repris, si bien
    /// que la phrase de récupération d'origine reste valable.
    pub fn from_existing_key(
        path: impl Into<PathBuf>,
        key: VaultKey,
        salt: Vec<u8>,
        verify_hash: String,
    ) -> Self {
        Self {
            path: path.into(),
            key,
            salt,
            verify_hash,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Clé des enregistrements de session, dérivée de la clé du coffre : les
    /// enregistrements ne sont lisibles qu'une fois le coffre ouvert.
    pub fn recording_key(&self) -> VaultKey {
        crate::sealed_log::derive_log_key(&self.key)
    }

    /// Ouvre le coffre sans intervention, grâce à la clé protégée par DPAPI.
    /// Renvoie [`VaultError::RecoveryPhraseRequired`] si la clé n'est pas lisible
    /// sur ce poste.
    pub fn open(path: impl Into<PathBuf>) -> Result<(Self, VaultData), VaultError> {
        let path = path.into();
        let file = read_file(&path)?;
        let key = unprotect_key(&file.protected_key)?;
        let vault = Self::from_file(path, key, &file)?;
        let data = vault.decrypt(&file)?;
        Ok((vault, data))
    }

    /// Ouvre le coffre avec la phrase de récupération, puis re-protège la clé
    /// pour ce poste afin que les lancements suivants soient automatiques.
    pub fn open_with_phrase(
        path: impl Into<PathBuf>,
        phrase: &str,
    ) -> Result<(Self, VaultData), VaultError> {
        let path = path.into();
        let file = read_file(&path)?;
        let salt = decode_hex(&file.salt, "sel du coffre")?;
        let expected = crypto::phrase_verification_hash(phrase, &salt);
        if !crypto::constant_time_eq(expected.as_bytes(), file.verify_hash.as_bytes()) {
            return Err(VaultError::WrongRecoveryPhrase);
        }
        let key = crypto::derive_key(phrase, &salt);
        let vault = Self::from_file(path, key, &file)?;
        let data = vault.decrypt(&file)?;
        vault.save(&data)?;
        Ok((vault, data))
    }

    fn from_file(path: PathBuf, key: VaultKey, file: &VaultFile) -> Result<Self, VaultError> {
        if file.version != FORMAT_VERSION {
            return Err(VaultError::UnsupportedVersion(file.version));
        }
        let salt = decode_hex(&file.salt, "sel du coffre")?;
        Ok(Self {
            path,
            key,
            salt,
            verify_hash: file.verify_hash.clone(),
        })
    }

    fn decrypt(&self, file: &VaultFile) -> Result<VaultData, VaultError> {
        let nonce = decode_base64(&file.nonce, "nonce du coffre")?;
        let payload = decode_base64(&file.payload, "contenu du coffre")?;
        let plaintext = crypto::open(&self.key, &nonce, &payload, "contenu du coffre")?;
        serde_json::from_slice(&plaintext).map_err(|error| VaultError::format(&self.path, error))
    }

    /// Chiffre et écrit le coffre. L'écriture passe par un fichier temporaire
    /// renommé ensuite : une coupure en cours d'écriture ne peut pas laisser un
    /// coffre à moitié écrit.
    pub fn save(&self, data: &VaultData) -> Result<(), VaultError> {
        let plaintext = Zeroizing::new(
            serde_json::to_vec(data).map_err(|error| VaultError::format(&self.path, error))?,
        );
        let (nonce, payload) = crypto::seal(&self.key, &plaintext);
        let file = VaultFile {
            version: FORMAT_VERSION,
            salt: crypto::to_hex(&self.salt),
            verify_hash: self.verify_hash.clone(),
            protected_key: protect_key(&self.key)?,
            nonce: BASE64.encode(nonce),
            payload: BASE64.encode(payload),
        };
        let json = serde_json::to_vec_pretty(&file)
            .map_err(|error| VaultError::format(&self.path, error))?;
        write_atomically(&self.path, &json)?;
        tracing::info!(path = %self.path.display(), connections = data.library.connections.len(), "vault saved");
        Ok(())
    }
}

fn read_file(path: &Path) -> Result<VaultFile, VaultError> {
    let bytes = std::fs::read(path).map_err(|error| VaultError::io(path, error))?;
    serde_json::from_slice(&bytes).map_err(|error| VaultError::format(path, error))
}

fn write_atomically(path: &Path, contents: &[u8]) -> Result<(), VaultError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| VaultError::io(parent, error))?;
    }
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, contents).map_err(|error| VaultError::io(&temporary, error))?;
    std::fs::rename(&temporary, path).map_err(|error| VaultError::io(path, error))
}

fn decode_hex(text: &str, context: &'static str) -> Result<Vec<u8>, VaultError> {
    crypto::from_hex(text).ok_or(VaultError::Decryption { context })
}

fn decode_base64(text: &str, context: &'static str) -> Result<Vec<u8>, VaultError> {
    BASE64
        .decode(text)
        .map_err(|_| VaultError::Decryption { context })
}

#[cfg(windows)]
fn protect_key(key: &[u8; KEY_LENGTH]) -> Result<String, VaultError> {
    Ok(BASE64.encode(crate::dpapi::protect(key)?))
}

#[cfg(windows)]
fn unprotect_key(protected: &str) -> Result<VaultKey, VaultError> {
    let blob = decode_base64(protected, "clé protégée")?;
    let bytes = crate::dpapi::unprotect(&blob).map_err(|_| VaultError::RecoveryPhraseRequired)?;
    let mut key = Zeroizing::new([0u8; KEY_LENGTH]);
    if bytes.len() != KEY_LENGTH {
        return Err(VaultError::RecoveryPhraseRequired);
    }
    key.copy_from_slice(&bytes);
    Ok(key)
}

// Hors Windows, pas de DPAPI : la phrase est demandée à chaque ouverture.
#[cfg(not(windows))]
fn protect_key(_key: &[u8; KEY_LENGTH]) -> Result<String, VaultError> {
    Ok(String::new())
}

#[cfg(not(windows))]
fn unprotect_key(_protected: &str) -> Result<VaultKey, VaultError> {
    Err(VaultError::RecoveryPhraseRequired)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ssh_manager_core::Connection;

    const PHRASE: &str =
        "abandon ability able about above absent absorb abstract absurd abuse access accident";

    fn sample_data() -> VaultData {
        let mut data = VaultData::default();
        data.library.upsert(Connection {
            id: "conn-test".into(),
            name: "prod".into(),
            host: "10.0.0.1".into(),
            username: "root".into(),
            password: Some("s3cr3t".into()),
            ..Connection::new(0)
        });
        data.host_keys
            .insert("10.0.0.1:22".into(), "SHA256:abc".into());
        data
    }

    fn temp_vault_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ssh-manager-vault-test-{name}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("vault.json")
    }

    #[test]
    fn should_reopen_automatically_after_saving() {
        let path = temp_vault_path("auto");
        Vault::create(&path, PHRASE).save(&sample_data()).unwrap();
        let (_, data) = Vault::open(&path).unwrap();
        assert_eq!(data, sample_data());
    }

    #[test]
    fn should_never_write_secrets_in_clear() {
        let path = temp_vault_path("clear");
        Vault::create(&path, PHRASE).save(&sample_data()).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("s3cr3t") && !raw.contains("10.0.0.1"));
    }

    #[test]
    fn should_open_with_the_recovery_phrase() {
        let path = temp_vault_path("phrase");
        Vault::create(&path, PHRASE).save(&sample_data()).unwrap();
        let (_, data) = Vault::open_with_phrase(&path, &PHRASE.to_uppercase()).unwrap();
        assert_eq!(data, sample_data());
    }

    #[test]
    fn should_reject_a_wrong_recovery_phrase() {
        let path = temp_vault_path("wrong");
        Vault::create(&path, PHRASE).save(&sample_data()).unwrap();
        let result = Vault::open_with_phrase(&path, "abandon abandon abandon");
        assert!(matches!(result, Err(VaultError::WrongRecoveryPhrase)));
    }

    #[test]
    fn should_detect_a_tampered_payload() {
        let path = temp_vault_path("tamper");
        Vault::create(&path, PHRASE).save(&sample_data()).unwrap();
        let mut file: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let payload = file["payload"].as_str().unwrap().to_owned();
        let mut bytes = BASE64.decode(payload).unwrap();
        bytes[0] ^= 1;
        file["payload"] = BASE64.encode(bytes).into();
        std::fs::write(&path, serde_json::to_vec(&file).unwrap()).unwrap();
        assert!(matches!(
            Vault::open(&path),
            Err(VaultError::Decryption { .. })
        ));
    }
}
