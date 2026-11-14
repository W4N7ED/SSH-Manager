//! L'espace de travail : tout ce qui s'affiche une fois le coffre ouvert.

mod form;
mod inspector;
mod list;
mod rail;
mod top_bar;

use std::time::{Duration, Instant};

use egui::{CornerRadius, Frame, Margin, RichText, Ui};
use ssh_manager_core::{Connection, GroupFilter, KindFilter};
use ssh_manager_vault::{Vault, VaultData};

use crate::theme::{self, color, radius};

pub use form::ConnectionForm;
pub use list::SortState;

const NOTICE_DURATION: Duration = Duration::from_secs(4);

/// Message éphémère en bas de l'écran.
pub struct Notice {
    pub text: String,
    pub ok: bool,
    shown_at: Instant,
}

/// Demande émise par un panneau, traitée après le rendu de l'image : on évite
/// ainsi de modifier les données pendant qu'on les parcourt pour l'affichage.
pub enum Action {
    Select(String),
    Open(String),
    NewConnection {
        group: Option<String>,
    },
    Edit(String),
    Save(Box<Connection>),
    Delete(String),
    ToggleFavorite(String),
    CreateGroup(String),
    RenameGroup {
        from: String,
        to: String,
    },
    DeleteGroup(String),
    SetGroupColor {
        group: String,
        color: Option<String>,
    },
}

pub struct Workspace {
    vault: Vault,
    pub data: VaultData,
    pub group_filter: GroupFilter,
    pub kind_filter: KindFilter,
    pub search: String,
    pub rail_collapsed: bool,
    pub selected: Option<String>,
    /// Connexion dont la suppression attend une confirmation.
    pub confirm_delete: Option<String>,
    pub sort: SortState,
    form: Option<ConnectionForm>,
    notice: Option<Notice>,
    pub rail: rail::RailState,
}

impl Workspace {
    pub fn new(vault: Vault, data: VaultData, notice: Option<String>) -> Self {
        let mut workspace = Self {
            vault,
            data,
            group_filter: GroupFilter::All,
            kind_filter: KindFilter::All,
            search: String::new(),
            rail_collapsed: false,
            selected: None,
            confirm_delete: None,
            sort: SortState::default(),
            form: None,
            notice: None,
            rail: rail::RailState::default(),
        };
        if let Some(text) = notice {
            workspace.notify(text, true);
        }
        workspace
    }

    pub fn notify(&mut self, text: impl Into<String>, ok: bool) {
        self.notice = Some(Notice {
            text: text.into(),
            ok,
            shown_at: Instant::now(),
        });
    }

    /// Écrit le coffre ; une erreur d'écriture est signalée, jamais ignorée.
    fn persist(&mut self) {
        if let Err(error) = self.vault.save(&self.data) {
            tracing::error!(%error, "failed to save the vault");
            self.notify(format!("Enregistrement impossible : {error}"), false);
        }
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        let mut actions = Vec::new();

        top_bar::show(self, ui, &mut actions);
        self.status_bar(ui);

        egui::CentralPanel::no_frame().show(ui, |ui| {
            Frame::new()
                .inner_margin(Margin {
                    left: 14,
                    right: 14,
                    top: 0,
                    bottom: 14,
                })
                .show(ui, |ui| {
                    if !self.rail_collapsed {
                        egui::Panel::left("group-rail")
                            .exact_size(208.0)
                            .resizable(false)
                            .show_separator_line(false)
                            .frame(Frame::new().inner_margin(Margin {
                                right: 14,
                                ..Default::default()
                            }))
                            .show(ui, |ui| rail::show(self, ui, &mut actions));
                    }
                    egui::Panel::right("inspector")
                        .exact_size(296.0)
                        .resizable(false)
                        .show_separator_line(false)
                        .frame(Frame::new().inner_margin(Margin {
                            left: 14,
                            ..Default::default()
                        }))
                        .show(ui, |ui| inspector::show(self, ui, &mut actions));
                    egui::CentralPanel::no_frame()
                        .show(ui, |ui| list::show(self, ui, &mut actions));
                });
        });

        if let Some(form) = &mut self.form
            && let Some(result) = form.show(ui.ctx(), &self.data.library.groups)
        {
            if let form::FormResult::Saved(connection) = result {
                actions.push(Action::Save(connection));
            }
            self.form = None;
        }

        self.show_notice(ui);

        for action in actions {
            self.apply(action);
        }
    }

