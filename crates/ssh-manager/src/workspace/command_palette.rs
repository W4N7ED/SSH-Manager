//! Palette Ctrl+K : ouvrir une connexion ou lancer une action au clavier.

use egui::{CornerRadius, Frame, Margin, RichText};
use ssh_manager_core::{Connection, ConnectionKind};

use super::Action;
use crate::session::terminal_theme::{self, TerminalTheme};
use crate::theme::{self, color, radius};

const MAX_VISIBLE_ITEMS: usize = 12;

/// Actions globales proposées à côté des connexions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GlobalAction {
    NewConnection,
    Toolbox,
    Recordings,
    Settings,
    Mosaic,
}

impl GlobalAction {
    const ALL: [Self; 5] = [
        Self::NewConnection,
        Self::Mosaic,
        Self::Toolbox,
        Self::Recordings,
        Self::Settings,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::NewConnection => "Nouvelle connexion…",
            Self::Toolbox => "Ouvrir la boîte à outils",
            Self::Recordings => "Historique des sessions",
            Self::Settings => "Réglages",
            Self::Mosaic => "Basculer en mosaïque",
        }
    }

    fn shortcut(self) -> Option<&'static str> {
        match self {
            Self::NewConnection => Some("Ctrl+N"),
            Self::Mosaic => Some("Ctrl+M"),
            _ => None,
        }
    }

    fn action(self) -> Action {
        match self {
            Self::NewConnection => Action::NewConnection { group: None },
            Self::Toolbox => Action::OpenToolbox,
            Self::Recordings => Action::OpenRecordings,
            Self::Settings => Action::OpenSettings,
            Self::Mosaic => Action::ToggleMosaic,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Item {
    Connection {
        id: String,
        label: String,
        detail: String,
        ssh: bool,
    },
    Global(GlobalAction),
    /// Index dans `terminal_theme::PRESETS`.
    Theme(usize),
}

/// Vrai si chaque mot de la requête apparaît dans l'un des champs.
pub fn matches(query: &str, fields: &[&str]) -> bool {
    let haystack = fields.join(" ").to_lowercase();
    query
        .to_lowercase()
        .split_whitespace()
        .all(|word| haystack.contains(word))
}

fn items(query: &str, connections: &[Connection]) -> Vec<Item> {
    let mut found: Vec<Item> = connections
        .iter()
        .filter(|connection| {
            matches(
                query,
                &[
                    &connection.name,
                    &connection.host,
                    &connection.username,
                    connection.group.as_deref().unwrap_or(""),
                    connection.kind.label(),
                ],
            )
        })
        .map(|connection| Item::Connection {
            id: connection.id.clone(),
            label: connection.name.clone(),
            detail: format!("{} · {}", connection.kind.label(), connection.target()),
            ssh: connection.kind == ConnectionKind::Ssh,
        })
        .collect();
    found.extend(
        GlobalAction::ALL
            .into_iter()
            .filter(|action| matches(query, &[action.label()]))
            .map(Item::Global),
    );
    found.extend(
        terminal_theme::PRESETS
            .iter()
            .enumerate()
            .filter(|(_, preset)| matches(query, &["thème", "theme", "terminal", preset.label]))
            .map(|(index, _)| Item::Theme(index)),
    );
    found
}

#[derive(Default)]
pub struct CommandPalette {
    query: String,
    highlighted: usize,
}

pub enum PaletteOutcome {
    Run(Action),
    Close,
}

impl CommandPalette {
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        connections: &[Connection],
    ) -> Option<PaletteOutcome> {
        let found = items(&self.query, connections);
        self.highlighted = self.highlighted.min(found.len().saturating_sub(1));
        let (down, up, enter, shift) = ctx.input_mut(|input| {
            (
                input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown),
                input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp),
                input.key_pressed(egui::Key::Enter),
                input.modifiers.shift,
            )
        });
        if down && self.highlighted + 1 < found.len() {
            self.highlighted += 1;
        }
        if up {
            self.highlighted = self.highlighted.saturating_sub(1);
        }

        let mut outcome = None;
        let modal = egui::Modal::new(egui::Id::unique("command-palette"))
            .backdrop_color(color::tint(egui::Color32::BLACK, 120))
            .frame(
                Frame::new()
                    .fill(color::BASE)
                    .stroke(egui::Stroke::new(1.0, color::BORDER))
                    .corner_radius(CornerRadius::same(radius::LARGE))
                    .inner_margin(Margin::same(14)),
            )
            .show(ctx, |ui| {
                ui.set_width(560.0);
                let field = ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .hint_text("Connexion, hôte, groupe ou action…")
                        .font(theme::mono(14.0))
                        .desired_width(f32::INFINITY),
                );
                field.request_focus();
                if field.changed() {
                    self.highlighted = 0;
                }
                ui.add_space(8.0);
                for (index, item) in found.iter().take(MAX_VISIBLE_ITEMS).enumerate() {
                    if let Some(chosen) = self.row(ui, item, index == self.highlighted) {
                        outcome = Some(chosen);
                    }
                }
                if found.is_empty() {
                    ui.label(RichText::new("Aucun résultat").color(color::TEXT_DIM));
                }
                ui.add_space(6.0);
                ui.label(
                    RichText::new(
                        "↑↓ choisir · Entrée ouvrir · Maj+Entrée ouvrir en SFTP · Échap fermer",
                    )
                    .font(theme::mono(10.0))
                    .color(color::TEXT_DIM),
                );
            });

        if enter && outcome.is_none() {
            outcome = found.get(self.highlighted).map(|item| run(item, shift));
        }
        if outcome.is_none() && modal.should_close() {
            outcome = Some(PaletteOutcome::Close);
        }
        outcome
    }

    fn row(&self, ui: &mut egui::Ui, item: &Item, highlighted: bool) -> Option<PaletteOutcome> {
        let (label, detail) = match item {
            Item::Connection { label, detail, .. } => (label.clone(), detail.clone()),
            Item::Global(action) => (
                action.label().to_owned(),
                action.shortcut().unwrap_or("").to_owned(),
            ),
            Item::Theme(index) => (
                format!(
                    "Thème du terminal : {}",
                    terminal_theme::PRESETS[*index].label
                ),
                String::new(),
            ),
        };
        let fill = if highlighted {
            color::ELEVATED
        } else {
            egui::Color32::TRANSPARENT
        };
        let response = Frame::new()
            .fill(fill)
            .corner_radius(CornerRadius::same(radius::SMALL))
            .inner_margin(Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let marker = match item {
                        Item::Connection { .. } => "●",
                        Item::Global(_) => "›",
                        Item::Theme(_) => "◐",
                    };
                    ui.label(RichText::new(marker).color(color::TEXT_DIM));
                    ui.label(RichText::new(label).font(theme::sans(13.0)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(detail)
                                .font(theme::mono(11.0))
                                .color(color::TEXT_DIM),
                        );
                    });
                });
            })
            .response
            .interact(egui::Sense::click());
        response.clicked().then(|| run(item, false))
    }
}

