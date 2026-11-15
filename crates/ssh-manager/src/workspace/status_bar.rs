//! Barre d'état et notifications éphémères.

use super::*;

impl Workspace {
    pub(super) fn status_bar(&self, ui: &mut Ui) {
        let live = self
            .sessions
            .iter()
            .filter(|session| session.state().is_live())
            .count();
        egui::Panel::bottom("status-bar")
            .exact_size(30.0)
            .show_separator_line(false)
            .frame(
                Frame::new()
                    .fill(color::BASE)
                    .inner_margin(Margin::symmetric(20, 0))
                    .stroke(egui::Stroke::new(1.0, color::BORDER_SUBTLE)),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    live_sessions_label(ui, live);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        storage_labels(ui);
                        if let Some(release) = lock(&self.available_update).clone() {
                            update_link(ui, &release);
                        }
                    });
                });
            });
    }

    pub(super) fn show_notice(&mut self, ui: &mut Ui) {
        let Some(notice) = &self.notice else { return };
        if notice.shown_at.elapsed() > NOTICE_DURATION {
            self.notice = None;
            return;
        }
        ui.ctx().request_repaint_after(Duration::from_millis(250));
        egui::Area::new(egui::Id::unique("notice"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-20.0, -46.0))
            .show(ui.ctx(), |ui| {
                Frame::new()
                    .fill(color::ELEVATED)
                    .corner_radius(CornerRadius::same(radius::MEDIUM))
                    .inner_margin(Margin::symmetric(18, 12))
                    .stroke(egui::Stroke::new(
                        1.0,
                        if notice.ok {
                            color::SUCCESS
                        } else {
                            color::DANGER
                        },
                    ))
                    .show(ui, |ui| {
                        ui.label(RichText::new(notice.text.as_str()).font(theme::sans(13.0)))
                    });
            });
    }
}

fn live_sessions_label(ui: &mut Ui, live: usize) {
    let text = match live {
        0 => "aucune session active".to_owned(),
        1 => "1 session active".to_owned(),
        n => format!("{n} sessions actives"),
    };
    let dot = if live > 0 {
        color::SUCCESS
    } else {
        color::TEXT_DIM
    };
    ui.label(RichText::new("●").size(8.0).color(dot));
    ui.label(
        RichText::new(text)
            .font(theme::mono(11.0))
            .color(color::TEXT_DIM),
    );
}

/// Version, et où sont les données (installée ou portable).
fn storage_labels(ui: &mut Ui) {
    let font = theme::mono(11.0);
    ui.label(
        RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
            .font(font.clone())
            .color(color::TEXT_DIM),
    );
    let storage = if ssh_manager_vault::paths::is_portable() {
        "portable · coffre chiffré"
    } else {
        "coffre chiffré"
    };
    ui.label(RichText::new(storage).font(font).color(color::TEXT_DIM))
        .on_hover_text(format!(
            "Données : {}",
            ssh_manager_vault::paths::data_dir().display()
        ));
}

/// « ↑ Version 2.1.0 disponible » : ouvre la page de la version.
fn update_link(ui: &mut Ui, release: &crate::update_check::Release) {
    let link = egui::Button::new(
        RichText::new(format!("↑ Version {} disponible", release.version))
            .font(theme::mono(11.0))
            .color(color::ACCENT_LIGHT),
    )
    .frame(false);
    if ui
        .add(link)
        .on_hover_text("Ouvrir la page de la version pour télécharger l'installeur")
        .clicked()
    {
        crate::session::links::open(&release.page_url);
    }
}