    fn apply(&mut self, action: Action) {
        let library = &mut self.data.library;
        match action {
            Action::Select(id) => {
                if self.selected.as_deref() != Some(id.as_str()) {
                    self.confirm_delete = None;
                }
                self.selected = Some(id);
            }
            Action::Open(id) => {
                // L'ouverture de session arrive avec le terminal ; on note déjà l'usage.
                let now = now_millis();
                library.mark_used(&id, now);
                self.persist();
                self.notify("L'ouverture de sessions arrive à l'étape suivante.", true);
            }
            Action::NewConnection { group } => {
                let mut connection = Connection::new(now_millis());
                connection.group = group;
                self.form = Some(ConnectionForm::new(connection, false));
            }
            Action::Edit(id) => {
                if let Some(connection) = library.find(&id) {
                    self.form = Some(ConnectionForm::new(connection.clone(), true));
                }
            }
            Action::Save(connection) => {
                let id = connection.id.clone();
                library.upsert(*connection);
                self.selected = Some(id);
                self.persist();
            }
            Action::Delete(id) => {
                if library.remove(&id).is_some() {
                    self.selected = None;
                    self.persist();
                }
            }
            Action::ToggleFavorite(id) => {
                library.toggle_favorite(&id);
                self.persist();
            }
            Action::CreateGroup(name) => {
                library.create_group(&name);
                self.persist();
            }
            Action::RenameGroup { from, to } => {
                if library.rename_group(&from, &to) {
                    if self.group_filter == GroupFilter::Group(from) {
                        self.group_filter = GroupFilter::Group(to.trim().to_owned());
                    }
                    self.persist();
                } else {
                    self.notify("Ce nom de groupe est vide ou déjà pris.", false);
                }
            }
            Action::DeleteGroup(name) => {
                library.delete_group(&name);
                if self.group_filter == GroupFilter::Group(name) {
                    self.group_filter = GroupFilter::All;
                }
                self.persist();
            }
            Action::SetGroupColor { group, color } => {
                library.set_group_color(&group, color);
                self.persist();
            }
        }
    }

    fn status_bar(&self, ui: &mut Ui) {
        egui::Panel::bottom("status-bar")
            .exact_size(30.0)
            .show_separator_line(false)
            .frame(
                Frame::new()
                    .fill(color::BASE)
                    .inner_margin(Margin::symmetric(20, 0))
                    .stroke(egui::Stroke::new(1.0, color::BORDER_SUBTLE)),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    let font = theme::mono(11.0);
                    ui.label(
                        RichText::new("0 session active")
                            .font(font.clone())
                            .color(color::TEXT_DIM),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                                .font(font.clone())
                                .color(color::TEXT_DIM),
                        );
                        ui.label(
                            RichText::new("coffre chiffré")
                                .font(font)
                                .color(color::TEXT_DIM),
                        );
                    });
                });
            });
    }

    fn show_notice(&mut self, ui: &mut Ui) {
        let Some(notice) = &self.notice else { return };
        if notice.shown_at.elapsed() > NOTICE_DURATION {
            self.notice = None;
            return;
        }
        ui.ctx().request_repaint_after(Duration::from_millis(250));
        egui::Area::new(egui::Id::unique("notice"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-20.0, -46.0))
            .show(ui.ctx(), |ui| {
                Frame::new()
                    .fill(color::ELEVATED)
                    .corner_radius(CornerRadius::same(radius::MEDIUM))
                    .inner_margin(Margin::symmetric(18, 12))
                    .stroke(egui::Stroke::new(
                        1.0,
                        if notice.ok {
                            color::SUCCESS
                        } else {
                            color::DANGER
                        },
                    ))
                    .show(ui, |ui| {
                        ui.label(RichText::new(notice.text.as_str()).font(theme::sans(13.0)))
                    });
            });
    }
}

pub fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}
