//! Barre de recherche d'un terminal (Ctrl+Maj+F) : Entrée cherche vers le
//! haut (plus ancien), Maj+Entrée vers le bas, Échap ferme.

use std::sync::Mutex;

use egui::{Frame, Margin, RichText, Ui};

use crate::session::emulator::Emulator;
use crate::session::search::{SearchDirection, SearchOutcome, TerminalSearch};
use crate::session::ssh::lock;
use crate::theme::{self, color};

#[derive(Default)]
pub struct SearchBar {
    search: TerminalSearch,
    /// Le champ prend le clavier une seule fois, à l'ouverture : ensuite un
    /// clic dans le terminal doit pouvoir le lui reprendre.
    focused_once: bool,
}

/// Renvoie vrai quand la barre est fermée.
pub fn show(ui: &mut Ui, bar: &mut SearchBar, emulator: &Mutex<Emulator>) -> bool {
    let mut closed = false;
    Frame::new()
        .fill(color::SURFACE)
        .inner_margin(Margin::symmetric(14, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("⌕").color(color::TEXT_DIM));
                let field = ui.add(
                    egui::TextEdit::singleline(&mut bar.search.query)
                        .hint_text("Rechercher dans le terminal")
                        .font(theme::mono(12.0))
                        .desired_width(260.0),
                );
                if !bar.focused_once {
                    field.request_focus();
                    bar.focused_once = true;
                }
                let (enter, shift, escape) = ui.input(|input| {
                    (
                        input.key_pressed(egui::Key::Enter),
                        input.modifiers.shift,
                        input.key_pressed(egui::Key::Escape),
                    )
                });
                let submitted = field.lost_focus() && enter;
                let mut direction = submitted.then_some(if shift {
                    SearchDirection::Newer
                } else {
                    SearchDirection::Older
                });
                if ui
                    .small_button("↑")
                    .on_hover_text("Plus ancien (Entrée)")
                    .clicked()
                {
                    direction = Some(SearchDirection::Older);
                }
                if ui
                    .small_button("↓")
                    .on_hover_text("Plus récent (Maj+Entrée)")
                    .clicked()
                {
                    direction = Some(SearchDirection::Newer);
                }
                if let Some(direction) = direction {
                    bar.search.find(&mut lock(emulator), direction);
                    // Entrée a retiré le focus du champ : on le rend pour enchaîner.
                    field.request_focus();
                }
                if bar.search.outcome == Some(SearchOutcome::NotFound) {
                    ui.label(
                        RichText::new("introuvable")
                            .font(theme::sans(12.0))
                            .color(color::DANGER),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let close =
                        egui::Button::new(RichText::new("✕").size(11.0).color(color::TEXT_DIM))
                            .frame(false);
                    if ui.add(close).on_hover_text("Fermer (Échap)").clicked()
                        // Échap retire le focus du champ dans la même image.
                        || (escape && (field.has_focus() || field.lost_focus()))
                    {
                        closed = true;
                    }
                });
            });
        });
    if closed {
        bar.search.clear(&mut lock(emulator));
    }
    closed
}
