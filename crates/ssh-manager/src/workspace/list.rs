//! Liste des connexions : colonnes triables, cartes, navigation au clavier.

use std::cmp::Ordering;

use egui::{Color32, CornerRadius, RichText, Sense, Stroke, Ui, Vec2};
use ssh_manager_core::{Connection, format};

use super::{Action, Workspace, now_millis};
use crate::theme::{self, color, radius};

/// Les cellules sont peintes directement plutôt que posées en widgets : c'est
/// ce qui garde des lignes de hauteur fixe, avec texte tronqué à la colonne.
const ROW_HEIGHT: f32 = 50.0;
const HOST_WIDTH: f32 = 180.0;
const USER_WIDTH: f32 = 110.0;
const KIND_WIDTH: f32 = 62.0;
const ACCESS_WIDTH: f32 = 110.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortColumn {
    Name,
    Host,
    Username,
    Kind,
    LastUsed,
}

#[derive(Debug, Clone, Copy)]
pub struct SortState {
    pub column: SortColumn,
    pub ascending: bool,
}

impl Default for SortState {
    fn default() -> Self {
        Self {
            column: SortColumn::Name,
            ascending: true,
        }
    }
}

impl SortState {
    fn toggle(&mut self, column: SortColumn) {
        if self.column == column {
            self.ascending = !self.ascending;
        } else {
            self.column = column;
            // Le dernier accès se lit du plus récent au plus ancien.
            self.ascending = column != SortColumn::LastUsed;
        }
    }

    fn compare(&self, left: &Connection, right: &Connection) -> Ordering {
        let ordering = match self.column {
            SortColumn::Name => left.name.to_lowercase().cmp(&right.name.to_lowercase()),
            SortColumn::Host => left.host.cmp(&right.host),
            SortColumn::Username => left.username.cmp(&right.username),
            SortColumn::Kind => left.kind.label().cmp(right.kind.label()),
            SortColumn::LastUsed => left.last_used_at.cmp(&right.last_used_at),
        };
        if self.ascending {
            ordering
        } else {
            ordering.reverse()
        }
    }
}

pub fn show(workspace: &mut Workspace, ui: &mut Ui, actions: &mut Vec<Action>) {
    let library = &workspace.data.library;
    let mut visible: Vec<&Connection> = library.filter(
        &workspace.group_filter,
        workspace.kind_filter,
        &workspace.search,
    );
    visible.sort_by(|left, right| workspace.sort.compare(left, right));
    let ids: Vec<String> = visible
        .iter()
        .map(|connection| connection.id.clone())
        .collect();

    // La sélection suit le filtrage : elle reste sur une ligne visible.
    if !ids.is_empty()
        && !workspace
            .selected
            .as_ref()
            .is_some_and(|id| ids.contains(id))
    {
        actions.push(Action::Select(ids[0].clone()));
    }

    header(&mut workspace.sort, ui);
    ui.add_space(4.0);

    let now = now_millis();
    let rows_height = ui.available_height() - 28.0;
    egui::ScrollArea::vertical()
        .max_height(rows_height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 8.0;
            for connection in &visible {
                let selected = workspace.selected.as_deref() == Some(connection.id.as_str());
                let accent = library
                    .effective_color(connection)
                    .and_then(color::parse_hex);
                row(ui, connection, selected, accent, now, actions);
            }
            if visible.is_empty() {
                ui.add_space(20.0);
                let message = if workspace.search.trim().is_empty() {
                    "Aucune connexion dans cette vue.".to_owned()
                } else {
                    format!("Aucun résultat pour « {} ».", workspace.search.trim())
                };
                ui.label(RichText::new(message).color(color::TEXT_DIM));
            }
        });

    ui.add_space(6.0);
    ui.label(
        RichText::new("Entrée pour ouvrir · ↑↓ pour naviguer · F2 pour modifier")
            .font(theme::sans(12.0))
            .color(color::TEXT_DIM),
    );

    keyboard(workspace, ui, &ids, actions);
}

fn header(sort: &mut SortState, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        let name_width = ui.available_width()
            - HOST_WIDTH
            - USER_WIDTH
            - KIND_WIDTH
            - ACCESS_WIDTH
            - 16.0
            - 4.0 * 14.0;
        for (column, label, width) in [
            (SortColumn::Name, "Nom", name_width),
            (SortColumn::Host, "Hôte", HOST_WIDTH),
            (SortColumn::Username, "Utilisateur", USER_WIDTH),
            (SortColumn::Kind, "Type", KIND_WIDTH),
            (SortColumn::LastUsed, "Accès", ACCESS_WIDTH),
        ] {
            let active = sort.column == column;
            let arrow = match (active, sort.ascending) {
                (false, _) => "",
                (true, true) => " ▲",
                (true, false) => " ▼",
            };
            let text = RichText::new(format!("{}{arrow}", label.to_uppercase()))
                .font(theme::mono(10.0))
                .color(if active {
                    color::ACCENT_LIGHT
                } else {
                    color::TEXT_DIM
                });
            let button = egui::Button::new(text)
                .frame(false)
                .min_size(Vec2::new(width, 18.0));
            if ui.add_sized(Vec2::new(width, 18.0), button).clicked() {
                sort.toggle(column);
            }
            ui.add_space(14.0 - ui.spacing().item_spacing.x);
        }
    });
}

