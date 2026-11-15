//! Onglet « Boîte à outils » : les utilitaires qui n'appartiennent à aucune
//! connexion — générateur de mot de passe et réglages du presse-papiers.

use std::time::{Duration, Instant};

use egui::{Margin, RichText, Ui};
use ssh_manager_core::password::{self, PasswordOptions, Strength};

use crate::secure_copy::{self, SecureClipboard};
use crate::settings::Settings;
use crate::theme::{self, color};
use crate::widgets;

const NOTICE_DURATION: Duration = Duration::from_secs(4);

pub struct Toolbox {
    options: PasswordOptions,
    password: String,
    copy_notice: Option<(String, Instant)>,
}

impl Default for Toolbox {
    fn default() -> Self {
        let options = PasswordOptions::default();
        Self {
            password: password::generate(&options),
            options,
            copy_notice: None,
        }
    }
}

fn strength_color(strength: Strength) -> egui::Color32 {
    match strength {
        Strength::Weak => color::DANGER,
        Strength::Fair => color::WARNING,
        Strength::Strong | Strength::Excellent => color::SUCCESS,
    }
}

impl Toolbox {
    /// Renvoie vrai si un réglage a changé (à enregistrer).
    pub fn show(
        &mut self,
        ui: &mut Ui,
        clipboard: &mut SecureClipboard,
        settings: &mut Settings,
    ) -> bool {
        let mut settings_changed = false;
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.columns(2, |columns| {
                widgets::card(color::SURFACE)
                    .inner_margin(Margin::same(20))
                    .show(&mut columns[0], |ui| {
                        ui.set_width(ui.available_width());
                        self.password_card(ui, clipboard, settings);
                    });
                widgets::card(color::SURFACE)
                    .inner_margin(Margin::same(20))
                    .show(&mut columns[1], |ui| {
                        ui.set_width(ui.available_width());
                        settings_changed = clipboard_card(ui, settings);
                    });
            });
        });
        settings_changed
    }

    fn password_card(&mut self, ui: &mut Ui, clipboard: &mut SecureClipboard, settings: &Settings) {
        widgets::micro_label(ui, "Générateur");
        ui.label(RichText::new("Mot de passe").font(theme::sans_bold(19.0)));
        ui.add_space(12.0);

        let empty = self.password.is_empty();
        ui.horizontal(|ui| {
            let text = if empty {
                RichText::new("Activez au moins un jeu de caractères").color(color::TEXT_DIM)
            } else {
                RichText::new(&self.password).font(theme::mono(16.0))
            };
            ui.add(egui::Label::new(text).wrap());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(!empty, egui::Button::new("↺"))
                    .on_hover_text("Régénérer")
                    .clicked()
                {
                    self.regenerate();
                }
            });
        });
        ui.add_space(10.0);

        ui.horizontal(|ui| {
            let notice = self
                .copy_notice
                .as_ref()
                .filter(|(_, shown_at)| shown_at.elapsed() < NOTICE_DURATION)
                .map(|(text, _)| text.clone());
            if notice.is_some() {
                ui.ctx().request_repaint_after(Duration::from_millis(500));
            }
            let label = notice.unwrap_or_else(|| "Copier".to_owned());
            if widgets::primary_button_enabled(ui, !empty, &label).clicked() {
                let outcome = clipboard.copy(ui.ctx(), &self.password, settings);
                self.copy_notice = Some((outcome.describe(false), Instant::now()));
            }
            let entropy = password::entropy_bits(&self.options);
            let strength = Strength::from_entropy(entropy);
            ui.label(
                RichText::new(format!("{entropy} bits · robustesse {}", strength.label()))
                    .font(theme::mono(12.0))
                    .color(strength_color(strength)),
            );
        });
        ui.add_space(14.0);

        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Longueur");
            changed |= ui
                .add(egui::Slider::new(
                    &mut self.options.length,
                    password::LENGTH_MIN..=password::LENGTH_MAX,
                ))
                .changed();
        });
        ui.add_space(6.0);
        let toggles: [(&mut bool, &str); 5] = [
            (&mut self.options.lowercase, "Minuscules  a-z"),
            (&mut self.options.uppercase, "Majuscules  A-Z"),
            (&mut self.options.digits, "Chiffres  0-9"),
            (&mut self.options.symbols, "Caractères spéciaux"),
            (
                &mut self.options.exclude_ambiguous,
                "Exclure les caractères ambigus (l 1 I O 0 o)",
            ),
        ];
        for (value, label) in toggles {
            changed |= ui.checkbox(value, label).changed();
        }
        // Un changement d'option produit aussitôt un mot de passe cohérent.
        if changed {
            self.regenerate();
        }
    }

    fn regenerate(&mut self) {
        self.password = password::generate(&self.options);
        self.copy_notice = None;
    }
}

/// Renvoie vrai si un réglage a changé.
fn clipboard_card(ui: &mut Ui, settings: &mut Settings) -> bool {
    widgets::micro_label(ui, "Sécurité");
    ui.label(RichText::new("Presse-papiers").font(theme::sans_bold(19.0)));
    ui.add_space(8.0);
    ui.label(
        RichText::new(
            "S'applique à toutes les copies de secrets : mots de passe générés et clés publiques. \
             Le presse-papiers n'est effacé que s'il contient encore ce que vous avez copié. \
             Ces copies sont aussi tenues hors de l'historique Windows (Win+V) et du presse-papiers cloud.",
        )
        .font(theme::sans(12.0))
        .color(color::TEXT_MUTED),
    );
    ui.add_space(12.0);
    let mut changed = ui
        .checkbox(
            &mut settings.clipboard_auto_clear,
            "Effacer automatiquement après copie",
        )
        .changed();
    ui.add_enabled_ui(settings.clipboard_auto_clear, |ui| {
        ui.horizontal(|ui| {
            ui.label("Délai");
            egui::ComboBox::from_id_salt("clipboard-delay")
                .selected_text(secure_copy::format_delay(settings.clipboard_clear_seconds))
                .show_ui(ui, |ui| {
                    for seconds in secure_copy::CLEAR_DELAY_OPTIONS {
                        changed |= ui
                            .selectable_value(
                                &mut settings.clipboard_clear_seconds,
                                seconds,
                                secure_copy::format_delay(seconds),
                            )
                            .changed();
                    }
                });
        });
    });
    changed
}
