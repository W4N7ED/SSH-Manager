//! Fenêtre des réglages de l'application.

use egui::{CornerRadius, Frame, Margin, RichText};

use super::Action;
use crate::session::terminal_theme::{self, CUSTOM_ID, TerminalTheme};
use crate::settings::{PasteMode, Settings};
use crate::theme::{self, color, radius};
use crate::widgets;

/// Affiche les réglages ; renvoie vrai quand la fenêtre est fermée. Chaque
/// changement s'applique immédiatement, l'enregistrement se fait à la fermeture.
pub fn show(ctx: &egui::Context, settings: &mut Settings, actions: &mut Vec<Action>) -> bool {
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
            // Le contenu défile si la fenêtre est petite ; « Fermer » reste visible.
            let max_height = (ctx.content_rect().height() - 180.0).max(200.0);
            egui::ScrollArea::vertical()
                .max_height(max_height)
                .show(ui, |ui| {
                    paste_section(ui, settings);
                    ui.add_space(18.0);
                    theme_section(ui, settings);
                    ui.add_space(18.0);
                    reconnect_section(ui, settings);
                    ui.add_space(18.0);
                    recording_section(ui, settings);
                    ui.add_space(18.0);
                    backup_section(ui, actions);
                });

            ui.add_space(20.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::primary_button(ui, "Fermer").clicked() {
                    closed = true;
                }
            });
        });
    closed || modal.should_close()
}

fn paste_section(ui: &mut egui::Ui, settings: &mut Settings) {
    widgets::micro_label(ui, "Terminal — coller");
    ui.add_space(4.0);
    widgets::segmented(
        ui,
        &[
            (PasteMode::RightClick, PasteMode::RightClick.label()),
            (PasteMode::CtrlV, PasteMode::CtrlV.label()),
        ],
        &mut settings.paste_mode,
    );
    ui.add_space(6.0);
    let explanation = match settings.paste_mode {
        PasteMode::RightClick => {
            "Le clic droit colle le presse-papiers. Ctrl+V est transmis au shell (insertion littérale d'un caractère)."
        }
        PasteMode::CtrlV => {
            "Ctrl+V colle le presse-papiers. Le clic droit n'a pas d'effet dans le terminal."
        }
    };
    ui.label(
        RichText::new(explanation)
            .font(theme::sans(12.0))
            .color(color::TEXT_MUTED),
    );
    ui.label(
        RichText::new("Dans les deux cas, le texte est collé sur la ligne du prompt : un saut de ligne final n'est pas transmis, la commande n'est donc pas exécutée.")
            .font(theme::sans(12.0))
            .color(color::TEXT_DIM),
    );
}

fn theme_section(ui: &mut egui::Ui, settings: &mut Settings) {
    widgets::micro_label(
        ui,
        &format!("Terminal — thème : {}", settings.terminal_theme.label()),
    );
    ui.add_space(4.0);
    theme_picker(ui, &mut settings.terminal_theme);
}

fn reconnect_section(ui: &mut egui::Ui, settings: &mut Settings) {
    widgets::micro_label(ui, "Terminal — reconnexion");
    ui.add_space(4.0);
    ui.checkbox(
        &mut settings.auto_reconnect,
        RichText::new("Reconnecter automatiquement après une coupure").font(theme::sans(13.0)),
    );
    ui.label(
        RichText::new("Après une coupure réseau, un délai d'inactivité ou une mise en veille : jusqu'à 5 tentatives espacées (3 s à 1 min). Jamais après un « exit » volontaire.")
            .font(theme::sans(12.0))
            .color(color::TEXT_DIM),
    );
}

fn retention_picker(ui: &mut egui::Ui, settings: &mut Settings) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Conserver les enregistrements :").font(theme::sans(12.0)));
        egui::ComboBox::from_id_salt("recordings-retention")
            .selected_text(crate::recordings::retention_label(
                settings.recordings_retention_days,
            ))
            .show_ui(ui, |ui| {
                for option in crate::recordings::RETENTION_OPTIONS {
                    ui.selectable_value(
                        &mut settings.recordings_retention_days,
                        option,
                        crate::recordings::retention_label(option),
                    );
                }
            });
    });
}

