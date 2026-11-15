//! Restauration d'une sauvegarde `.enc` produite par le code d'export de
//! l'application Electron (voir `fixtures/generate-electron-backup.cjs`).

use std::path::Path;

use ssh_manager_core::ConnectionKind;
use ssh_manager_vault::backup::{self, Backup};

const MNEMONIC: &str =
    "abandon ability able about above absent absorb abstract absurd abuse access accident";

fn electron_backup() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("electron-backup.enc");
    std::fs::read(path).unwrap()
}

#[test]
fn should_restore_a_backup_exported_by_the_electron_app() {
    let Backup::Encrypted(encrypted) = backup::parse(&electron_backup()).unwrap() else {
        panic!("sauvegarde chiffrée attendue");
    };
    let data = encrypted.open_with_phrase(MNEMONIC).unwrap();

    let web = data.library.find("conn-1").unwrap();
    assert_eq!(web.password.as_deref(), Some("Pa55-wörd!"));
    assert_eq!(web.group.as_deref(), Some("Production"));
    assert!(web.favorite);
    let ftp = data.library.find("conn-2").unwrap();
    assert_eq!(ftp.kind, ConnectionKind::Ftp);
    assert!(ftp.ftp_secure);
    assert!(data.library.groups.contains(&"Vide".to_owned()));
}

#[test]
fn should_accept_the_phrase_typed_with_capitals_and_spaces() {
    let Backup::Encrypted(encrypted) = backup::parse(&electron_backup()).unwrap() else {
        panic!("sauvegarde chiffrée attendue");
    };
    let typed = format!("  {}  ", MNEMONIC.to_uppercase());
    assert!(encrypted.open_with_phrase(&typed).is_ok());
}
