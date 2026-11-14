//! Éléments d'interface récurrents de la direction Minuit.

use egui::{Button, Color32, CornerRadius, Frame, Margin, Response, RichText, Stroke, Ui, Vec2};

use crate::theme::{self, color, radius};

/// Micro-libellé de section : mono, capitales, très espacé.
pub fn micro_label(ui: &mut Ui, text: &str) -> Response {
    ui.label(
        RichText::new(text.to_uppercase())
            .font(theme::mono(10.0))
            .color(color::TEXT_DIM)
            .extra_letter_spacing(1.2),
    )
}

/// Carte posée sur le fond, sans filet visible.
pub fn card(fill: Color32) -> Frame {
    Frame::new()
        .fill(fill)
        .corner_radius(CornerRadius::same(radius::MEDIUM))
        .inner_margin(Margin::same(14))
}

fn pill(text: &str, fill: Color32, text_color: Color32, bold: bool) -> Button<'static> {
    let font = if bold {
        theme::sans_semibold(13.0)
    } else {
        theme::sans(13.0)
    };
    Button::new(RichText::new(text.to_owned()).font(font).color(text_color))
        .fill(fill)
        .stroke(Stroke::NONE)
        .corner_radius(CornerRadius::same(radius::PILL))
        .min_size(Vec2::new(0.0, 34.0))
}

/// Action principale : aplat d'accent, texte encre.
pub fn primary_button(ui: &mut Ui, text: &str) -> Response {
    ui.add(pill(text, color::ACCENT, color::ON_ACCENT, true))
}

pub fn primary_button_enabled(ui: &mut Ui, enabled: bool, text: &str) -> Response {
    ui.add_enabled(enabled, pill(text, color::ACCENT, color::ON_ACCENT, true))
}

pub fn secondary_button(ui: &mut Ui, text: &str) -> Response {
    ui.add(pill(text, color::SURFACE, color::TEXT_MUTED, false))
}

/// Contrôle segmenté en pilule ; renvoie vrai si la sélection a changé.
pub fn segmented<T: PartialEq + Copy>(
    ui: &mut Ui,
    options: &[(T, &str)],
    selected: &mut T,
) -> bool {
    let mut changed = false;
    Frame::new()
        .fill(color::SURFACE)
        .corner_radius(CornerRadius::same(radius::PILL))
        .inner_margin(Margin::same(3))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.horizontal(|ui| {
                for (value, label) in options {
                    let active = *selected == *value;
                    let (fill, text) = if active {
                        (color::ACCENT, color::ON_ACCENT)
                    } else {
                        (Color32::TRANSPARENT, color::TEXT_MUTED)
                    };
                    let button = Button::new(
                        RichText::new(*label)
                            .font(if active {
                                theme::sans_semibold(12.0)
                            } else {
                                theme::sans(12.0)
                            })
                            .color(text),
                    )
                    .fill(fill)
                    .stroke(Stroke::NONE)
                    .corner_radius(CornerRadius::same(radius::PILL))
                    .min_size(Vec2::new(0.0, 28.0));
                    if ui.add(button).clicked() && !active {
                        *selected = *value;
                        changed = true;
                    }
                }
            });
        });
    changed
}

/// Champ de saisie au style du thème.
pub fn text_field<'a>(text: &'a mut String, hint: &str) -> egui::TextEdit<'a> {
    egui::TextEdit::singleline(text)
        .hint_text(RichText::new(hint).color(color::TEXT_DIM))
        .margin(Margin::symmetric(12, 9))
        .background_color(color::SURFACE)
}

/// Marque de l'application : chevron d'invite et curseur sur un carré violet.
pub fn brand_mark(ui: &mut Ui, size: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        CornerRadius::same((size * 0.234) as u8),
        color::ACCENT,
    );
    let unit = size / 1024.0;
    let at = |x: f32, y: f32| rect.min + Vec2::new(x * unit, y * unit);
    let stroke = Stroke::new((49.0 * unit).max(1.2), color::ON_ACCENT);
    // Géométrie reprise d'assets/icon.svg (chevron décalé pour centrer l'encre).
    let chevron = [at(314.3, 420.0), at(421.7, 512.0), at(314.3, 604.0)];
    painter.line_segment([chevron[0], chevron[1]], stroke);
    painter.line_segment([chevron[1], chevron[2]], stroke);
    painter.rect_filled(
        egui::Rect::from_min_max(at(663.0, 376.0), at(735.0, 648.0)),
        CornerRadius::same((24.0 * unit) as u8),
        color::ON_ACCENT,
    );
    response
}