fn recording_section(ui: &mut egui::Ui, settings: &mut Settings) {
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
    ui.add_space(4.0);
    retention_picker(ui, settings);
}

fn backup_section(ui: &mut egui::Ui, actions: &mut Vec<Action>) {
    widgets::micro_label(ui, "Sauvegarde");
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if widgets::secondary_button(ui, "Exporter (.enc chiffré)").clicked() {
            actions.push(Action::ExportBackup { plain: false });
        }
        if widgets::secondary_button(ui, "Importer…").clicked() {
            actions.push(Action::ImportBackup);
        }
        if widgets::secondary_button(ui, "Importer ~/.ssh/config")
            .on_hover_text(
                "Une connexion par bloc Host (HostName, User, Port, IdentityFile, ProxyJump, LocalForward)",
            )
            .clicked()
        {
            actions.push(Action::ImportSshConfig);
        }
        let plain = egui::Button::new(
            RichText::new("Exporter en clair (.json)")
                .font(theme::sans(12.0))
                .color(color::TEXT_MUTED),
        )
        .frame(false);
        if ui.add(plain).clicked() {
            actions.push(Action::ExportBackup { plain: true });
        }
    });
    ui.label(
        RichText::new("La sauvegarde chiffrée se restaure sur n'importe quel poste avec la phrase de 12 mots. Les sauvegardes de l'ancienne application s'importent aussi.")
            .font(theme::sans(12.0))
            .color(color::TEXT_DIM),
    );
}

/// Aperçu d'un thème : son fond, un texte et le curseur.
fn swatch(
    ui: &mut egui::Ui,
    label: &str,
    colors: terminal_theme::TerminalColors,
    selected: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(104.0, 46.0), egui::Sense::click());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(radius::SMALL), colors.background);
    let stroke = if selected {
        egui::Stroke::new(2.0, color::ACCENT)
    } else if response.hovered() {
        egui::Stroke::new(1.0, color::BORDER)
    } else {
        egui::Stroke::new(1.0, color::BORDER_SUBTLE)
    };
    painter.rect_stroke(
        rect,
        CornerRadius::same(radius::SMALL),
        stroke,
        egui::StrokeKind::Inside,
    );
    painter.text(
        rect.left_top() + egui::vec2(8.0, 7.0),
        egui::Align2::LEFT_TOP,
        "$ ls",
        theme::mono(11.0),
        colors.foreground,
    );
    let cursor = egui::Rect::from_min_size(
        rect.left_top() + egui::vec2(40.0, 8.0),
        egui::vec2(6.0, 12.0),
    );
    painter.rect_filled(cursor, CornerRadius::ZERO, colors.cursor);
    painter.text(
        rect.left_bottom() + egui::vec2(8.0, -6.0),
        egui::Align2::LEFT_BOTTOM,
        label,
        theme::sans(10.0),
        colors.foreground.gamma_multiply(0.8),
    );
    response
}

/// Thèmes prédéfinis, puis couleurs personnalisées (fond, texte, curseur).
fn theme_picker(ui: &mut egui::Ui, theme_setting: &mut TerminalTheme) {
    ui.horizontal_wrapped(|ui| {
        for preset in &terminal_theme::PRESETS {
            if swatch(
                ui,
                preset.label,
                preset.colors,
                theme_setting.id == preset.id,
            )
            .clicked()
            {
                *theme_setting = TerminalTheme::from_preset(preset);
            }
        }
    });
    ui.add_space(6.0);
    let mut colors = theme_setting.colors();
    let mut changed = false;
    ui.horizontal(|ui| {
        let custom = theme_setting.id == CUSTOM_ID;
        ui.label(
            RichText::new(if custom {
                "Personnalisé :"
            } else {
                "Personnaliser :"
            })
            .font(theme::sans(12.0))
            .color(color::TEXT_MUTED),
        );
        for (label, value) in [
            ("fond", &mut colors.background),
            ("texte", &mut colors.foreground),
            ("curseur", &mut colors.cursor),
        ] {
            changed |= ui.color_edit_button_srgba(value).changed();
            ui.label(
                RichText::new(label)
                    .font(theme::sans(12.0))
                    .color(color::TEXT_DIM),
            );
        }
    });
    if changed {
        *theme_setting = TerminalTheme::custom(colors);
    }
}
