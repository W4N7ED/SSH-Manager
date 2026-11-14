//! Barre unifiée : rail, recherche, filtre de protocole, nouvelle connexion.

use egui::{CornerRadius, Frame, Margin, RichText, Ui};
use ssh_manager_core::{ConnectionKind, KindFilter};

use super::{Action, Workspace};
use crate::theme::{self, color, radius};
use crate::widgets;

pub fn show(workspace: &mut Workspace, ui: &mut Ui, actions: &mut Vec<Action>) {
    egui::Panel::top("top-bar")
        .exact_size(56.0)
        .show_separator_line(false)
        .frame(Frame::new().fill(color::BASE).inner_margin(Margin {
            left: 14,
            right: 14,
            top: 12,
            bottom: 10,
        }))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                let toggle = egui::Button::new(RichText::new("≡").color(color::TEXT_MUTED))
                    .fill(color::SURFACE)
                    .corner_radius(CornerRadius::same(radius::SMALL))
                    .min_size(egui::vec2(30.0, 34.0));
                if ui
                    .add(toggle)
                    .on_hover_text("Afficher ou masquer les groupes")
                    .clicked()
                {
                    workspace.rail_collapsed = !workspace.rail_collapsed;
                }

                search_field(workspace, ui);

                let options = [
                    (KindFilter::All, "Tous"),
                    (KindFilter::Only(ConnectionKind::Ssh), "SSH"),
                    (KindFilter::Only(ConnectionKind::Sftp), "SFTP"),
                    (KindFilter::Only(ConnectionKind::Ftp), "FTP"),
                ];
                widgets::segmented(ui, &options, &mut workspace.kind_filter);

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let group = match &workspace.group_filter {
                        ssh_manager_core::GroupFilter::Group(name) => Some(name.clone()),
                        _ => None,
                    };
                    if widgets::primary_button(ui, "+ Nouvelle connexion").clicked() {
                        actions.push(Action::NewConnection { group });
                    }
                    let settings =
                        egui::Button::new(RichText::new("⚙").size(15.0).color(color::TEXT_MUTED))
                            .fill(color::SURFACE)
                            .corner_radius(CornerRadius::same(radius::PILL))
                            .min_size(egui::vec2(34.0, 34.0));
                    if ui.add(settings).on_hover_text("Réglages").clicked() {
                        actions.push(Action::OpenSettings);
                    }
                });
            });
        });
}

fn search_field(workspace: &mut Workspace, ui: &mut Ui) {
    Frame::new()
        .fill(color::SURFACE)
        .corner_radius(CornerRadius::same(radius::PILL))
        .stroke(egui::Stroke::new(1.0, color::BORDER_SUBTLE))
        .inner_margin(Margin::symmetric(14, 0))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(RichText::new("⌕").color(color::TEXT_DIM));
                ui.add(
                    egui::TextEdit::singleline(&mut workspace.search)
                        .hint_text(
                            RichText::new("Rechercher un nom, un hôte, un utilisateur")
                                .color(color::TEXT_DIM),
                        )
                        .frame(egui::Frame::NONE)
                        .font(theme::mono(13.0))
                        .desired_width(360.0),
                );
                if !workspace.search.is_empty()
                    && ui
                        .add(
                            egui::Button::new(RichText::new("✕").size(11.0).color(color::TEXT_DIM))
                                .frame(false),
                        )
                        .clicked()
                {
                    workspace.search.clear();
                }
            });
        });
}
