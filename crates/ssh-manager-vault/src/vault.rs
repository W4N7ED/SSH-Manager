//! Le coffre de la nouvelle application : un fichier JSON dont tout le contenu
//! utile est chiffré et authentifié.
//!
//! ```text
//! vault.json
//! ├─ version, salt, verifyHash      en clair : nécessaires pour vérifier la phrase
//! ├─ protectedKey(s)                 clé du coffre protégée par DPAPI, une
//! │                                  copie par poste et compte Windows
//! │                                  (version portable sur clé USB)
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
    /// Copie protégée pour le dernier poste ayant enregistré (format d'origine).
    protected_key: String,
    /// Une copie protégée par poste : `utilisateur@POSTE` → blob DPAPI.
    /// DPAPI lie un blob à un compte d'un poste ; garder celles des autres
    /// postes évite de redemander la phrase en revenant sur l'un d'eux.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    protected_keys: BTreeMap<String, String>,
    nonce: String,
    payload: String,
}

/// Identifiant du compte Windows courant, celui auquel DPAPI lie la clé.
fn machine_account() -> String {
    let user = std::env::var("USERNAME").unwrap_or_default();
    let computer = std::env::var("COMPUTERNAME").unwrap_or_default();
    format!("{user}@{computer}")
}

/// Coffre ouvert : la clé est en mémoire (effacée à la libération).
pub struct Vault {
    path: PathBuf,
    key: VaultKey,
    salt: Vec<u8>,
    verify_hash: String,
    /// Clés protégées des autres postes, conservées à chaque enregistrement.
    other_accounts: BTreeMap<String, String>,
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
            other_accounts: BTreeMap::new(),
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
            other_accounts: BTreeMap::new(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Sauvegarde `.enc` de `data`, restaurable ailleurs avec la phrase de 12 mots.
    pub fn export_backup(&self, data: &VaultData) -> String {
        crate::backup::export_encrypted(data, &self.key, &self.salt)
    }

    /// Ouvre une sauvegarde faite par ce même coffre, sans redemander la
    /// phrase. `None` si elle vient d'un autre coffre (la phrase est alors requise).
    pub fn open_own_backup(
        &self,
        backup: &crate::backup::EncryptedBackup,
    ) -> Option<Result<VaultData, VaultError>> {
        backup
            .has_salt(&self.salt)
            .then(|| backup.open_with_key(&self.key))
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
        // D'abord la copie de ce compte, puis la copie d'origine.
        let key = file
            .protected_keys
            .get(&machine_account())
            .into_iter()
            .chain(std::iter::once(&file.protected_key))
            .find_map(|protected| unprotect_key(protected).ok())
            .ok_or(VaultError::RecoveryPhraseRequired)?;
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
        let mut other_accounts = file.protected_keys.clone();
        other_accounts.remove(&machine_account());
        Ok(Self {
            path,
            key,
            salt,
            verify_hash: file.verify_hash.clone(),
            other_accounts,
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
        let protected_key = protect_key(&self.key)?;
        let mut protected_keys = self.other_accounts.clone();
        protected_keys.insert(machine_account(), protected_key.clone());
        let file = VaultFile {
            version: FORMAT_VERSION,
            salt: crypto::to_hex(&self.salt),
            verify_hash: self.verify_hash.clone(),
            protected_key,
            protected_keys,
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

    /// Simule un coffre portable déjà ouvert sur un autre poste : sa copie de
    /// clé (illisible ici) doit survivre aux enregistrements faits sur ce poste.
    #[test]
    fn should_keep_the_protected_keys_of_other_computers() {
        let path = temp_vault_path("portable");
        Vault::create(&path, PHRASE).save(&sample_data()).unwrap();
        let mut file: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        file["protectedKeys"]["alice@LAPTOP"] = "b3RoZXItcG9zdGU=".into();
        std::fs::write(&path, serde_json::to_vec(&file).unwrap()).unwrap();

        let (vault, data) = Vault::open(&path).unwrap();
        vault.save(&data).unwrap();

        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["protectedKeys"]["alice@LAPTOP"], "b3RoZXItcG9zdGU=");
        assert!(saved["protectedKeys"][machine_account()].is_string());
    }

    #[test]
    fn should_ask_for_the_phrase_when_no_copy_belongs_to_this_computer() {
        let path = temp_vault_path("foreign");
        Vault::create(&path, PHRASE).save(&sample_data()).unwrap();
        let mut file: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        file["protectedKey"] = "aWxsaXNpYmxl".into();
        file["protectedKeys"] = serde_json::json!({ "bob@DESKTOP": "aWxsaXNpYmxl" });
        std::fs::write(&path, serde_json::to_vec(&file).unwrap()).unwrap();
        assert!(matches!(
            Vault::open(&path),
            Err(VaultError::RecoveryPhraseRequired)
        ));
        // La phrase ouvre le coffre et ajoute la copie de ce poste.
        Vault::open_with_phrase(&path, PHRASE).unwrap();
        assert!(Vault::open(&path).is_ok());
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
