//! Sauvegardes exportables (`.enc` chiffré, `.json` en clair), au format de
//! l'application Electron : une sauvegarde faite par l'une se restaure dans
//! l'autre.
//!
//! ```text
//! .enc  { version: 2, encrypted: true, salt, iv, authTag, data }   (hex)
//!       data = AES-256-GCM(clé du coffre, iv de 16 octets, JSON du contenu)
//! .json { version: 1, connections, groups }                        (en clair)
//! ```
//!
//! La clé est celle du coffre, dérivée de la phrase de 12 mots et du sel écrit
//! dans le fichier : la sauvegarde se restaure sur n'importe quel poste avec
//! la phrase. Les empreintes d'hôte et les couleurs de groupe sont ajoutées
//! dans des champs que l'ancienne application ignore.

use std::collections::BTreeMap;

use aes_gcm::aead::consts::U16;
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::aes::Aes256;
use aes_gcm::{AesGcm, Nonce};
use serde::{Deserialize, Serialize};
use ssh_manager_core::ConnectionLibrary;

use crate::crypto::{self, KEY_LENGTH};
use crate::error::VaultError;
use crate::legacy::{LegacyConnection, convert_connection};
use crate::vault::VaultData;

/// L'application Electron utilise un IV de 16 octets (et non 12) avec GCM.
type BackupCipher = AesGcm<Aes256, U16>;
const IV_LENGTH: usize = 16;
const TAG_LENGTH: usize = 16;
const ENCRYPTED_VERSION: u32 = 2;
const PLAIN_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Contents {
    connections: Vec<LegacyConnection>,
    #[serde(default)]
    groups: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    group_colors: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    host_keys: BTreeMap<String, String>,
}

impl Contents {
    fn from_data(data: &VaultData) -> Self {
        Self {
            connections: data
                .library
                .connections
                .iter()
                .map(LegacyConnection::from_connection)
                .collect(),
            groups: data.library.groups.clone(),
            group_colors: data.library.group_colors.clone(),
            host_keys: data.host_keys.clone(),
        }
    }

