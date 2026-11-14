//! Import du coffre Electron, vérifié contre des fichiers produits par la
//! bibliothèque `conf` réelle (voir `fixtures/generate-electron-fixtures.cjs`).

use std::path::{Path, PathBuf};

use serde::Deserialize;
use ssh_manager_core::ConnectionKind;
use ssh_manager_vault::crypto;
use ssh_manager_vault::legacy::{LegacyVault, decrypt_conf_store};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Reference {
    mnemonic: String,
    salt_hex: String,
    key_hex: String,
    verify_hash: String,
    connection_count: usize,
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

fn reference() -> Reference {
    serde_json::from_slice(&std::fs::read(fixtures().join("electron-reference.json")).unwrap())
        .unwrap()
}

/// Reconstitue le dossier d'une app Electron : méta + connexions chiffrées.
fn legacy_dir(name: &str) -> PathBuf {
    let reference = reference();
    let dir =
        std::env::temp_dir().join(format!("ssh-manager-legacy-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(
        fixtures().join("electron-connections.json"),
        dir.join("ssh-manager-connections.json"),
    )
    .unwrap();
    let meta = serde_json::json!({
        "initialized": true,
        "salt": reference.salt_hex,
        "verifyHash": reference.verify_hash,
        "hostKeys": { "10.20.0.11:22": "SHA256:pinned" },
    });
    std::fs::write(
        dir.join("ssh-manager-meta.json"),
        serde_json::to_vec(&meta).unwrap(),
    )
    .unwrap();
    dir
}

#[test]
fn should_derive_the_same_key_as_the_electron_app() {
    let reference = reference();
    let salt = crypto::from_hex(&reference.salt_hex).unwrap();
    assert_eq!(
        crypto::to_hex(crypto::derive_key(&reference.mnemonic, &salt).as_ref()),
        reference.key_hex
    );
}

#[test]
fn should_compute_the_same_verification_hash_as_the_electron_app() {
    let reference = reference();
    let salt = crypto::from_hex(&reference.salt_hex).unwrap();
    assert_eq!(
        crypto::phrase_verification_hash(&reference.mnemonic, &salt),
        reference.verify_hash
    );
}

#[test]
fn should_decrypt_every_conf_store_regardless_of_its_random_iv() {
    let reference = reference();
    let mut checked = 0;
    for entry in std::fs::read_dir(fixtures()).unwrap() {
        let path = entry.unwrap().path();
        if !path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("electron-iv-")
        {
            continue;
        }
        let json = decrypt_conf_store(&std::fs::read(&path).unwrap(), &reference.key_hex)
            .unwrap_or_else(|error| panic!("{} : {error}", path.display()));
        let value: serde_json::Value = serde_json::from_slice(&json).unwrap();
        assert!(value["marker"].is_number(), "{}", path.display());
        checked += 1;
    }
    assert_eq!(checked, 24);
}

#[test]
fn should_import_connections_with_the_recovery_phrase() {
    let reference = reference();
    let legacy = LegacyVault::locate(legacy_dir("phrase")).expect("coffre initialisé");
    let key = legacy
        .recover_key_with_phrase(&reference.mnemonic.to_uppercase())
        .unwrap();
    let import = legacy.read(Some(key)).unwrap();

    let library = &import.data.library;
    assert_eq!(library.connections.len(), reference.connection_count);
    assert_eq!(
        library.groups,
        vec!["Production".to_string(), "Sauvegardes".to_string()]
    );

    let web = library.find("conn-1").unwrap();
    assert_eq!(web.kind, ConnectionKind::Ssh);
    assert_eq!(web.password.as_deref(), Some("Pa55-wörd!"));
    assert!(web.favorite);
    assert_eq!(web.last_used_at, Some(1_755_650_000_000));

    let ftp = library.find("conn-3").unwrap();
    assert_eq!(
        (ftp.kind, ftp.port, ftp.ftp_secure),
        (ConnectionKind::Ftp, 21, true)
    );
    assert_eq!(
        import
            .data
            .host_keys
            .get("10.20.0.11:22")
            .map(String::as_str),
        Some("SHA256:pinned")
    );
}

#[test]
fn should_reject_a_wrong_recovery_phrase() {
    let legacy = LegacyVault::locate(legacy_dir("wrong")).unwrap();
    assert!(
        legacy
            .recover_key_with_phrase("abandon abandon abandon")
            .is_err()
    );
}

#[test]
fn should_ignore_a_folder_without_an_initialized_vault() {
    let dir = std::env::temp_dir().join(format!("ssh-manager-legacy-empty-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    assert!(LegacyVault::locate(dir).is_none());
}
