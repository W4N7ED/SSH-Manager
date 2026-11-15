//! Fenêtres de la sauvegarde : phrase de 12 mots pour importer une sauvegarde
//! d'un autre coffre, et confirmation d'un export en clair.

use egui::{CornerRadius, Frame, Margin, RichText};
use ssh_manager_vault::backup::EncryptedBackup;
use zeroize::Zeroizing;

use crate::theme::{self, color, radius};
use crate::widgets;

fn modal_frame() -> Frame {
    Frame::new()
        .fill(color::BASE)
        .stroke(egui::Stroke::new(1.0, color::BORDER))
        .corner_radius(CornerRadius::same(radius::LARGE))
        .inner_margin(Margin::same(22))
}

/// Sauvegarde chiffrée par un autre coffre, en attente de sa phrase.
pub struct PhrasePrompt {
    pub backup: EncryptedBackup,
    phrase: Zeroizing<String>,
    pub error: Option<String>,
}

pub enum PhraseOutcome {
    Submitted(Zeroizing<String>),
    Cancelled,
}

impl PhrasePrompt {
    pub fn new(backup: EncryptedBackup) -> Self {
        Self {
            backup,
            phrase: Zeroizing::new(String::new()),
            error: None,
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<PhraseOutcome> {
        let mut outcome = None;
        let modal = egui::Modal::new(egui::Id::unique("backup-phrase"))
            .backdrop_color(color::tint(egui::Color32::BLACK, 150))
            .frame(modal_frame())
            .show(ctx, |ui| {
                ui.set_width(480.0);
                ui.label(RichText::new("Sauvegarde d'un autre coffre").font(theme::sans_bold(19.0)));
                ui.label(
                    RichText::new("Cette sauvegarde a été chiffrée par un autre coffre. Saisissez sa phrase de récupération de 12 mots pour la déchiffrer.")
                        .font(theme::sans(12.0))
                        .color(color::TEXT_MUTED),
                );
                ui.add_space(10.0);
                let field = ui.add(
                    egui::TextEdit::multiline(&mut *self.phrase)
                        .hint_text("mot1 mot2 mot3 …")
                        .font(theme::mono(13.0))
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                );
                field.request_focus();
                if let Some(error) = &self.error {
                    ui.label(RichText::new(error).color(color::DANGER));
                }
                ui.add_space(12.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let ready = !self.phrase.trim().is_empty();
                    let submitted = ui.input(|input| input.key_pressed(egui::Key::Enter) && !input.modifiers.shift);
                    if widgets::primary_button_enabled(ui, ready, "Déchiffrer").clicked() || (ready && submitted) {
                        outcome = Some(PhraseOutcome::Submitted(Zeroizing::new(self.phrase.trim().to_owned())));
                    }
                    if widgets::secondary_button(ui, "Annuler").clicked() {
                        outcome = Some(PhraseOutcome::Cancelled);
                    }
                });
            });
        if outcome.is_none() && modal.should_close() {
            outcome = Some(PhraseOutcome::Cancelled);
        }
        outcome
    }
}

/// Avertit qu'un export `.json` contient les mots de passe en clair.
/// Renvoie `Some(true)` pour exporter quand même, `Some(false)` pour annuler.
pub fn confirm_plain_export(ctx: &egui::Context) -> Option<bool> {
    let mut answer = None;
    let modal = egui::Modal::new(egui::Id::unique("plain-export-confirmation"))
        .backdrop_color(color::tint(egui::Color32::BLACK, 150))
        .frame(modal_frame())
        .show(ctx, |ui| {
            ui.set_width(460.0);
            ui.label(RichText::new("⚠ Exporter en clair ?").font(theme::sans_bold(19.0)).color(color::WARNING));
            ui.add_space(6.0);
            ui.label(
                "Le fichier .json contiendra vos mots de passe et identifiants EN CLAIR, \
                 lisibles par quiconque y a accès.",
            );
            ui.label(
                RichText::new("Préférez la sauvegarde chiffrée (.enc), sauf si vous savez ce que vous faites.")
                    .color(color::TEXT_MUTED),
            );
            ui.add_space(14.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let danger = egui::Button::new(RichText::new("Exporter en clair").color(color::DANGER))
                    .fill(color::SURFACE)
                    .corner_radius(CornerRadius::same(radius::SMALL));
                if ui.add(danger).clicked() {
                    answer = Some(true);
                }
                if widgets::primary_button(ui, "Annuler").clicked() {
                    answer = Some(false);
                }
            });
        });
    if answer.is_none() && modal.should_close() {
        answer = Some(false);
    }
    answer
}
