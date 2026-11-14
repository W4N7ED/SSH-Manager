//! Panneau du navigateur de fichiers : fil d'Ariane, actions, liste, pied.

use chrono::{Local, TimeZone};
use egui::{Color32, CornerRadius, Frame, Margin, RichText, Sense, Stroke, Ui, Vec2};
use ssh_manager_core::format;

use super::browser::{FileBrowser, Listing};
use super::remote_path;
use crate::theme::{self, color, radius};
use crate::widgets;

const SIZE_WIDTH: f32 = 110.0;
const DATE_WIDTH: f32 = 160.0;
const RIGHTS_WIDTH: f32 = 88.0;
const ROW_HEIGHT: f32 = 36.0;

/// État d'affichage propre à l'onglet (sélection, saisie en cours).
#[derive(Default)]
pub struct BrowserView {
    selected: Option<String>,
    new_folder: Option<String>,
    confirm_delete: bool,
}

/// Demande de fermeture de l'onglet émise par le panneau.
pub struct CloseRequested;

pub fn show(ui: &mut Ui, browser: &FileBrowser, view: &mut BrowserView) -> Option<CloseRequested> {
    let listing = browser.listing();
    // Une sélection qui n'existe plus (après suppression, navigation) est oubliée.
    if view
        .selected
        .as_ref()
        .is_some_and(|name| !listing.entries.iter().any(|entry| &entry.name == name))
    {
        view.selected = None;
        view.confirm_delete = false;
    }
    let mut close = None;

    Frame::new()
        .fill(color::SURFACE)
        .corner_radius(CornerRadius::same(radius::MEDIUM))
        .show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            if header(ui, browser, view, &listing) {
                close = Some(CloseRequested);
            }
            ui.add(egui::Separator::default().spacing(0.0));
            new_folder_row(ui, browser, view);
            columns(ui);
            let rows_height = ui.available_height() - 38.0;
            egui::ScrollArea::vertical()
                .max_height(rows_height)
                .auto_shrink([false, false])
                .show(ui, |ui| rows(ui, browser, view, &listing));
            footer(ui, view, &listing);
        });
    close
}

/// Renvoie vrai si la fermeture de l'onglet est demandée.
fn header(ui: &mut Ui, browser: &FileBrowser, view: &mut BrowserView, listing: &Listing) -> bool {
    let mut close = false;
    let idle = listing.busy.is_none();
    Frame::new()
        .inner_margin(Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let parent = remote_path::parent(&listing.path);
                if ui
                    .add_enabled(parent.is_some() && idle, round_button("↑"))
                    .on_hover_text("Dossier parent")
                    .clicked()
                    && let Some(parent) = parent
                {
                    browser.open(parent);
                }
                if ui
                    .add_enabled(idle, round_button("↺"))
                    .on_hover_text("Actualiser")
                    .clicked()
                {
                    browser.refresh();
                }
                breadcrumb(ui, browser, &listing.path, idle);

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(round_button("✕"))
                        .on_hover_text("Fermer l'onglet")
                        .clicked()
                    {
                        close = true;
                    }
                    actions(ui, browser, view, listing, idle);
                });
            });
        });
    close
}

fn actions(
    ui: &mut Ui,
    browser: &FileBrowser,
    view: &mut BrowserView,
    listing: &Listing,
    idle: bool,
) {
    let selected = view
        .selected
        .as_ref()
        .and_then(|name| listing.entries.iter().find(|entry| &entry.name == name))
        .cloned();

    let delete_label = if view.confirm_delete {
        "⚠ Confirmer"
    } else {
        "Supprimer"
    };
    if ui
        .add_enabled(
            idle && selected.is_some(),
            toolbar_button(delete_label, color::DANGER),
        )
        .clicked()
        && let Some(entry) = &selected
    {
        if view.confirm_delete {
            browser.delete(entry.name.clone(), entry.is_dir);
            view.confirm_delete = false;
        } else {
            view.confirm_delete = true;
        }
    }
    if ui
        .add_enabled(idle, toolbar_button("Nouveau dossier", color::TEXT))
        .clicked()
    {
        view.new_folder = Some(String::new());
    }
    let can_download = selected.as_ref().is_some_and(|entry| !entry.is_dir);
    if ui
        .add_enabled(
            idle && can_download,
            toolbar_button("Télécharger", color::TEXT),
        )
        .clicked()
        && let Some(entry) = &selected
    {
        download(browser, &entry.name);
    }
    if ui
        .add_enabled(idle, toolbar_button("Envoyer", color::TEXT))
        .clicked()
    {
        // Plusieurs fichiers possibles : ils partent l'un après l'autre.
        for source in rfd::FileDialog::new()
            .set_title("Fichiers à envoyer")
            .pick_files()
            .unwrap_or_default()
        {
            browser.upload(source);
        }
    }
}