fn row(
    ui: &mut Ui,
    connection: &Connection,
    selected: bool,
    accent: Option<Color32>,
    now: i64,
    actions: &mut Vec<Action>,
) {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_HEIGHT), Sense::click());
    let painter = ui.painter();
    let fill = match (selected, response.hovered()) {
        (true, _) => color::tint(color::ACCENT, 36),
        (false, true) => color::ELEVATED,
        (false, false) => color::SURFACE,
    };
    let stroke = if selected {
        Stroke::new(1.0, color::ACCENT)
    } else {
        Stroke::NONE
    };
    painter.rect(
        rect,
        CornerRadius::same(radius::MEDIUM),
        fill,
        stroke,
        egui::StrokeKind::Inside,
    );

    let center_y = rect.center().y;
    let mut x = rect.left() + 16.0;
    painter.rect_filled(
        egui::Rect::from_center_size(egui::pos2(x + 4.5, center_y), Vec2::splat(9.0)),
        CornerRadius::same(3),
        accent.unwrap_or(color::BORDER),
    );
    x += 19.0;

    let name_width = rect.width()
        - HOST_WIDTH
        - USER_WIDTH
        - KIND_WIDTH
        - ACCESS_WIDTH
        - 35.0
        - 4.0 * 14.0
        - 16.0;
    let name_font = if selected {
        theme::sans_semibold(14.0)
    } else {
        theme::sans(14.0)
    };
    let name = if connection.favorite {
        format!("{}  ★", connection.name)
    } else {
        connection.name.clone()
    };
    let text = |x: f32, width: f32, value: String, font: egui::FontId, tint: Color32| {
        let galley = painter.layout(value, font, tint, f32::INFINITY);
        let clipped = painter.with_clip_rect(egui::Rect::from_min_size(
            egui::pos2(x, rect.top()),
            Vec2::new(width, rect.height()),
        ));
        clipped.galley(
            egui::pos2(x, center_y - galley.size().y / 2.0),
            galley,
            tint,
        );
    };
    text(x, name_width, name, name_font, color::TEXT);
    x += name_width + 14.0;
    text(
        x,
        HOST_WIDTH,
        format!("{}:{}", connection.host, connection.port),
        theme::mono(12.0),
        color::TEXT_MUTED,
    );
    x += HOST_WIDTH + 14.0;
    text(
        x,
        USER_WIDTH,
        connection.username.clone(),
        theme::mono(12.0),
        color::TEXT_MUTED,
    );
    x += USER_WIDTH + 14.0;

    let (pill_fill, pill_text) = if selected {
        (color::ACCENT_LIGHT, color::ON_ACCENT)
    } else {
        (color::ELEVATED, color::TEXT_MUTED)
    };
    let pill_galley = painter.layout_no_wrap(
        connection.kind.label().to_owned(),
        theme::mono(10.0),
        pill_text,
    );
    let pill_rect = egui::Rect::from_min_size(
        egui::pos2(x, center_y - 9.0),
        Vec2::new(pill_galley.size().x + 14.0, 18.0),
    );
    painter.rect_filled(pill_rect, CornerRadius::same(radius::PILL), pill_fill);
    painter.galley(
        pill_rect.center() - pill_galley.size() / 2.0,
        pill_galley,
        pill_text,
    );
    x += KIND_WIDTH + 14.0;
    text(
        x,
        ACCESS_WIDTH,
        format::last_used(connection.last_used_at, now),
        theme::sans(12.0),
        color::TEXT_DIM,
    );

    if response.clicked() {
        actions.push(Action::Select(connection.id.clone()));
    }
    if response.double_clicked() {
        actions.push(Action::Open(connection.id.clone()));
    }
    response.context_menu(|ui| {
        if ui.button("▶ Connecter").clicked() {
            actions.push(Action::Open(connection.id.clone()));
            ui.close();
        }
        if ui.button("✎ Modifier").clicked() {
            actions.push(Action::Edit(connection.id.clone()));
            ui.close();
        }
        let favorite = if connection.favorite {
            "☆ Retirer des favoris"
        } else {
            "★ Ajouter aux favoris"
        };
        if ui.button(favorite).clicked() {
            actions.push(Action::ToggleFavorite(connection.id.clone()));
            ui.close();
        }
        ui.separator();
        if ui
            .button(RichText::new("✕ Supprimer").color(color::DANGER))
            .clicked()
        {
            actions.push(Action::Delete(connection.id.clone()));
            ui.close();
        }
    });
}

fn keyboard(workspace: &Workspace, ui: &Ui, ids: &[String], actions: &mut Vec<Action>) {
    // Pas de raccourci pendant une saisie : les flèches et Entrée appartiennent au champ.
    if ids.is_empty() || ui.ctx().egui_wants_keyboard_input() {
        return;
    }
    let current = workspace
        .selected
        .as_ref()
        .and_then(|id| ids.iter().position(|candidate| candidate == id));
    let (down, up, enter, rename) = ui.input(|input| {
        (
            input.key_pressed(egui::Key::ArrowDown),
            input.key_pressed(egui::Key::ArrowUp),
            input.key_pressed(egui::Key::Enter),
            input.key_pressed(egui::Key::F2),
        )
    });
    let index = current.unwrap_or(0);
    if down {
        actions.push(Action::Select(ids[(index + 1).min(ids.len() - 1)].clone()));
    } else if up {
        actions.push(Action::Select(ids[index.saturating_sub(1)].clone()));
    } else if enter && let Some(index) = current {
        actions.push(Action::Open(ids[index].clone()));
    } else if rename && let Some(index) = current {
        actions.push(Action::Edit(ids[index].clone()));
    }
}
