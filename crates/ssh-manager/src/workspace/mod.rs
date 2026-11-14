//! L'espace de travail : tout ce qui s'affiche une fois le coffre ouvert.

mod form;
mod inspector;
mod list;
mod password_prompt;
mod rail;
mod sessions;
mod top_bar;

use std::time::{Duration, Instant};

use egui::{CornerRadius, Frame, Margin, RichText, Ui};
use ssh_manager_core::{Connection, GroupFilter, KindFilter};
use ssh_manager_vault::{Vault, VaultData};
use tokio::runtime::Runtime;

use crate::session::ssh::SshSession;
use crate::theme::{self, color, radius};

pub use form::ConnectionForm;
pub use list::SortState;
use password_prompt::{PasswordPrompt, PromptResult};
use sessions::{ActiveTab, INITIAL_SIZE, OpenSession};

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
    CloseSession(u64),
    Reconnect(u64),
    RetryWithPassword(u64),
    TrustHostKey {
        session: u64,
        fingerprint: String,
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
    password_prompt: Option<PasswordPrompt>,
    notice: Option<Notice>,
    pub rail: rail::RailState,
    ctx: egui::Context,
    /// Moteur asynchrone des sessions réseau, propre à l'espace de travail.
    runtime: Runtime,
    pub sessions: Vec<OpenSession>,
    next_session_id: u64,
    pub active: ActiveTab,
}

impl Workspace {
    pub fn new(ctx: egui::Context, vault: Vault, data: VaultData, notice: Option<String>) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("ssh-session")
            .enable_all()
            .build()
            .expect("le système refuse la création des fils réseau");
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
            password_prompt: None,
            notice: None,
            rail: rail::RailState::default(),
            ctx,
            runtime,
            sessions: Vec::new(),
            next_session_id: 1,
            active: ActiveTab::Home,
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
        self.pin_learned_host_keys();

        top_bar::show(self, ui, &mut actions);
        sessions::tab_bar(self, ui, &mut actions);
        self.status_bar(ui);

        egui::CentralPanel::no_frame().show(ui, |ui| {
            Frame::new()
                .inner_margin(Margin {
                    left: 14,
                    right: 14,
                    top: 0,
                    bottom: 14,
                })
                .show(ui, |ui| match self.active {
                    ActiveTab::Home => self.home(ui, &mut actions),
                    ActiveTab::Session(id) => {
                        if let Some(session) =
                            self.sessions.iter_mut().find(|session| session.id == id)
                        {
                            sessions::session_panel(session, ui, &mut actions);
                        }
                    }
                });
        });

