//! Habillage de la fenêtre : la barre de titre native de Windows est retirée
//! pour se fondre dans le fond de l'application. On redessine donc ce qu'elle
//! apportait : déplacement, double-clic pour agrandir, boutons réduire /
//! agrandir / fermer, et poignées de redimensionnement sur les bords.

use egui::{
    Align, Color32, CornerRadius, CursorIcon, Frame, Layout, Margin, Rect, ResizeDirection,
    RichText, Sense, Ui, Vec2, ViewportCommand,
};

use crate::theme::{self, color, radius};
use crate::widgets;

const TITLE_BAR_HEIGHT: f32 = 36.0;
const CONTROL_SIZE: Vec2 = Vec2::new(40.0, 28.0);
/// Épaisseur des poignées de redimensionnement le long des bords.
const RESIZE_MARGIN: f32 = 5.0;

fn is_maximized(ui: &Ui) -> bool {
    ui.input(|input| input.viewport().maximized.unwrap_or(false))
}

/// Barre de titre : marque à gauche, zone de déplacement, boutons à droite.
pub fn title_bar(ui: &mut Ui) {
    egui::Panel::top("title-bar")
        .exact_size(TITLE_BAR_HEIGHT)
        .show_separator_line(false)
        .frame(Frame::new().fill(color::BASE).inner_margin(Margin {
            left: 16,
            right: 6,
            top: 4,
            bottom: 0,
        }))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                widgets::brand_mark(ui, 14.0);
                ui.label(RichText::new("SSH Manager").font(theme::sans_semibold(13.0)));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    window_controls(ui);
                    drag_area(ui);
                });
            });
        });
}

fn window_controls(ui: &mut Ui) {
    if control(ui, "✕", color::DANGER)
        .on_hover_text("Fermer")
        .clicked()
    {
        ui.ctx().send_viewport_cmd(ViewportCommand::Close);
    }
    let maximized = is_maximized(ui);
    let (icon, hint) = if maximized {
        ("❐", "Restaurer")
    } else {
        ("□", "Agrandir")
    };
    if control(ui, icon, color::ELEVATED)
        .on_hover_text(hint)
        .clicked()
    {
        ui.ctx()
            .send_viewport_cmd(ViewportCommand::Maximized(!maximized));
    }
    if control(ui, "─", color::ELEVATED)
        .on_hover_text("Réduire")
        .clicked()
    {
        ui.ctx().send_viewport_cmd(ViewportCommand::Minimized(true));
    }
}

fn control(ui: &mut Ui, icon: &str, hover_fill: Color32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(CONTROL_SIZE, Sense::click());
    let hovered = response.hovered();
    if hovered {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(radius::SMALL), hover_fill);
    }
    let tint = match (hovered, hover_fill == color::DANGER) {
        (true, true) => color::ON_ACCENT,
        (true, false) => color::TEXT,
        (false, _) => color::TEXT_DIM,
    };
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        theme::sans(12.0),
        tint,
    );
    response
}

/// Le reste de la barre déplace la fenêtre ; un double-clic l'agrandit ou la restaure.
fn drag_area(ui: &mut Ui) {
    let size = Vec2::new(ui.available_width(), TITLE_BAR_HEIGHT);
    let (_, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
    if response.double_clicked() {
        let maximized = is_maximized(ui);
        ui.ctx()
            .send_viewport_cmd(ViewportCommand::Maximized(!maximized));
    } else if response.drag_started() {
        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
    }
}

/// Poignées invisibles sur les bords et les coins : sans cadre natif, c'est ce
/// qui permet encore de redimensionner la fenêtre à la souris.
pub fn resize_handles(ui: &Ui) {
    if is_maximized(ui) {
        return;
    }
    let window = ui.ctx().content_rect();
    let m = RESIZE_MARGIN;
    let corner = m * 3.0;
    let handles = [
        (
            Rect::from_min_max(window.left_top(), window.left_top() + Vec2::splat(corner)),
            ResizeDirection::NorthWest,
            CursorIcon::ResizeNwSe,
        ),
        (
            Rect::from_min_max(
                window.right_top() + Vec2::new(-corner, 0.0),
                window.right_top() + Vec2::new(0.0, corner),
            ),
            ResizeDirection::NorthEast,
            CursorIcon::ResizeNeSw,
        ),
        (
            Rect::from_min_max(
                window.left_bottom() + Vec2::new(0.0, -corner),
                window.left_bottom() + Vec2::new(corner, 0.0),
            ),
            ResizeDirection::SouthWest,
            CursorIcon::ResizeNeSw,
        ),
        (
            Rect::from_min_max(
                window.right_bottom() - Vec2::splat(corner),
                window.right_bottom(),
            ),
            ResizeDirection::SouthEast,
            CursorIcon::ResizeNwSe,
        ),
        (
            Rect::from_min_max(window.left_top(), window.right_top() + Vec2::new(0.0, m)),
            ResizeDirection::North,
            CursorIcon::ResizeVertical,
        ),
        (
            Rect::from_min_max(
                window.left_bottom() - Vec2::new(0.0, m),
                window.right_bottom(),
            ),
            ResizeDirection::South,
            CursorIcon::ResizeVertical,
        ),
        (
            Rect::from_min_max(window.left_top(), window.left_bottom() + Vec2::new(m, 0.0)),
            ResizeDirection::West,
            CursorIcon::ResizeHorizontal,
        ),
        (
            Rect::from_min_max(
                window.right_top() - Vec2::new(m, 0.0),
                window.right_bottom(),
            ),
            ResizeDirection::East,
            CursorIcon::ResizeHorizontal,
        ),
    ];
    for (index, (rect, direction, cursor)) in handles.into_iter().enumerate() {
        let response = ui.interact(
            rect,
            egui::Id::unique(("window-resize", index)),
            Sense::drag(),
        );
        if response.hovered() || response.dragged() {
            ui.ctx().set_cursor_icon(cursor);
        }
        if response.drag_started() {
            ui.ctx()
                .send_viewport_cmd(ViewportCommand::BeginResize(direction));
        }
    }
}
