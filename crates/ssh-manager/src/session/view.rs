//! Rendu du terminal dans egui, saisie clavier, sélection, défilement et
//! copier-coller.
//!
//! Chaque caractère est peint à la position exacte de sa cellule plutôt que
//! par lignes de texte : un glyphe absent de la police (boîtes, symboles) est
//! rendu par une police de repli de largeur différente, ce qui décalerait tout
//! le reste de la ligne.

use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::TermMode;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::CursorShape;
use egui::{
    Color32, CornerRadius, Event, EventFilter, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2,
};

use super::emulator::{Emulator, GridSize};
use super::keys;
use super::palette;
use super::ssh::{SshSession, lock};
use crate::clipboard;
use crate::settings::PasteMode;
use crate::theme;

const FONT_SIZE: f32 = 14.0;
const PADDING: Vec2 = Vec2::new(12.0, 10.0);
/// Lignes parcourues par cran de molette.
const LINES_PER_WHEEL_STEP: f32 = 3.0;

fn font() -> FontId {
    theme::mono(FONT_SIZE)
}

/// Dimensions d'une cellule, mesurées une fois sur la police chargée.
#[derive(Clone, Copy)]
struct CellMetrics {
    width: f32,
    height: f32,
}

fn measure(ui: &Ui) -> CellMetrics {
    ui.ctx().fonts_mut(|fonts| CellMetrics {
        width: fonts.glyph_width(&font(), 'M'),
        height: fonts.row_height(&font()),
    })
}

/// Ce que l'affichage du terminal renvoie à l'onglet.
pub struct TerminalResponse {
    /// Taille de grille en cours.
    pub size: GridSize,
    /// Entrée pressée dans un terminal déconnecté : l'utilisateur veut se reconnecter.
    pub reconnect_requested: bool,
}

/// Affiche la session. Hors connexion (`live` faux), l'écran reste lisible et
/// sélectionnable, mais la saisie ne part plus : seule Entrée a un effet.
pub fn show(
    ui: &mut Ui,
    session: &SshSession,
    paste_mode: PasteMode,
    live: bool,
) -> TerminalResponse {
    let metrics = measure(ui);
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let grid_rect = rect.shrink2(PADDING);
    let size = GridSize {
        columns: ((grid_rect.width() / metrics.width).floor() as u16).max(2),
        rows: ((grid_rect.height() / metrics.height).floor() as u16).max(2),
    };

    // Le terminal prend le clavier dès qu'il s'affiche, sauf si un champ
    // (recherche, formulaire) l'a déjà : on peut taper sans cliquer d'abord.
    let nothing_focused = ui.memory(|memory| memory.focused().is_none());
    if response.clicked() || response.drag_started() || nothing_focused {
        response.request_focus();
    }
    // Tab, flèches et Échap appartiennent au shell distant, pas à la navigation d'egui.
    ui.memory_mut(|memory| {
        memory.set_focus_lock_filter(
            response.id,
            EventFilter {
                tab: true,
                horizontal_arrows: true,
                vertical_arrows: true,
                escape: true,
            },
        )
    });

    let mut emulator = lock(&session.emulator);
    if emulator.resize(size) {
        session.resize(size);
    }
    handle_mouse(ui, &response, &mut emulator, grid_rect, metrics);
    let mut reconnect_requested = false;
    if !live {
        reconnect_requested =
            response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        // La sélection reste copiable : utile pour récupérer une sortie avant de reconnecter.
        let copy_requested = ui.input(|input| {
            input
                .events
                .iter()
                .any(|event| matches!(event, Event::Copy))
        });
        if response.has_focus()
            && copy_requested
            && let Some(text) = emulator
                .term()
                .selection_to_string()
                .filter(|text| !text.is_empty())
        {
            ui.ctx().copy_text(text);
        }
    } else if paste_mode == PasteMode::RightClick
        && response.secondary_clicked()
        && let Some(text) = clipboard::read_text()
    {
        response.request_focus();
        paste(&mut emulator, session, &text);
    }
    if live && response.has_focus() {
        handle_keyboard(ui, &mut emulator, session, paste_mode);
    }
    paint(
        ui,
        rect,
        grid_rect,
        metrics,
        &emulator,
        response.has_focus() && live,
    );
    TerminalResponse {
        size,
        reconnect_requested,
    }
}

fn paste(emulator: &mut Emulator, session: &SshSession, text: &str) {
    let bracketed = emulator.mode().contains(TermMode::BRACKETED_PASTE);
    let bytes = keys::paste(text, bracketed);
    if !bytes.is_empty() {
        emulator.scroll_to_bottom();
        session.send(bytes);
    }
}

