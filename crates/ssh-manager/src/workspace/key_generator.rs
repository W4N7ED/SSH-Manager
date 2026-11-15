//! Panneau « Créer une nouvelle clé » du formulaire de connexion : crée une
//! paire dans `~/.ssh`, puis affiche une fois la clé publique à copier sur le
//! serveur.

use std::path::PathBuf;

use egui::{CornerRadius, Frame, Margin, RichText, Ui};

use crate::background::BackgroundTask;
use crate::secure_copy::SecureClipboard;
use crate::settings::Settings;
use crate::ssh_keys::{self, GeneratedKey, KeyGenerationError, KeyType};
use crate::theme::{self, color, radius};
use crate::widgets;

type Generation = BackgroundTask<Result<GeneratedKey, KeyGenerationError>>;

pub struct KeyGenerator {
    key_type: KeyType,
    name: String,
    /// Tant que le nom n'a pas été retouché, il suit le type choisi.
    name_edited: bool,
    connection_label: String,
    running: Option<Generation>,
    error: Option<String>,
    public_key: Option<String>,
    copy_notice: Option<String>,
}

pub enum KeyGeneratorEvent {
    /// Clé créée : le formulaire l'utilise comme clé privée de la connexion.
    Generated(PathBuf),
    Closed,
}

impl KeyGenerator {
    pub fn new(connection_label: &str) -> Self {
        Self {
            key_type: KeyType::Ed25519,
            name: ssh_keys::suggest_key_name(KeyType::Ed25519, connection_label),
            name_edited: false,
            connection_label: connection_label.to_owned(),
            running: None,
            error: None,
            public_key: None,
            copy_notice: None,
        }
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        clipboard: &mut SecureClipboard,
        settings: &Settings,
    ) -> Option<KeyGeneratorEvent> {
        let mut event = self.poll();
        Frame::new()
            .fill(color::SURFACE)
            .corner_radius(CornerRadius::same(radius::MEDIUM))
            .inner_margin(Margin::same(14))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let closed = match self.public_key.clone() {
                    Some(public_key) => self.result_view(ui, &public_key, clipboard, settings),
                    None => self.creation_view(ui),
                };
                if closed {
                    event = Some(KeyGeneratorEvent::Closed);
                }
            });
        event
    }

    fn poll(&mut self) -> Option<KeyGeneratorEvent> {
        let result = self.running.as_ref()?.poll()?;
        self.running = None;
        match result {
            Ok(generated) => {
                self.public_key = Some(generated.public_key);
                Some(KeyGeneratorEvent::Generated(generated.private_key_path))
            }
            Err(error) => {
                tracing::warn!(%error, "ssh key generation refused");
                self.error = Some(error.to_string());
                None
            }
        }
    }

    /// Renvoie vrai si l'utilisateur annule.
    fn creation_view(&mut self, ui: &mut Ui) -> bool {
        ui.label(RichText::new("Créer une nouvelle clé").font(theme::sans_semibold(14.0)));
        ui.add_space(6.0);
        let options =
            [KeyType::Ed25519, KeyType::Rsa4096].map(|key_type| (key_type, key_type.label()));
        if widgets::segmented(ui, &options, &mut self.key_type) && !self.name_edited {
            self.name = ssh_keys::suggest_key_name(self.key_type, &self.connection_label);
        }
        ui.label(
            RichText::new(self.key_type.hint())
                .font(theme::sans(12.0))
                .color(color::TEXT_DIM),
        );
        ui.add_space(8.0);

        widgets::micro_label(ui, "Nom du fichier");
        let response = ui.add(
            widgets::text_field(&mut self.name, "id_ed25519_mon-serveur")
                .font(theme::mono(13.0))
                .desired_width(f32::INFINITY),
        );
        if response.changed() {
            self.name_edited = true;
            self.error = None;
        }
        let path = ssh_keys::ssh_directory().join(self.name.trim());
        ui.label(
            RichText::new(path.display().to_string())
                .font(theme::mono(11.0))
                .color(color::TEXT_DIM),
        );
        if let Some(error) = &self.error {
            ui.label(RichText::new(error).color(color::DANGER));
        }

        ui.add_space(8.0);
        let mut cancelled = false;
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Créée sans phrase de passe.")
                    .font(theme::sans(12.0))
                    .color(color::TEXT_DIM),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let busy = self.running.is_some();
                let label = if busy { "Génération…" } else { "Générer" };
                if widgets::primary_button_enabled(ui, !busy && !self.name.trim().is_empty(), label)
                    .clicked()
                {
                    self.start(ui.ctx());
                }
                if widgets::secondary_button(ui, "Annuler").clicked() {
                    cancelled = true;
                }
            });
        });
        cancelled
    }

    fn start(&mut self, ctx: &egui::Context) {
        self.error = None;
        let name = self.name.trim().to_owned();
        let key_type = self.key_type;
        // RSA 4096 prend quelques secondes : hors du fil de l'interface.
        self.running = Some(BackgroundTask::spawn(ctx, move || {
            ssh_keys::generate(&ssh_keys::ssh_directory(), &name, key_type)
        }));
    }

    /// Renvoie vrai si l'utilisateur ferme le panneau.
    fn result_view(
        &mut self,
        ui: &mut Ui,
        public_key: &str,
        clipboard: &mut SecureClipboard,
        settings: &Settings,
    ) -> bool {
        ui.label(
            RichText::new("✓ Clé créée dans ~/.ssh")
                .font(theme::sans_semibold(14.0))
                .color(color::SUCCESS),
        );
        ui.label(
            RichText::new("Ajoutez cette clé publique dans ~/.ssh/authorized_keys sur le serveur pour que la connexion aboutisse.")
                .font(theme::sans(12.0))
                .color(color::TEXT_MUTED),
        );
        ui.add_space(6.0);
        Frame::new()
            .fill(color::BASE)
            .corner_radius(CornerRadius::same(radius::SMALL))
            .inner_margin(Margin::same(8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.add(egui::Label::new(RichText::new(public_key).font(theme::mono(11.0))).wrap());
            });
        ui.add_space(8.0);
        let mut closed = false;
        ui.horizontal(|ui| {
            let label = self
                .copy_notice
                .as_deref()
                .unwrap_or("Copier la clé publique");
            if widgets::secondary_button(ui, label).clicked() {
                let outcome = clipboard.copy(ui.ctx(), public_key, settings);
                self.copy_notice = Some(outcome.describe(true));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::secondary_button(ui, "Fermer").clicked() {
                    closed = true;
                }
            });
        });
        closed
    }
}
