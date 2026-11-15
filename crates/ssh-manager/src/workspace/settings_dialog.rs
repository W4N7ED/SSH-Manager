//! Fenêtre des réglages de l'application.

use egui::{CornerRadius, Frame, Margin, RichText};

use crate::settings::{PasteMode, Settings};
use crate::theme::{self, color, radius};
use crate::widgets;

/// Affiche les réglages ; renvoie vrai quand la fenêtre est fermée. Chaque
/// changement s'applique immédiatement, l'enregistrement se fait à la fermeture.
pub fn show(ctx: &egui::Context, settings: &mut Settings) -> bool {
    let mut closed = false;
    let modal = egui::Modal::new(egui::Id::unique("settings-dialog"))
        .backdrop_color(color::tint(egui::Color32::BLACK, 150))
        .frame(
            Frame::new()
                .fill(color::BASE)
                .stroke(egui::Stroke::new(1.0, color::BORDER))
                .corner_radius(CornerRadius::same(radius::LARGE))
                .inner_margin(Margin::same(22)),
        )
        .show(ctx, |ui| {
            ui.set_width(460.0);
            ui.label(RichText::new("Réglages").font(theme::sans_bold(21.0)));
            ui.add_space(16.0);

            widgets::micro_label(ui, "Terminal — coller");
            ui.add_space(4.0);
            widgets::segmented(
                ui,
                &[(PasteMode::RightClick, PasteMode::RightClick.label()), (PasteMode::CtrlV, PasteMode::CtrlV.label())],
                &mut settings.paste_mode,
            );
            ui.add_space(6.0);
            let explanation = match settings.paste_mode {
                PasteMode::RightClick => "Le clic droit colle le presse-papiers. Ctrl+V est transmis au shell (insertion littérale d'un caractère).",
                PasteMode::CtrlV => "Ctrl+V colle le presse-papiers. Le clic droit n'a pas d'effet dans le terminal.",
            };
            ui.label(RichText::new(explanation).font(theme::sans(12.0)).color(color::TEXT_MUTED));
            ui.label(
                RichText::new("Dans les deux cas, le texte est collé sur la ligne du prompt : un saut de ligne final n'est pas transmis, la commande n'est donc pas exécutée.")
                    .font(theme::sans(12.0))
                    .color(color::TEXT_DIM),
            );

            ui.add_space(18.0);
            widgets::micro_label(ui, "Terminal — enregistrement");
            ui.add_space(4.0);
            ui.checkbox(
                &mut settings.record_sessions,
                RichText::new("Enregistrer le texte de chaque session").font(theme::sans(13.0)),
            );
            ui.label(
                RichText::new("Le texte est chiffré avec la clé du coffre. Retrouvez les sessions dans « Historique » pour les exporter en .txt. Les saisies non affichées (mots de passe) ne sont jamais enregistrées.")
                    .font(theme::sans(12.0))
                    .color(color::TEXT_DIM),
            );

            ui.add_space(20.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::primary_button(ui, "Fermer").clicked() {
                    closed = true;
                }
            });
        });
    closed || modal.should_close()
}
