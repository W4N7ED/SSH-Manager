//! Invite de mot de passe, pour une connexion qui n'en mémorise pas.

use egui::{CornerRadius, Frame, Margin, RichText};
use ssh_manager_core::Connection;

use crate::theme::{self, color, radius};
use crate::widgets;

pub struct PasswordPrompt {
    pub connection: Connection,
    password: String,
    remember: bool,
}

#[expect(
    clippy::large_enum_variant,
    reason = "valeur de retour éphémère, jamais stockée"
)]
pub enum PromptResult {
    Confirmed {
        connection: Connection,
        remember: bool,
    },
    Cancelled,
}

impl PasswordPrompt {
    pub fn new(connection: Connection) -> Self {
        Self {
            connection,
            password: String::new(),
            remember: false,
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
                widgets::card(color::SURFACE).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(&self.connection.name).font(theme::sans_semibold(13.0)));
                    ui.label(
                        RichText::new(self.connection.target())
                            .font(theme::mono(11.0))
                            .color(color::TEXT_MUTED),
                    );
                });
                ui.add_space(12.0);
                widgets::micro_label(ui, "Mot de passe");
                let field = ui.add(
                    widgets::text_field(&mut self.password, "••••••••")
                        .password(true)
                        .desired_width(f32::INFINITY),
                );
                field.request_focus();
                let submitted =
                    field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                ui.add_space(6.0);
                ui.checkbox(
                    &mut self.remember,
                    "Mémoriser le mot de passe dans le coffre",
                );
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    let connect = widgets::primary_button(ui, "Connecter").clicked();
                    if widgets::secondary_button(ui, "Annuler").clicked() {
                        result = Some(PromptResult::Cancelled);
                    }
                    if connect || submitted {
                        let mut connection = self.connection.clone();
                        connection.password = Some(std::mem::take(&mut self.password));
                        result = Some(PromptResult::Confirmed {
                            connection,
                            remember: self.remember,
                        });
                    }
                });
            });
        if modal.should_close() && result.is_none() {
            result = Some(PromptResult::Cancelled);
        }
        result
    }
}