    fn into_data(self) -> VaultData {
        let mut library = ConnectionLibrary::default();
        for group in &self.groups {
            library.create_group(group);
        }
        for connection in self.connections {
            library.upsert(convert_connection(connection));
        }
        library.group_colors = self.group_colors;
        VaultData {
            library,
            host_keys: self.host_keys,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EncryptedFile {
    version: u32,
    encrypted: bool,
    salt: String,
    iv: String,
    auth_tag: String,
    data: String,
}

#[derive(Serialize)]
struct PlainFile<'a> {
    version: u32,
    #[serde(flatten)]
    contents: &'a Contents,
}

/// Sauvegarde chiffrée avec la clé du coffre. `salt` est le sel du coffre :
/// avec la phrase, il suffit à retrouver la clé sur un autre poste.
pub fn export_encrypted(data: &VaultData, key: &[u8; KEY_LENGTH], salt: &[u8]) -> String {
    let plaintext = serde_json::to_vec(&Contents::from_data(data)).expect("contenu sérialisable");
    let iv = crypto::random_bytes::<IV_LENGTH>();
    let cipher = BackupCipher::new_from_slice(key).expect("clé de 32 octets");
    let mut sealed = cipher
        .encrypt(&Nonce::<U16>::from(iv), plaintext.as_slice())
        .expect("le chiffrement AES-GCM n'échoue que sur des tailles hors limites");
    // Node sépare l'étiquette d'authentification du texte chiffré.
    let tag = sealed.split_off(sealed.len() - TAG_LENGTH);
    let file = EncryptedFile {
        version: ENCRYPTED_VERSION,
        encrypted: true,
        salt: crypto::to_hex(salt),
        iv: crypto::to_hex(&iv),
        auth_tag: crypto::to_hex(&tag),
        data: crypto::to_hex(&sealed),
    };
    serde_json::to_string(&file).expect("fichier sérialisable")
}

/// Sauvegarde en clair : mots de passe lisibles par quiconque a le fichier.
pub fn export_plain(data: &VaultData) -> String {
    let contents = Contents::from_data(data);
    serde_json::to_string_pretty(&PlainFile {
        version: PLAIN_VERSION,
        contents: &contents,
    })
    .expect("fichier sérialisable")
}

/// Une sauvegarde chiffrée, en attente de sa clé.
pub struct EncryptedBackup {
    salt: Vec<u8>,
    iv: Vec<u8>,
    sealed: Vec<u8>,
}

pub enum Backup {
    Encrypted(EncryptedBackup),
    Plain(VaultData),
}

fn hex_field(value: &str, context: &'static str) -> Result<Vec<u8>, VaultError> {
    crypto::from_hex(value).ok_or(VaultError::Decryption { context })
}

/// Lit un fichier de sauvegarde, chiffré ou non.
pub fn parse(bytes: &[u8]) -> Result<Backup, VaultError> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|error| VaultError::format("sauvegarde", error))?;
    if value.get("encrypted").and_then(serde_json::Value::as_bool) == Some(true) {
        let file: EncryptedFile = serde_json::from_value(value)
            .map_err(|error| VaultError::format("sauvegarde", error))?;
        let mut sealed = hex_field(&file.data, "données de la sauvegarde")?;
        sealed.extend(hex_field(&file.auth_tag, "étiquette de la sauvegarde")?);
        return Ok(Backup::Encrypted(EncryptedBackup {
            salt: hex_field(&file.salt, "sel de la sauvegarde")?,
            iv: hex_field(&file.iv, "IV de la sauvegarde")?,
            sealed,
        }));
    }
    let contents: Contents =
        serde_json::from_value(value).map_err(|error| VaultError::format("sauvegarde", error))?;
    Ok(Backup::Plain(contents.into_data()))
}

impl EncryptedBackup {
    /// Vrai si la sauvegarde vient de ce coffre (même sel) : la clé en mémoire
    /// suffit alors, sans redemander la phrase.
    pub fn has_salt(&self, salt: &[u8]) -> bool {
        self.salt == salt
    }

    pub fn open_with_key(&self, key: &[u8; KEY_LENGTH]) -> Result<VaultData, VaultError> {
        let context = "sauvegarde (phrase incorrecte ou fichier corrompu)";
        let iv: [u8; IV_LENGTH] = self
            .iv
            .as_slice()
            .try_into()
            .map_err(|_| VaultError::Decryption { context })?;
        let cipher =
            BackupCipher::new_from_slice(key).map_err(|_| VaultError::Decryption { context })?;
        let plaintext = zeroize::Zeroizing::new(
            cipher
                .decrypt(
                    &Nonce::<U16>::from(iv),
                    Payload {
                        msg: &self.sealed,
                        aad: &[],
                    },
                )
                .map_err(|_| VaultError::Decryption { context })?,
        );
        let contents: Contents = serde_json::from_slice(&plaintext)
            .map_err(|error| VaultError::format("sauvegarde", error))?;
        Ok(contents.into_data())
    }

    /// Ouvre avec la phrase de 12 mots du coffre qui a fait la sauvegarde.
    pub fn open_with_phrase(&self, phrase: &str) -> Result<VaultData, VaultError> {
        let key = crypto::derive_key(phrase, &self.salt);
        self.open_with_key(&key)
    }
}

/// Ajoute les connexions d'une sauvegarde à la bibliothèque, comme l'ancienne
/// app : une connexion déjà présente (même identifiant) est conservée telle
/// quelle. Renvoie le nombre de connexions ajoutées.
pub fn merge_into(target: &mut VaultData, imported: VaultData) -> usize {
    for group in &imported.library.groups {
        target.library.create_group(group);
    }
    for (group, color) in imported.library.group_colors {
        target.library.group_colors.entry(group).or_insert(color);
    }
    for (host, fingerprint) in imported.host_keys {
        target.host_keys.entry(host).or_insert(fingerprint);
    }
    let mut added = 0;
    for connection in imported.library.connections {
        if target.library.find(&connection.id).is_none() {
            target.library.upsert(connection);
            added += 1;
        }
    }
    added
}