fn handle_keyboard(ui: &Ui, emulator: &mut Emulator, session: &SshSession, paste_mode: PasteMode) {
    let mode = emulator.mode();
    let events = ui.input(|input| input.events.clone());
    for event in events {
        let bytes = match event {
            Event::Text(text) => Some(text.into_bytes()),
            Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } => keys::encode(key, modifiers, mode.contains(TermMode::APP_CURSOR)),
            Event::Paste(text) => match paste_mode {
                PasteMode::CtrlV => {
                    Some(keys::paste(&text, mode.contains(TermMode::BRACKETED_PASTE)))
                }
                // Ctrl+V n'est pas le geste de collage choisi : il part au shell
                // comme caractère de contrôle (insertion littérale sous bash).
                PasteMode::RightClick => Some(vec![0x16]),
            },
            // Ctrl+C : copie s'il y a une sélection, sinon interruption (comme l'ancienne app).
            Event::Copy => match emulator
                .term()
                .selection_to_string()
                .filter(|text| !text.is_empty())
            {
                Some(text) => {
                    ui.ctx().copy_text(text);
                    emulator.term_mut().selection = None;
                    None
                }
                None => Some(vec![0x03]),
            },
            Event::Cut => Some(vec![0x18]),
            _ => None,
        };
        if let Some(bytes) = bytes {
            emulator.scroll_to_bottom();
            session.send(bytes);
        }
    }
}

fn handle_mouse(
    ui: &Ui,
    response: &egui::Response,
    emulator: &mut Emulator,
    grid_rect: Rect,
    metrics: CellMetrics,
) {
    let display_offset = emulator.term().grid().display_offset();
    let to_point = |position: Pos2| -> (Point, Side) {
        let relative = position - grid_rect.min;
        let size = emulator.size();
        let column = (relative.x / metrics.width).clamp(0.0, f32::from(size.columns) - 1.0);
        let row = (relative.y / metrics.height).clamp(0.0, f32::from(size.rows) - 1.0);
        let side = if column.fract() < 0.5 {
            Side::Left
        } else {
            Side::Right
        };
        (
            Point::new(
                Line(row as i32 - display_offset as i32),
                Column(column as usize),
            ),
            side,
        )
    };

    if let Some(position) = response.interact_pointer_pos() {
        let (point, side) = to_point(position);
        if response.double_clicked() {
            emulator.term_mut().selection =
                Some(Selection::new(SelectionType::Semantic, point, side));
        } else if response.drag_started() {
            emulator.term_mut().selection =
                Some(Selection::new(SelectionType::Simple, point, side));
        } else if response.dragged()
            && let Some(selection) = emulator.term_mut().selection.as_mut()
        {
            selection.update(point, side);
        } else if response.clicked() {
            emulator.term_mut().selection = None;
        }
    }

    if response.hovered() {
        let scroll = ui.input(|input| input.smooth_scroll_delta.y);
        if scroll != 0.0 {
            let lines = (scroll / metrics.height * LINES_PER_WHEEL_STEP).round() as i32;
            if lines != 0 {
                emulator.scroll(lines);
            }
        }
    }
}

fn paint(
    ui: &Ui,
    rect: Rect,
    grid_rect: Rect,
    metrics: CellMetrics,
    emulator: &Emulator,
    focused: bool,
) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::ZERO, palette::BACKGROUND);

    let content = emulator.term().renderable_content();
    let display_offset = content.display_offset as i32;
    let cell_rect = |line: i32, column: usize, width_cells: f32| {
        let top_left = grid_rect.min
            + Vec2::new(
                column as f32 * metrics.width,
                (line + display_offset) as f32 * metrics.height,
            );
        Rect::from_min_size(
            top_left,
            Vec2::new(metrics.width * width_cells, metrics.height),
        )
    };

    for indexed in content.display_iter {
        let cell = indexed.cell;
        if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }
        let width_cells = if cell.flags.contains(Flags::WIDE_CHAR) {
            2.0
        } else {
            1.0
        };
        let target = cell_rect(indexed.point.line.0, indexed.point.column.0, width_cells);
        let (foreground, mut background) = palette::cell_colors(cell.fg, cell.bg, cell.flags);
        if content
            .selection
            .is_some_and(|range| range.contains(indexed.point))
        {
            background = palette::SELECTION;
        }
        if background != palette::BACKGROUND {
            painter.rect_filled(target, CornerRadius::ZERO, background);
        }
        if cell.c != ' ' && cell.c != '\0' {
            painter.text(
                target.left_top(),
                egui::Align2::LEFT_TOP,
                cell.c,
                font(),
                foreground,
            );
        }
        if cell.flags.contains(Flags::UNDERLINE) {
            painter.hline(
                target.x_range(),
                target.bottom() - 1.5,
                Stroke::new(1.0, foreground),
            );
        }
    }

    if content.cursor.shape != CursorShape::Hidden {
        let target = cell_rect(
            content.cursor.point.line.0,
            content.cursor.point.column.0,
            1.0,
        );
        if focused {
            painter.rect_filled(
                target,
                CornerRadius::same(1),
                palette::CURSOR.gamma_multiply(0.85),
            );
        } else {
            painter.rect_stroke(
                target,
                CornerRadius::same(1),
                Stroke::new(1.0, palette::CURSOR),
                egui::StrokeKind::Inside,
            );
        }
    }

    if content.display_offset > 0 {
        let label = format!("↑ {} lignes", content.display_offset);
        painter.text(
            rect.right_top() + Vec2::new(-14.0, 8.0),
            egui::Align2::RIGHT_TOP,
            label,
            theme::mono(11.0),
            Color32::from_gray(140),
        );
    }
}
