//! Panneau d'inspection : le détail de la connexion sélectionnée et ses actions.

use egui::{CornerRadius, Frame, Margin, RichText, Ui};
use ssh_manager_core::format;

use super::{Action, Workspace, now_millis};
use crate::theme::{self, color, radius};
use crate::widgets;

/// « SHA256:kQ8xR…7fA » : assez pour comparer d'un coup d'œil.
fn shorten_fingerprint(fingerprint: &str) -> String {
    if fingerprint.chars().count() <= 22 {
        return fingerprint.to_owned();
    }
    let head: String = fingerprint.chars().take(13).collect();
    let tail: String = fingerprint
        .chars()
        .rev()
        .take(3)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{head}…{tail}")
}

pub fn show(workspace: &mut Workspace, ui: &mut Ui, actions: &mut Vec<Action>) {
    widgets::card(color::SURFACE)
        .inner_margin(Margin::same(18))
        .show(ui, |ui| {
            ui.set_min_height(ui.available_height());
            ui.set_width(ui.available_width());
            let Some(connection) = workspace
                .selected
                .as_ref()
                .and_then(|id| workspace.data.library.find(id))
                .cloned()
            else {
                widgets::micro_label(ui, "Connexion");
                ui.add_space(8.0);
                ui.label(
                    RichText::new("Sélectionnez une connexion pour voir son détail.")
                        .color(color::TEXT_DIM),
                );
                return;
            };

            widgets::micro_label(ui, "Connexion");
            ui.label(RichText::new(&connection.name).font(theme::sans_bold(21.0)));
            ui.label(
                RichText::new(connection.target())
                    .font(theme::mono(12.0))
                    .color(color::TEXT_MUTED),
            );
            ui.add_space(12.0);

            let authentication = match (&connection.private_key_path, &connection.password) {
                (Some(_), _) => "Clé privée",
                (None, Some(_)) => "Mot de passe enregistré",
                (None, None) => "Mot de passe demandé",
            };
            let mut facts: Vec<(&str, String, bool)> = vec![
                (
                    "Groupe",
                    connection
                        .group
                        .clone()
                        .unwrap_or_else(|| "Sans groupe".into()),
                    false,
                ),
                ("Authentification", authentication.into(), false),
            ];
            if let Some(path) = &connection.private_key_path {
                let file = std::path::Path::new(path)
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned());
                facts.push(("Clé", file.unwrap_or_else(|| path.clone()), true));
            }
            if connection.kind.uses_host_key() {
                let fingerprint = workspace.data.host_keys.get(&connection.host_key_id());
                facts.push((
                    "Empreinte",
                    fingerprint
                        .map(|f| shorten_fingerprint(f))
                        .unwrap_or_else(|| "pas encore épinglée".into()),
                    true,
                ));
            }
            facts.push((
                "Favori",
                if connection.favorite { "Oui" } else { "Non" }.into(),
                false,
            ));
            facts_table(ui, &facts);

            ui.add_space(14.0);
            let primary = if connection.kind.is_file_transfer() {
                "Ouvrir les fichiers"
            } else {
                "Ouvrir un terminal"
            };
            if full_width_button(ui, primary, color::ACCENT, color::ON_ACCENT).clicked() {
                actions.push(Action::Open(connection.id.clone()));
            }
            if full_width_button(ui, "Modifier", color::ELEVATED, color::TEXT).clicked() {
                actions.push(Action::Edit(connection.id.clone()));
            }
            // Suppression en deux temps : un clic arme, le second confirme.
            let armed = workspace.confirm_delete.as_deref() == Some(connection.id.as_str());
            let label = if armed {
                "⚠ Confirmer la suppression"
            } else {
                "Supprimer"
            };
            if full_width_button(ui, label, egui::Color32::TRANSPARENT, color::DANGER).clicked() {
                if armed {
                    actions.push(Action::Delete(connection.id.clone()));
                    workspace.confirm_delete = None;
                } else {
                    workspace.confirm_delete = Some(connection.id.clone());
                }
            }

            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                let last = match connection.last_used_at {
                    Some(_) => format!(
                        "Dernier accès · {}",
                        format::last_used(connection.last_used_at, now_millis())
                    ),
                    None => "Jamais ouverte".into(),
                };
                ui.label(
                    RichText::new(last)
                        .font(theme::mono(10.0))
                        .color(color::TEXT_DIM),
                );
            });
        });
}

fn facts_table(ui: &mut Ui, facts: &[(&str, String, bool)]) {
    Frame::new()
        .fill(color::BASE)
        .corner_radius(CornerRadius::same(radius::SMALL))
        .inner_margin(Margin::symmetric(12, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            for (index, (label, value, monospace)) in facts.iter().enumerate() {
                if index > 0 {
                    ui.separator();
                }
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(*label)
                            .font(theme::sans(12.0))
                            .color(color::TEXT_DIM),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let font = if *monospace {
                            theme::mono(12.0)
                        } else {
                            theme::sans(12.0)
                        };
                        ui.label(RichText::new(value.as_str()).font(font));
                    });
                });
            }
        });
}

fn full_width_button(
    ui: &mut Ui,
    text: &str,
    fill: egui::Color32,
    text_color: egui::Color32,
) -> egui::Response {
    let font = if fill == color::ACCENT {
        theme::sans_semibold(13.0)
    } else {
        theme::sans(13.0)
    };
    let button = egui::Button::new(RichText::new(text).font(font).color(text_color))
        .fill(fill)
        .corner_radius(CornerRadius::same(radius::SMALL))
        .min_size(egui::vec2(ui.available_width(), 38.0));
    ui.add(button)
}

#[cfg(test)]
mod tests {
    use super::shorten_fingerprint;

    #[test]
    fn should_keep_short_fingerprints_intact() {
        assert_eq!(shorten_fingerprint("SHA256:abc"), "SHA256:abc");
    }

    #[test]
    fn should_shorten_long_fingerprints_around_an_ellipsis() {
        assert_eq!(
            shorten_fingerprint("SHA256:kQ8xR2mVv1a9Zt0LpQ7fA"),
            "SHA256:kQ8xR2…7fA"
        );
    }
}