#[cfg(test)]
mod tests {
    use super::*;
    use ssh_manager_core::Connection;

    const PHRASE: &str =
        "abandon ability able about above absent absorb abstract absurd abuse access accident";

    fn sample() -> VaultData {
        let mut data = VaultData::default();
        data.library.upsert(Connection {
            id: "conn-1".into(),
            name: "web".into(),
            host: "10.0.0.5".into(),
            username: "deploy".into(),
            password: Some("s3cret".into()),
            group: Some("Prod".into()),
            ..Connection::new(1_700_000_000_000)
        });
        data.library
            .group_colors
            .insert("Prod".into(), "#ff8800".into());
        data.host_keys
            .insert("10.0.0.5:22".into(), "SHA256:abc".into());
        data
    }

    #[test]
    fn should_restore_an_encrypted_backup_with_the_recovery_phrase() {
        let salt = [7u8; 32];
        let key = crypto::derive_key(PHRASE, &salt);
        let file = export_encrypted(&sample(), &key, &salt);
        assert!(
            !file.contains("s3cret"),
            "le mot de passe n'apparaît pas en clair"
        );

        let Backup::Encrypted(backup) = parse(file.as_bytes()).unwrap() else {
            panic!("sauvegarde chiffrée attendue");
        };
        assert!(backup.has_salt(&salt));
        assert_eq!(backup.open_with_phrase(PHRASE).unwrap(), sample());
    }

    #[test]
    fn should_refuse_the_wrong_phrase() {
        let salt = [7u8; 32];
        let file = export_encrypted(&sample(), &crypto::derive_key(PHRASE, &salt), &salt);
        let Backup::Encrypted(backup) = parse(file.as_bytes()).unwrap() else {
            panic!("sauvegarde chiffrée attendue");
        };
        assert!(backup.open_with_phrase("wrong words entirely").is_err());
    }

    #[test]
    fn should_read_a_plain_backup_from_the_electron_app() {
        let json = br#"{"version":1,"connections":[{"id":"c1","name":"nas","type":"sftp","host":"nas.local","port":22,"username":"admin","password":"pw","group":"Maison","favorite":true,"createdAt":1}],"groups":["Maison","Vide"]}"#;
        let Backup::Plain(data) = parse(json).unwrap() else {
            panic!("sauvegarde en clair attendue");
        };
        let connection = data.library.find("c1").unwrap();
        assert_eq!(connection.kind, ssh_manager_core::ConnectionKind::Sftp);
        assert_eq!(connection.password.as_deref(), Some("pw"));
        assert_eq!(data.library.groups, ["Maison", "Vide"]);
    }

    #[test]
    fn should_round_trip_a_plain_backup() {
        let Backup::Plain(data) = parse(export_plain(&sample()).as_bytes()).unwrap() else {
            panic!("sauvegarde en clair attendue");
        };
        assert_eq!(data, sample());
    }

    #[test]
    fn should_add_only_unknown_connections_when_merging() {
        let mut target = sample();
        target.library.connections[0].name = "renamed locally".into();
        let mut imported = sample();
        imported.library.upsert(Connection {
            id: "conn-2".into(),
            name: "db".into(),
            host: "10.0.0.6".into(),
            username: "root".into(),
            ..Connection::new(0)
        });
        assert_eq!(merge_into(&mut target, imported), 1);
        assert_eq!(
            target.library.find("conn-1").unwrap().name,
            "renamed locally"
        );
        assert!(target.library.find("conn-2").is_some());
    }
}
