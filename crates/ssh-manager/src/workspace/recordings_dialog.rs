//! Fenêtre « Historique » : les sessions enregistrées, à exporter ou supprimer.
//! Elle ne fait qu'afficher et renvoyer le choix ; les fichiers sont traités
//! par l'espace de travail.

use std::path::{Path, PathBuf};

use egui::{CornerRadius, Frame, Margin, RichText};
use ssh_manager_core::format;

use crate::recordings::{self, RecordingSummary};
use crate::theme::{self, color, radius};
use crate::widgets;

pub struct RecordingsDialog {
    recordings: Vec<RecordingSummary>,
    filter: String,
    /// Enregistrement dont la suppression attend une confirmation.
    confirm_delete: Option<PathBuf>,
}

pub enum RecordingsChoice {
    Export(RecordingSummary),
    Delete(PathBuf),
    Close,
}

impl RecordingsDialog {
    pub fn new(recordings: Vec<RecordingSummary>) -> Self {
        Self {
            recordings,
            filter: String::new(),
            confirm_delete: None,
        }
    }

    pub fn forget(&mut self, path: &Path) {
        self.recordings.retain(|recording| recording.path != path);
    }

    /// `live` : enregistrements des sessions encore ouvertes, non supprimables.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        live: &[PathBuf],
        recording_enabled: bool,
    ) -> Option<RecordingsChoice> {
        let mut choice = None;
        let modal = egui::Modal::new(egui::Id::unique("recordings-dialog"))
            .backdrop_color(color::tint(egui::Color32::BLACK, 150))
            .frame(
                Frame::new()
                    .fill(color::BASE)
                    .stroke(egui::Stroke::new(1.0, color::BORDER))
                    .corner_radius(CornerRadius::same(radius::LARGE))
                    .inner_margin(Margin::same(22)),
            )
            .show(ctx, |ui| {
                ui.set_width(640.0);
                ui.label(RichText::new("Historique des sessions").font(theme::sans_bold(21.0)));
                ui.label(
                    RichText::new("Le texte de chaque session terminal est conservé chiffré avec la clé du coffre. Exportez-le en .txt pour le lire ou le partager.")
                        .font(theme::sans(12.0))
                        .color(color::TEXT_MUTED),
                );
                if !recording_enabled {
                    ui.label(
                        RichText::new("L'enregistrement est désactivé dans les réglages : les nouvelles sessions ne sont pas conservées.")
                            .font(theme::sans(12.0))
                            .color(color::WARNING),
                    );
                }
                ui.add_space(10.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter)
                        .hint_text("Filtrer par nom ou hôte")
                        .font(theme::mono(12.0))
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(8.0);
                self.list(ui, live, &mut choice);
                ui.add_space(14.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::primary_button(ui, "Fermer").clicked() {
                        choice = Some(RecordingsChoice::Close);
                    }
                });
            });
        if modal.should_close() && choice.is_none() {
            choice = Some(RecordingsChoice::Close);
        }
        choice
    }

    fn list(&mut self, ui: &mut egui::Ui, live: &[PathBuf], choice: &mut Option<RecordingsChoice>) {
        let filter = self.filter.trim().to_lowercase();
        let visible: Vec<&RecordingSummary> = self
            .recordings
            .iter()
            .filter(|recording| {
                filter.is_empty()
                    || recording
                        .header
                        .connection_name
                        .to_lowercase()
                        .contains(&filter)
                    || recording.header.target.to_lowercase().contains(&filter)
            })
            .collect();
        if visible.is_empty() {
            ui.label(RichText::new("Aucun enregistrement.").color(color::TEXT_DIM));
            return;
        }
        let confirm_delete = self.confirm_delete.as_deref();
        let mut arm = None;
        egui::ScrollArea::vertical()
            .max_height(380.0)
            .show(ui, |ui| {
                for recording in visible {
                    let is_live = live.contains(&recording.path);
                    Frame::new()
                        .fill(color::SURFACE)
                        .corner_radius(CornerRadius::same(radius::SMALL))
                        .inner_margin(Margin::symmetric(12, 8))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(
                                        RichText::new(&recording.header.connection_name)
                                            .font(theme::sans_semibold(13.0)),
                                    );
                                    let details = format!(
                                        "{} · {} · {}",
                                        recordings::format_date(recording.header.started_at),
                                        recording.header.target,
                                        format::file_size(recording.size_bytes)
                                    );
                                    ui.label(
                                        RichText::new(details)
                                            .font(theme::mono(11.0))
                                            .color(color::TEXT_DIM),
                                    );
                                });
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        Self::row_actions(
                                            ui,
                                            recording,
                                            is_live,
                                            confirm_delete,
                                            &mut arm,
                                            choice,
                                        );
                                    },
                                );
                            });
                        });
                    ui.add_space(6.0);
                }
            });
        if arm.is_some() {
            self.confirm_delete = arm;
        }
    }

    fn row_actions(
        ui: &mut egui::Ui,
        recording: &RecordingSummary,
        is_live: bool,
        confirm_delete: Option<&Path>,
        arm: &mut Option<PathBuf>,
        choice: &mut Option<RecordingsChoice>,
    ) {
        if is_live {
            ui.label(
                RichText::new("● en cours")
                    .font(theme::sans(12.0))
                    .color(color::SUCCESS),
            );
        } else {
            // Suppression en deux temps : le premier clic arme, le second confirme.
            let armed = confirm_delete == Some(recording.path.as_path());
            let label = if armed {
                "Confirmer la suppression"
            } else {
                "Supprimer"
            };
            let delete = egui::Button::new(RichText::new(label).color(color::DANGER)).frame(false);
            if ui.add(delete).clicked() {
                if armed {
                    *choice = Some(RecordingsChoice::Delete(recording.path.clone()));
                } else {
                    *arm = Some(recording.path.clone());
                }
            }
        }
        if widgets::secondary_button(ui, "Exporter .txt").clicked() {
            *choice = Some(RecordingsChoice::Export(recording.clone()));
        }
    }
}