fn download(browser: &FileBrowser, name: &str) {
    let target = rfd::FileDialog::new()
        .set_title("Enregistrer sous")
        .set_file_name(name)
        .set_directory(dirs::download_dir().unwrap_or_default())
        .save_file();
    if let Some(target) = target {
        browser.download(name.to_owned(), target);
    }
}

fn breadcrumb(ui: &mut Ui, browser: &FileBrowser, path: &str, idle: bool) {
    let crumbs = remote_path::breadcrumb(path);
    let last = crumbs.len().saturating_sub(1);
    ui.spacing_mut().item_spacing.x = 2.0;
    for (index, (label, full_path)) in crumbs.into_iter().enumerate() {
        if index > 1 {
            ui.label(
                RichText::new("/")
                    .font(theme::mono(13.0))
                    .color(color::TEXT_DIM),
            );
        }
        let current = index == last;
        let (fill, text) = if current {
            (color::tint(color::ACCENT, 36), color::ACCENT_LIGHT)
        } else {
            (Color32::TRANSPARENT, color::TEXT_MUTED)
        };
        let crumb = egui::Button::new(RichText::new(label).font(theme::mono(13.0)).color(text))
            .fill(fill)
            .stroke(Stroke::NONE)
            .corner_radius(CornerRadius::same(radius::PILL));
        if ui.add_enabled(idle, crumb).clicked() && !current {
            browser.open(full_path);
        }
    }
    ui.spacing_mut().item_spacing.x = 8.0;
}

fn new_folder_row(ui: &mut Ui, browser: &FileBrowser, view: &mut BrowserView) {
    let Some(name) = &mut view.new_folder else {
        return;
    };
    let mut finished = false;
    Frame::new()
        .inner_margin(Margin::symmetric(14, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let field = ui
                    .add(widgets::text_field(name, "Nom du nouveau dossier").desired_width(280.0));
                field.request_focus();
                let valid = remote_path::is_valid_name(name);
                let submitted =
                    field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                if (widgets::primary_button_enabled(ui, valid, "Créer").clicked() || submitted)
                    && valid
                {
                    browser.create_dir(name.trim().to_owned());
                    finished = true;
                }
                if widgets::secondary_button(ui, "Annuler").clicked()
                    || ui.input(|input| input.key_pressed(egui::Key::Escape))
                {
                    finished = true;
                }
                if !valid && !name.is_empty() {
                    ui.label(RichText::new("Nom invalide").color(color::DANGER));
                }
            });
        });
    if finished {
        view.new_folder = None;
    }
}

fn columns(ui: &mut Ui) {
    Frame::new()
        .inner_margin(Margin {
            left: 26,
            right: 26,
            top: 10,
            bottom: 4,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let name_width =
                    ui.available_width() - SIZE_WIDTH - DATE_WIDTH - RIGHTS_WIDTH - 3.0 * 16.0;
                for (label, width) in [
                    ("Nom", name_width),
                    ("Taille", SIZE_WIDTH),
                    ("Modifié", DATE_WIDTH),
                    ("Droits", RIGHTS_WIDTH),
                ] {
                    ui.add_sized(
                        Vec2::new(width, 14.0),
                        egui::Label::new(
                            RichText::new(label.to_uppercase())
                                .font(theme::mono(10.0))
                                .color(color::TEXT_DIM),
                        ),
                    );
                    ui.add_space(16.0 - ui.spacing().item_spacing.x);
                }
            });
        });
}

fn rows(ui: &mut Ui, browser: &FileBrowser, view: &mut BrowserView, listing: &Listing) {
    Frame::new()
        .inner_margin(Margin::symmetric(10, 0))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            if let Some(parent) = remote_path::parent(&listing.path) {
                let response = row(ui, "..", None, false, None, None, false);
                if response.double_clicked() {
                    browser.open(parent);
                }
            }
            for entry in &listing.entries {
                let selected = view.selected.as_deref() == Some(entry.name.as_str());
                let size = (!entry.is_dir).then(|| format::file_size(entry.size));
                let modified = entry
                    .modified
                    .and_then(|seconds| Local.timestamp_opt(seconds, 0).single())
                    .map(|date| date.format("%d/%m/%Y %H:%M").to_string());
                let response = row(
                    ui,
                    &entry.name,
                    size,
                    entry.is_dir,
                    modified,
                    entry.permissions.as_deref(),
                    selected,
                );
                if response.clicked() {
                    view.selected = Some(entry.name.clone());
                    view.confirm_delete = false;
                }
                if response.double_clicked() {
                    if entry.is_dir {
                        browser.open(remote_path::join(&listing.path, &entry.name));
                    } else {
                        download(browser, &entry.name);
                    }
                }
            }
            if listing.entries.is_empty() && listing.busy.is_none() {
                ui.add_space(24.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("Dossier vide").color(color::TEXT_DIM))
                });
            }
        });
}

