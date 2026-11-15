//! Sauvegardes des connexions : export (.enc ou .json), import, phrase
//! d'un autre coffre.

use super::*;

impl Workspace {
    /// Importe les serveurs de `~/.ssh/config` dans le groupe « ssh config ».
    /// Un serveur déjà enregistré (même user@hôte:port) n'est pas dupliqué.
    pub(super) fn import_ssh_config(&mut self) {
        let home = dirs::home_dir().unwrap_or_default();
        let path = home.join(".ssh").join("config");
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.notify(format!("Aucun fichier {}", path.display()), false);
                return;
            }
            Err(error) => {
                tracing::error!(path = %path.display(), %error, "ssh config could not be read");
                self.notify(format!("Lecture impossible : {error}"), false);
                return;
            }
        };
        let home_text = home.display().to_string();
        let default_user = std::env::var("USERNAME").unwrap_or_default();
        let context = ssh_manager_core::ssh_config::ImportContext {
            home: &home_text,
            default_user: &default_user,
            group: "ssh config",
            now: now_millis(),
        };
        let entries = ssh_manager_core::ssh_config::parse(&text);
        let connections = ssh_manager_core::ssh_config::to_connections(&entries, &context);
        let added = self.data.library.import(connections);
        tracing::info!(path = %path.display(), hosts = entries.len(), added, "ssh config imported");
        self.persist();
        self.notify(
            format!(
                "{added} connexion(s) importée(s) depuis ~/.ssh/config ({} serveur(s) lu(s))",
                entries.len()
            ),
            true,
        );
    }

    pub(super) fn backup_modals(&mut self, ctx: &egui::Context) {
        if self.confirm_plain_export
            && let Some(confirmed) = backup_dialogs::confirm_plain_export(ctx)
        {
            self.confirm_plain_export = false;
            if confirmed {
                self.export_backup(true);
            }
        }
        let Some(prompt) = &mut self.backup_phrase else {
            return;
        };
        match prompt.show(ctx) {
            None => {}
            Some(PhraseOutcome::Cancelled) => self.backup_phrase = None,
            Some(PhraseOutcome::Submitted(phrase)) => match prompt.backup.open_with_phrase(&phrase)
            {
                Ok(imported) => {
                    self.backup_phrase = None;
                    self.merge_backup(imported);
                }
                Err(error) => {
                    tracing::warn!(%error, "backup decryption refused");
                    prompt.error = Some("Phrase incorrecte ou fichier corrompu.".into());
                }
            },
        }
    }

    pub(super) fn export_backup(&mut self, plain: bool) {
        let (file_name, filter, extension) = if plain {
            ("ssh-manager-backup.json", "JSON non chiffré", "json")
        } else {
            (
                "ssh-manager-backup.enc",
                "Sauvegarde chiffrée SSH Manager",
                "enc",
            )
        };
        let Some(target) = rfd::FileDialog::new()
            .set_title("Exporter les connexions")
            .set_file_name(file_name)
            .add_filter(filter, &[extension])
            .save_file()
        else {
            return;
        };
        let content = if plain {
            backup::export_plain(&self.data)
        } else {
            self.vault.export_backup(&self.data)
        };
        match std::fs::write(&target, content) {
            Ok(()) => {
                tracing::info!(path = %target.display(), plain, connections = self.data.library.connections.len(), "backup exported");
                self.notify(format!("Exporté : {}", target.display()), true);
            }
            Err(error) => {
                tracing::error!(path = %target.display(), %error, "failed to write the backup");
                self.notify(format!("Export impossible : {error}"), false);
            }
        }
    }

    pub(super) fn import_backup(&mut self) {
        let Some(source) = rfd::FileDialog::new()
            .set_title("Importer des connexions")
            .add_filter("Sauvegardes SSH Manager", &["enc", "json"])
            .pick_file()
        else {
            return;
        };
        let parsed = std::fs::read(&source)
            .map_err(|error| error.to_string())
            .and_then(|bytes| backup::parse(&bytes).map_err(|error| error.to_string()));
        match parsed {
            Ok(Backup::Plain(imported)) => self.merge_backup(imported),
            Ok(Backup::Encrypted(encrypted)) => match self.vault.open_own_backup(&encrypted) {
                Some(Ok(imported)) => self.merge_backup(imported),
                Some(Err(error)) => {
                    tracing::error!(path = %source.display(), %error, "backup of this vault could not be decrypted");
                    self.notify("Sauvegarde corrompue : déchiffrement impossible.", false);
                }
                // Sauvegarde d'un autre coffre : sa phrase est nécessaire.
                None => self.backup_phrase = Some(PhrasePrompt::new(encrypted)),
            },
            Err(error) => {
                tracing::warn!(path = %source.display(), %error, "backup file rejected");
                self.notify(format!("Fichier de sauvegarde invalide : {error}"), false);
            }
        }
    }

    pub(super) fn merge_backup(&mut self, imported: ssh_manager_vault::VaultData) {
        let added = backup::merge_into(&mut self.data, imported);
        tracing::info!(added, "backup imported");
        self.persist();
        self.notify(format!("{added} connexion(s) importée(s)"), true);
    }
}
