//! Décision prise au lancement : quel écran afficher.
//!
//! ```text
//! coffre présent ?
//! ├─ oui ─ clé DPAPI lisible ? ── oui → espace de travail
//! │                            └─ non → phrase demandée (autre poste)
//! └─ non ─ coffre Electron ?
//!          ├─ chiffré ── récupération automatique ? ── oui → migration → espace de travail
//!          │                                        └─ non → phrase demandée pour la migration
//!          ├─ non chiffré → création d'une phrase, connexions reprises
//!          └─ absent → création d'une phrase, coffre vide
//! ```

use ssh_manager_vault::legacy::{LegacyImport, LegacyVault};
use ssh_manager_vault::{Vault, VaultData, VaultError, paths};

pub enum Start {
    Ready {
        vault: Vault,
        data: VaultData,
        notice: Option<String>,
    },
    /// La phrase de récupération est nécessaire pour continuer.
    NeedsPhrase(PhrasePurpose),
    /// Aucun coffre chiffré exploitable : il faut créer une phrase.
    /// `imported` contient les connexions reprises d'un ancien coffre non chiffré.
    NeedsSetup {
        imported: Option<VaultData>,
    },
    Failed(String),
}

pub enum PhrasePurpose {
    /// Le coffre existe mais sa clé protégée n'est pas lisible sur ce poste.
    UnlockVault,
    /// L'ancien coffre Electron n'a pas pu être ouvert automatiquement.
    ImportLegacy(LegacyVault),
}

pub fn determine() -> Start {
    let vault_file = paths::vault_file();
    if vault_file.exists() {
        return match Vault::open(&vault_file) {
            Ok((vault, data)) => Start::Ready {
                vault,
                data,
                notice: None,
            },
            Err(VaultError::RecoveryPhraseRequired) => {
                Start::NeedsPhrase(PhrasePurpose::UnlockVault)
            }
            Err(error) => {
                tracing::error!(path = %vault_file.display(), %error, "failed to open the vault");
                Start::Failed(error.to_string())
            }
        };
    }

    // Une version portable démarre à neuf : elle ne reprend pas le coffre de
    // l'ancienne application installée sur le poste où on la branche.
    if paths::is_portable() {
        tracing::info!(dir = %paths::data_dir().display(), "portable first launch");
        return Start::NeedsSetup { imported: None };
    }

    let Some(legacy) = LegacyVault::locate(paths::legacy_dir()) else {
        tracing::info!("no vault and no legacy vault found — first launch");
        return Start::NeedsSetup { imported: None };
    };

    if !legacy.is_encrypted() {
        return match legacy.read(None) {
            Ok(import) => Start::NeedsSetup {
                imported: Some(import.data),
            },
            Err(error) => Start::Failed(error.to_string()),
        };
    }

    match legacy.recover_key_automatically() {
        Ok(key) => match legacy.read(Some(key)).and_then(migrate) {
            Ok(start) => start,
            Err(error) => {
                tracing::error!(%error, "legacy vault migration failed");
                Start::Failed(error.to_string())
            }
        },
        Err(error) => {
            tracing::warn!(%error, "automatic legacy key recovery failed — asking for the recovery phrase");
            Start::NeedsPhrase(PhrasePurpose::ImportLegacy(legacy))
        }
    }
}

/// Crée le nouveau coffre à partir d'un import chiffré. Le sel et l'empreinte
/// d'origine sont repris : la phrase de récupération existante reste valable.
pub fn migrate(import: LegacyImport) -> Result<Start, VaultError> {
    let (Some(key), Some(salt), Some(verify_hash)) = (import.key, import.salt, import.verify_hash)
    else {
        return Err(VaultError::IncompleteLegacyVault(
            "clé, sel ou empreinte manquant",
        ));
    };
    let vault = Vault::from_existing_key(paths::vault_file(), key, salt, verify_hash);
    vault.save(&import.data)?;
    let count = import.data.library.connections.len();
    tracing::info!(connections = count, "legacy vault migrated");
    let notice = Some(match count {
        0 => "Ancien coffre importé — il ne contenait aucune connexion.".to_owned(),
        1 => "1 connexion importée depuis l'ancienne application.".to_owned(),
        n => format!("{n} connexions importées depuis l'ancienne application."),
    });
    Ok(Start::Ready {
        vault,
        data: import.data,
        notice,
    })
}