fn row(
    ui: &mut Ui,
    name: &str,
    size: Option<String>,
    is_dir: bool,
    modified: Option<String>,
    rights: Option<&str>,
    selected: bool,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_HEIGHT), Sense::click());
    let painter = ui.painter();
    let fill = match (selected, response.hovered()) {
        (true, _) => color::tint(color::ACCENT, 36),
        (false, true) => color::ELEVATED,
        (false, false) => Color32::TRANSPARENT,
    };
    painter.rect_filled(rect, CornerRadius::same(radius::SMALL), fill);

    let center_y = rect.center().y;
    let mut x = rect.left() + 16.0;
    if name != ".." {
        let marker = egui::Rect::from_center_size(egui::pos2(x + 3.5, center_y), Vec2::splat(7.0));
        if is_dir {
            painter.rect_filled(marker, CornerRadius::same(2), color::ACCENT_LIGHT);
        } else {
            painter.circle_filled(marker.center(), 3.5, color::TEXT_MUTED);
        }
    }
    x += 17.0;
    let name_width = rect.width() - SIZE_WIDTH - DATE_WIDTH - RIGHTS_WIDTH - 3.0 * 16.0 - 33.0;
    let cell = |x: f32, width: f32, text: &str, font: egui::FontId, tint: Color32| {
        let galley = painter.layout_no_wrap(text.to_owned(), font, tint);
        let clip =
            egui::Rect::from_min_size(egui::pos2(x, rect.top()), Vec2::new(width, rect.height()));
        painter.with_clip_rect(clip).galley(
            egui::pos2(x, center_y - galley.size().y / 2.0),
            galley,
            tint,
        );
    };
    let name_font = if selected {
        theme::sans_semibold(13.0)
    } else if name == ".." {
        theme::mono(13.0)
    } else {
        theme::sans(13.0)
    };
    cell(
        x,
        name_width,
        name,
        name_font,
        if name == ".." {
            color::TEXT_MUTED
        } else {
            color::TEXT
        },
    );
    x += name_width + 16.0;
    cell(
        x,
        SIZE_WIDTH,
        size.as_deref()
            .unwrap_or(if name == ".." { "" } else { "—" }),
        theme::mono(12.0),
        color::TEXT,
    );
    x += SIZE_WIDTH + 16.0;
    cell(
        x,
        DATE_WIDTH,
        modified.as_deref().unwrap_or(""),
        theme::sans(12.0),
        color::TEXT_MUTED,
    );
    x += DATE_WIDTH + 16.0;
    cell(
        x,
        RIGHTS_WIDTH,
        rights.unwrap_or(""),
        theme::mono(11.0),
        color::TEXT_DIM,
    );
    response
}

fn footer(ui: &mut Ui, view: &BrowserView, listing: &Listing) {
    ui.add(egui::Separator::default().spacing(0.0));
    Frame::new()
        .inner_margin(Margin::symmetric(18, 10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let font = theme::mono(11.0);
                let total: u64 = listing
                    .entries
                    .iter()
                    .filter(|entry| !entry.is_dir)
                    .map(|entry| entry.size)
                    .sum();
                let count = listing.entries.len();
                let plural = if count > 1 { "s" } else { "" };
                ui.label(
                    RichText::new(format!(
                        "{count} élément{plural} · {}",
                        format::file_size(total)
                    ))
                    .font(font.clone())
                    .color(color::TEXT_DIM),
                );
                if view.selected.is_some() {
                    ui.label(
                        RichText::new("1 sélectionné")
                            .font(font.clone())
                            .color(color::TEXT_DIM),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(activity) = &listing.busy {
                        ui.label(
                            RichText::new(activity.as_str())
                                .font(font.clone())
                                .color(color::ACCENT_LIGHT),
                        );
                        ui.spinner();
                    } else if let Some((message, ok)) = &listing.message {
                        let (prefix, tint) = if *ok {
                            ("✓ ", color::SUCCESS)
                        } else {
                            ("⚠ ", color::DANGER)
                        };
                        ui.label(
                            RichText::new(format!("{prefix}{message}"))
                                .font(font)
                                .color(tint),
                        );
                    }
                });
            });
        });
}

fn round_button(icon: &str) -> egui::Button<'static> {
    egui::Button::new(
        RichText::new(icon.to_owned())
            .font(theme::sans(12.0))
            .color(color::TEXT_MUTED),
    )
    .fill(color::ELEVATED)
    .stroke(Stroke::NONE)
    .corner_radius(CornerRadius::same(radius::PILL))
    .min_size(Vec2::splat(28.0))
}

fn toolbar_button(label: &str, tint: Color32) -> egui::Button<'static> {
    let fill = if tint == color::DANGER {
        Color32::TRANSPARENT
    } else {
        color::ELEVATED
    };
    egui::Button::new(
        RichText::new(label.to_owned())
            .font(theme::sans(12.0))
            .color(tint),
    )
    .fill(fill)
    .stroke(Stroke::NONE)
    .corner_radius(CornerRadius::same(radius::PILL))
    .min_size(Vec2::new(0.0, 30.0))
}
