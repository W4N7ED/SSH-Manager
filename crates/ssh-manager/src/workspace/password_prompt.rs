//! Invite d'un secret à l'ouverture : mot de passe d'une connexion qui n'en
//! mémorise pas, ou phrase de passe d'une clé privée chiffrée.

use egui::{CornerRadius, Frame, Margin, RichText};
use ssh_manager_core::Connection;

use crate::theme::{self, color, radius};
use crate::widgets;

/// Le secret demandé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKind {
    Password,
    /// `wrong` : la phrase essayée a été refusée.
    KeyPassphrase {
        wrong: bool,
    },
}

pub struct PasswordPrompt {
    pub connection: Connection,
    kind: SecretKind,
    secret: String,
    remember: bool,
}

#[expect(
    clippy::large_enum_variant,
    reason = "valeur de retour éphémère, jamais stockée"
)]
pub enum PromptResult {
    Confirmed {
        connection: Connection,
        kind: SecretKind,
        remember: bool,
    },
    Cancelled,
}

impl PasswordPrompt {
    pub fn new(connection: Connection) -> Self {
        Self::asking(connection, SecretKind::Password)
    }

    pub fn asking(connection: Connection, kind: SecretKind) -> Self {
        Self {
            connection,
            kind,
            secret: String::new(),
            remember: false,
        }
    }

    fn texts(&self) -> (&'static str, &'static str) {
        match self.kind {
            SecretKind::Password => ("Mot de passe", "Mémoriser le mot de passe dans le coffre"),
            SecretKind::KeyPassphrase { .. } => (
                "Phrase de passe de la clé privée",
                "Mémoriser la phrase de passe dans le coffre",
            ),
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<PromptResult> {
        let mut result = None;
        let modal = egui::Modal::new(egui::Id::unique("password-prompt"))
            .backdrop_color(color::tint(egui::Color32::BLACK, 150))
            .frame(
                Frame::new()
                    .fill(color::BASE)
                    .stroke(egui::Stroke::new(1.0, color::BORDER))
                    .corner_radius(CornerRadius::same(radius::LARGE))
                    .inner_margin(Margin::same(22)),
            )
            .show(ctx, |ui| {
                ui.set_width(400.0);
                ui.label(RichText::new("Authentification").font(theme::sans_bold(18.0)));
                ui.add_space(10.0);
                self.connection_card(ui);
                ui.add_space(12.0);
                result = self.form(ui);
            });
        if modal.should_close() && result.is_none() {
            result = Some(PromptResult::Cancelled);
        }
        result
    }

    fn connection_card(&self, ui: &mut egui::Ui) {
        widgets::card(color::SURFACE).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(&self.connection.name).font(theme::sans_semibold(13.0)));
            ui.label(
                RichText::new(self.connection.target())
                    .font(theme::mono(11.0))
                    .color(color::TEXT_MUTED),
            );
            if let (SecretKind::KeyPassphrase { .. }, Some(path)) =
                (self.kind, &self.connection.private_key_path)
            {
                ui.label(
                    RichText::new(format!("Clé : {path}"))
                        .font(theme::mono(11.0))
                        .color(color::TEXT_DIM),
                );
            }
        });
    }

    fn form(&mut self, ui: &mut egui::Ui) -> Option<PromptResult> {
        let (label, remember_label) = self.texts();
        widgets::micro_label(ui, label);
        let field = ui.add(
            widgets::text_field(&mut self.secret, "••••••••")
                .password(true)
                .desired_width(f32::INFINITY),
        );
        field.request_focus();
        if self.kind == (SecretKind::KeyPassphrase { wrong: true }) {
            ui.label(RichText::new("La phrase précédente a été refusée.").color(color::DANGER));
        }
        let submitted = field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        ui.add_space(6.0);
        ui.checkbox(&mut self.remember, remember_label);
        ui.add_space(14.0);
        let mut result = None;
        ui.horizontal(|ui| {
            let connect = widgets::primary_button(ui, "Connecter").clicked();
            if widgets::secondary_button(ui, "Annuler").clicked() {
                result = Some(PromptResult::Cancelled);
            }
            if connect || submitted {
                let mut connection = self.connection.clone();
                let secret = Some(std::mem::take(&mut self.secret));
                match self.kind {
                    SecretKind::Password => connection.password = secret,
                    SecretKind::KeyPassphrase { .. } => connection.key_passphrase = secret,
                }
                result = Some(PromptResult::Confirmed {
                    connection,
                    kind: self.kind,
                    remember: self.remember,
                });
            }
        });
        result
    }
}
