//! Export proposé par le désinstalleur, sans ouvrir l'application : le
//! désinstalleur lance `ssh-manager.exe --pre-uninstall-export` et attend.
//!
//! La sauvegarde est chiffrée (`.enc`) : elle se restaure avec la phrase de
//! 12 mots, sur ce poste ou un autre.

use rfd::{MessageButtons, MessageDialog, MessageLevel};
use ssh_manager_vault::{Vault, VaultError, paths};

pub const FLAG: &str = "--pre-uninstall-export";

fn inform(level: MessageLevel, text: &str) {
    MessageDialog::new()
        .set_title("SSH Manager — export avant désinstallation")
        .set_level(level)
        .set_description(text)
        .set_buttons(MessageButtons::Ok)
        .show();
}

pub fn run() {
    let vault_file = paths::vault_file();
    if !vault_file.exists() {
        inform(
            MessageLevel::Info,
            "Aucune connexion enregistrée : rien à exporter.",
        );
        return;
    }
    let (vault, data) = match Vault::open(&vault_file) {
        Ok(opened) => opened,
        Err(VaultError::RecoveryPhraseRequired) => {
            inform(
                MessageLevel::Warning,
                "Le coffre ne peut pas être ouvert automatiquement sur ce poste.\n\nOuvrez SSH Manager, saisissez votre phrase de récupération, puis exportez depuis Réglages → Sauvegarde avant de désinstaller.",
            );
            return;
        }
        Err(error) => {
            tracing::error!(%error, "pre-uninstall export: vault could not be opened");
            inform(
                MessageLevel::Error,
                &format!("Ouverture du coffre impossible : {error}"),
            );
            return;
        }
    };

    let file_name = format!(
        "ssh-manager-backup-{}.enc",
        chrono::Local::now().format("%Y-%m-%d")
    );
    let Some(target) = rfd::FileDialog::new()
        .set_title("Exporter les connexions avant la désinstallation")
        .set_directory(dirs::document_dir().unwrap_or_default())
        .set_file_name(file_name)
        .add_filter("Sauvegarde chiffrée SSH Manager", &["enc"])
        .save_file()
    else {
        tracing::info!("pre-uninstall export cancelled by the user");
        return;
    };
    match std::fs::write(&target, vault.export_backup(&data)) {
        Ok(()) => {
            tracing::info!(path = %target.display(), connections = data.library.connections.len(), "pre-uninstall export written");
            inform(
                MessageLevel::Info,
                &format!(
                    "{} connexion(s) exportée(s) dans :\n{}\n\nRestaurez-les avec Réglages → Sauvegarde → Importer et votre phrase de 12 mots.",
                    data.library.connections.len(),
                    target.display()
                ),
            );
        }
        Err(error) => {
            tracing::error!(path = %target.display(), %error, "pre-uninstall export failed");
            inform(
                MessageLevel::Error,
                &format!("Écriture impossible : {error}"),
            );
        }
    }
}
