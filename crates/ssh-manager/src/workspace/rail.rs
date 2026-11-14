//! Rail des groupes : Toutes, Favoris, chaque groupe, Sans groupe.

use egui::{Color32, CornerRadius, Margin, RichText, Sense, Ui, Vec2};
use ssh_manager_core::GroupFilter;

use super::{Action, Workspace};
use crate::theme::{self, color, radius};
use crate::widgets;

/// Palette proposée pour les groupes et les connexions.
pub const COLOR_PALETTE: [&str; 12] = [
    "#ef4444", "#f97316", "#eab308", "#22c55e", "#14b8a6", "#3b82f6", "#8b5cf6", "#ec4899",
    "#64748b", "#a16207", "#0891b2", "#be185d",
];

#[derive(Default)]
pub struct RailState {
    new_group: Option<String>,
    renaming: Option<(String, String)>,
}

pub fn show(workspace: &mut Workspace, ui: &mut Ui, actions: &mut Vec<Action>) {
    widgets::card(color::SURFACE)
        .inner_margin(Margin::symmetric(10, 12))
        .show(ui, |ui| {
            ui.set_min_height(ui.available_height());
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                widgets::micro_label(ui, "Groupes");
            });
            ui.add_space(6.0);

            ui.spacing_mut().item_spacing.y = 2.0;
            item(workspace, ui, GroupFilter::All, "Toutes", None);
            item(workspace, ui, GroupFilter::Favorites, "Favoris", None);
            for group in workspace.data.library.groups.clone() {
                if renaming_row(workspace, ui, &group, actions) {
                    continue;
                }
                let dot = workspace
                    .data
                    .library
                    .group_colors
                    .get(&group)
                    .and_then(|hex| color::parse_hex(hex));
                let response = item(
                    workspace,
                    ui,
                    GroupFilter::Group(group.clone()),
                    &group,
                    Some(dot.unwrap_or(color::BORDER)),
                );
                group_menu(workspace, &response, &group, actions);
            }
            item(workspace, ui, GroupFilter::Ungrouped, "Sans groupe", None);

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(6.0);
            new_group_row(workspace, ui, actions);
        });
}

fn item(
    workspace: &mut Workspace,
    ui: &mut Ui,
    filter: GroupFilter,
    label: &str,
    dot: Option<Color32>,
) -> egui::Response {
    let count = workspace.data.library.count(&filter);
    let active = workspace.group_filter == filter;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::click());
    let fill = if active {
        color::tint(color::ACCENT, 36)
    } else if response.hovered() {
        color::ELEVATED
    } else {
        Color32::TRANSPARENT
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(radius::SMALL), fill);

    let mut cursor = rect.left_center() + Vec2::new(12.0, 0.0);
    if let Some(dot) = dot {
        let dot_rect = egui::Rect::from_center_size(cursor + Vec2::new(4.0, 0.0), Vec2::splat(8.0));
        ui.painter()
            .rect_filled(dot_rect, CornerRadius::same(3), dot);
        cursor.x += 18.0;
    }
    let font = if active {
        theme::sans_semibold(13.0)
    } else {
        theme::sans(13.0)
    };
    let text_color = if active || response.hovered() {
        color::TEXT
    } else {
        color::TEXT_MUTED
    };
    ui.painter()
        .text(cursor, egui::Align2::LEFT_CENTER, label, font, text_color);
    ui.painter().text(
        rect.right_center() - Vec2::new(12.0, 0.0),
        egui::Align2::RIGHT_CENTER,
        count.to_string(),
        theme::mono(11.0),
        if active {
            color::ACCENT_LIGHT
        } else {
            color::TEXT_DIM
        },
    );

    if response.clicked() {
        workspace.group_filter = filter;
    }
    response
}

fn group_menu(
    workspace: &mut Workspace,
    response: &egui::Response,
    group: &str,
    actions: &mut Vec<Action>,
) {
    response.context_menu(|ui| {
        if ui.button("✎ Renommer").clicked() {
            workspace.rail.renaming = Some((group.to_owned(), group.to_owned()));
            ui.close();
        }
        widgets::micro_label(ui, "Couleur du groupe");
        ui.horizontal_wrapped(|ui| {
            ui.set_max_width(190.0);
            if ui.button("✕").on_hover_text("Aucune couleur").clicked() {
                actions.push(Action::SetGroupColor {
                    group: group.to_owned(),
                    color: None,
                });
                ui.close();
            }
            for hex in COLOR_PALETTE {
                let swatch = egui::Button::new("")
                    .fill(color::parse_hex(hex).unwrap_or(color::BORDER))
                    .min_size(Vec2::splat(20.0));
                if ui.add(swatch).clicked() {
                    actions.push(Action::SetGroupColor {
                        group: group.to_owned(),
                        color: Some(hex.to_owned()),
                    });
                    ui.close();
                }
            }
        });
        ui.separator();
        if ui
            .button(RichText::new("✕ Supprimer le groupe").color(color::DANGER))
            .clicked()
        {
            actions.push(Action::DeleteGroup(group.to_owned()));
            ui.close();
        }
    });
}

/// Ligne de renommage ; renvoie vrai si elle remplace la ligne du groupe.
fn renaming_row(
    workspace: &mut Workspace,
    ui: &mut Ui,
    group: &str,
    actions: &mut Vec<Action>,
) -> bool {
    let Some((original, value)) = &mut workspace.rail.renaming else {
        return false;
    };
    if original != group {
        return false;
    }
    let response = ui.add(widgets::text_field(value, "Nom du groupe").desired_width(f32::INFINITY));
    response.request_focus();
    if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
        workspace.rail.renaming = None;
    } else if response.lost_focus() {
        actions.push(Action::RenameGroup {
            from: original.clone(),
            to: value.clone(),
        });
        workspace.rail.renaming = None;
    }
    true
}

fn new_group_row(workspace: &mut Workspace, ui: &mut Ui, actions: &mut Vec<Action>) {
    match &mut workspace.rail.new_group {
        Some(name) => {
            let response =
                ui.add(widgets::text_field(name, "Nom du groupe…").desired_width(f32::INFINITY));
            response.request_focus();
            if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                workspace.rail.new_group = None;
            } else if response.lost_focus() {
                if !name.trim().is_empty() {
                    actions.push(Action::CreateGroup(name.clone()));
                }
                workspace.rail.new_group = None;
            }
        }
        None => {
            let button = egui::Button::new(
                RichText::new("+ Nouveau groupe")
                    .font(theme::sans(12.0))
                    .color(color::TEXT_MUTED),
            )
            .fill(color::ELEVATED)
            .corner_radius(CornerRadius::same(radius::SMALL))
            .min_size(Vec2::new(ui.available_width(), 32.0));
            if ui.add(button).clicked() {
                workspace.rail.new_group = Some(String::new());
            }
        }
    }
}
