//! Barre unifiée : rail, recherche, filtre de protocole, nouvelle connexion.

use egui::{CornerRadius, Frame, Margin, RichText, Ui};
use ssh_manager_core::{Connection, ConnectionKind, KindFilter};

use super::{Action, Workspace, now_millis};
use crate::theme::{self, color, radius};
use crate::widgets;

/// Identifiant du champ de recherche, pour lui donner le focus (Ctrl+F).
pub const SEARCH_FIELD_ID: &str = "top-bar-search";

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

                search_field(workspace, ui, actions);

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
                    let history = egui::Button::new(
                        RichText::new("Historique")
                            .size(13.0)
                            .color(color::TEXT_MUTED),
                    )
                    .fill(color::SURFACE)
                    .corner_radius(CornerRadius::same(radius::PILL))
                    .min_size(egui::vec2(0.0, 34.0));
                    if ui
                        .add(history)
                        .on_hover_text("Sessions enregistrées, à exporter en .txt")
                        .clicked()
                    {
                        actions.push(Action::OpenRecordings);
                    }
                });
            });
        });
}

/// Connexion ponctuelle si la recherche a la forme `user@hôte[:port]`. Le
/// protocole suit le filtre choisi, SSH par défaut.
fn quick_target(workspace: &Workspace) -> Option<Connection> {
    let kind = match workspace.kind_filter {
        KindFilter::Only(kind) => kind,
        KindFilter::All => ConnectionKind::Ssh,
    };
    Connection::from_quick_target(&workspace.search, kind, now_millis())
}

fn search_field(workspace: &mut Workspace, ui: &mut Ui, actions: &mut Vec<Action>) {
    Frame::new()
        .fill(color::SURFACE)
        .corner_radius(CornerRadius::same(radius::PILL))
        .stroke(egui::Stroke::new(1.0, color::BORDER_SUBTLE))
        .inner_margin(Margin::symmetric(14, 0))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(RichText::new("⌕").color(color::TEXT_DIM));
                let field = ui.add(
                    egui::TextEdit::singleline(&mut workspace.search)
                        .id(egui::Id::unique(SEARCH_FIELD_ID))
                        // Même police que la saisie : sinon l'indication, plus haute
                        // ou plus basse, n'est plus centrée dans la pastille.
                        .hint_text(
                            RichText::new("Rechercher, ou user@hôte:port pour se connecter")
                                .font(theme::mono(12.0))
                                .color(color::TEXT_DIM),
                        )
                        .frame(egui::Frame::NONE)
                        .font(theme::mono(13.0))
                        .min_size(egui::vec2(0.0, 32.0))
                        .align(egui::Align2::LEFT_CENTER)
                        .desired_width(380.0),
                );
                if let Some(connection) = quick_target(workspace) {
                    let submitted =
                        field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                    let connect = egui::Button::new(
                        RichText::new(format!(
                            "↵ {} {}",
                            connection.kind.label(),
                            connection.target()
                        ))
                        .font(theme::mono(11.0))
                        .color(color::ON_ACCENT),
                    )
                    .fill(color::ACCENT)
                    .corner_radius(CornerRadius::same(radius::PILL));
                    if ui
                        .add(connect)
                        .on_hover_text("Se connecter sans enregistrer (Entrée)")
                        .clicked()
                        || submitted
                    {
                        workspace.search.clear();
                        actions.push(Action::QuickConnect(Box::new(connection.clone())));
                    } else if ui
                        .add(
                            egui::Button::new(
                                RichText::new("Enregistrer…").font(theme::sans(12.0)),
                            )
                            .frame(false),
                        )
                        .on_hover_text("Créer une connexion à partir de cette adresse")
                        .clicked()
                    {
                        workspace.search.clear();
                        actions.push(Action::SaveQuickTarget(Box::new(connection)));
                    }
                }
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