        self.modals(ui, &mut actions);
        self.show_notice(ui);
        for action in actions {
            self.apply(action);
        }
    }

    fn home(&mut self, ui: &mut Ui, actions: &mut Vec<Action>) {
        if !self.rail_collapsed {
            egui::Panel::left("group-rail")
                .exact_size(208.0)
                .resizable(false)
                .show_separator_line(false)
                .frame(Frame::new().inner_margin(Margin {
                    right: 14,
                    ..Default::default()
                }))
                .show(ui, |ui| rail::show(self, ui, actions));
        }
        egui::Panel::right("inspector")
            .exact_size(296.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().inner_margin(Margin {
                left: 14,
                ..Default::default()
            }))
            .show(ui, |ui| inspector::show(self, ui, actions));
        egui::CentralPanel::no_frame().show(ui, |ui| list::show(self, ui, actions));
    }

    fn modals(&mut self, ui: &Ui, actions: &mut Vec<Action>) {
        if let Some(form) = &mut self.form
            && let Some(result) = form.show(ui.ctx(), &self.data.library.groups)
        {
            if let form::FormResult::Saved(connection) = result {
                actions.push(Action::Save(connection));
            }
            self.form = None;
        }
        if let Some(prompt) = &mut self.password_prompt
            && let Some(result) = prompt.show(ui.ctx())
        {
            self.password_prompt = None;
            if let PromptResult::Confirmed {
                connection,
                remember,
            } = result
            {
                if remember {
                    self.data.library.upsert(connection.clone());
                    self.persist();
                }
                self.start_session(connection);
            }
        }
    }

    /// Épingle dans le coffre les clés d'hôte découvertes à la première connexion.
    fn pin_learned_host_keys(&mut self) {
        let learned: Vec<(String, String)> = self
            .sessions
            .iter()
            .filter_map(|session| {
                session
                    .ssh
                    .take_learned_host_key()
                    .map(|key| (session.connection.host_key_id(), key))
            })
            .collect();
        if learned.is_empty() {
            return;
        }
        for (host, fingerprint) in learned {
            tracing::info!(%host, %fingerprint, "host key pinned on first use");
            self.data.host_keys.insert(host, fingerprint);
        }
        self.persist();
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::Open(id) => self.open(&id),
            Action::CloseSession(id) => self.close_session(id),
            Action::Reconnect(id) => self.reconnect(id, false),
            Action::RetryWithPassword(id) => self.reconnect(id, true),
            Action::TrustHostKey {
                session,
                fingerprint,
            } => self.trust_host_key(session, fingerprint),
            other => self.apply_library(other),
        }
    }

    fn apply_library(&mut self, action: Action) {
        let library = &mut self.data.library;
        match action {
            Action::Select(id) => {
                if self.selected.as_deref() != Some(id.as_str()) {
                    self.confirm_delete = None;
                }
                self.selected = Some(id);
                return;
            }
            Action::NewConnection { group } => {
                let mut connection = Connection::new(now_millis());
                connection.group = group;
                self.form = Some(ConnectionForm::new(connection, false));
                return;
            }
            Action::Edit(id) => {
                if let Some(connection) = library.find(&id) {
                    self.form = Some(ConnectionForm::new(connection.clone(), true));
                }
                return;
            }
            Action::Save(connection) => {
                self.selected = Some(connection.id.clone());
                library.upsert(*connection);
            }
            Action::Delete(id) => {
                library.remove(&id);
                self.selected = None;
            }
            Action::ToggleFavorite(id) => library.toggle_favorite(&id),
            Action::CreateGroup(name) => library.create_group(&name),
            Action::RenameGroup { from, to } => {
                if !library.rename_group(&from, &to) {
                    self.notify("Ce nom de groupe est vide ou déjà pris.", false);
                    return;
                }
                if self.group_filter == GroupFilter::Group(from) {
                    self.group_filter = GroupFilter::Group(to.trim().to_owned());
                }
            }
            Action::DeleteGroup(name) => {
                library.delete_group(&name);
                if self.group_filter == GroupFilter::Group(name) {
                    self.group_filter = GroupFilter::All;
                }
            }
            Action::SetGroupColor { group, color } => library.set_group_color(&group, color),
            _ => return,
        }
        self.persist();
    }

    /// Ouvre une connexion enregistrée : demande le mot de passe si besoin.
    fn open(&mut self, id: &str) {
        let Some(connection) = self.data.library.find(id).cloned() else {
            return;
        };
        if connection.kind.is_file_transfer() {
            self.notify(
                "Le navigateur de fichiers SFTP/FTP arrive à l'étape suivante.",
                true,
            );
            return;
        }
        if connection.needs_password_prompt() {
            self.password_prompt = Some(PasswordPrompt::new(connection));
        } else {
            self.start_session(connection);
        }
    }

    fn start_session(&mut self, connection: Connection) {
        self.data.library.mark_used(&connection.id, now_millis());
        self.persist();
        let pinned = self.data.host_keys.get(&connection.host_key_id()).cloned();
        let ssh = SshSession::start(
            &self.runtime,
            self.ctx.clone(),
            connection.clone(),
            pinned,
            INITIAL_SIZE,
        );
        let id = self.next_session_id;
        self.next_session_id += 1;
        self.sessions.push(OpenSession {
            id,
            connection,
            ssh,
            size: INITIAL_SIZE,
        });
        self.active = ActiveTab::Session(id);
    }

    fn close_session(&mut self, id: u64) {
        // L'abandon de la session (Drop) ferme proprement la connexion.
        self.sessions.retain(|session| session.id != id);
        if self.active == ActiveTab::Session(id) {
            self.active = self
                .sessions
                .last()
                .map_or(ActiveTab::Home, |session| ActiveTab::Session(session.id));
        }
    }

    /// Relance une session dans le même onglet, avec un nouveau mot de passe si demandé.
    fn reconnect(&mut self, id: u64, ask_password: bool) {
        let Some(index) = self.sessions.iter().position(|session| session.id == id) else {
            return;
        };
        let session = self.sessions.remove(index);
        if self.active == ActiveTab::Session(id) {
            self.active = ActiveTab::Home;
        }
        let mut connection = session.connection;
        if ask_password {
            connection.password = None;
            self.password_prompt = Some(PasswordPrompt::new(connection));
        } else {
            self.start_session(connection);
        }
    }

    fn trust_host_key(&mut self, id: u64, fingerprint: String) {
        let Some(session) = self.sessions.iter().find(|session| session.id == id) else {
            return;
        };
        let host = session.connection.host_key_id();
        tracing::warn!(%host, %fingerprint, "host key replaced at the user's request");
        self.data.host_keys.insert(host, fingerprint);
        self.persist();
        self.reconnect(id, false);
    }

    fn status_bar(&self, ui: &mut Ui) {
        let live = self
            .sessions
            .iter()
            .filter(|session| session.ssh.state().is_live())
            .count();
        let sessions = match live {
            0 => "aucune session active".to_owned(),
            1 => "1 session active".to_owned(),
            n => format!("{n} sessions actives"),
        };
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
                    let dot = if live > 0 {
                        color::SUCCESS
                    } else {
                        color::TEXT_DIM
                    };
                    ui.label(RichText::new("●").size(8.0).color(dot));
                    ui.label(
                        RichText::new(sessions)
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