fn run(item: &Item, alternate: bool) -> PaletteOutcome {
    PaletteOutcome::Run(match item {
        Item::Connection { id, ssh: true, .. } if alternate => Action::OpenFiles(id.clone()),
        Item::Connection { id, .. } => Action::Open(id.clone()),
        Item::Global(action) => action.action(),
        Item::Theme(index) => {
            Action::SetTerminalTheme(TerminalTheme::from_preset(&terminal_theme::PRESETS[*index]))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_match_every_word_in_any_field() {
        assert!(matches("prod web", &["web-01", "prod"]));
        assert!(matches("WEB", &["web-01"]));
        assert!(!matches("prod db", &["web-01", "prod"]));
        assert!(matches("", &["anything"]));
    }

    #[test]
    fn should_list_matching_connections_before_actions() {
        let connection = Connection {
            name: "Réglages serveur".into(),
            host: "10.0.0.1".into(),
            username: "root".into(),
            ..Connection::new(0)
        };
        let found = items("réglages", &[connection]);
        assert!(matches!(found[0], Item::Connection { .. }));
        assert_eq!(found[1], Item::Global(GlobalAction::Settings));
    }

    #[test]
    fn should_offer_every_theme_when_searching_for_themes() {
        let found = items("thème", &[]);
        let themes = found
            .iter()
            .filter(|item| matches!(item, Item::Theme(_)))
            .count();
        assert_eq!(themes, terminal_theme::PRESETS.len());
        assert_eq!(items("dracula", &[]), [Item::Theme(2)]);
    }
}
