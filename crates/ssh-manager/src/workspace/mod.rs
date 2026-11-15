//! L'espace de travail : tout ce qui s'affiche une fois le coffre ouvert.

mod action;
mod auto_reconnect;
mod backup_dialogs;
mod backup_flow;
mod command_palette;
mod form;
mod inspector;
mod key_generator;
mod lifecycle;
mod list;
mod mosaic;
mod multi_session;
mod password_prompt;
mod rail;
mod recording_flow;
mod recordings_dialog;
mod search_bar;
mod sessions;
mod settings_dialog;
mod status_bar;
mod toolbox;
mod top_bar;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui::{CornerRadius, Frame, Margin, RichText, Ui};
use ssh_manager_core::{Connection, GroupFilter, KindFilter};
use ssh_manager_vault::{Vault, VaultData};
use tokio::runtime::Runtime;

use crate::files::FileBrowser;
use crate::files::view::BrowserView;
use crate::recordings::{self, Recorder, RecordingError};
use crate::secure_copy::SecureClipboard;
use crate::session::ssh::{SshSession, TerminalBuffers, lock};
use crate::settings::Settings;
use crate::theme::{self, color, radius};

use crate::session::view::TerminalOptions;
pub use action::Action;
use backup_dialogs::{PhraseOutcome, PhrasePrompt};
use command_palette::{CommandPalette, PaletteOutcome};
pub use form::ConnectionForm;
pub use list::SortState;
use mosaic::Mosaic;
use password_prompt::{PasswordPrompt, PromptResult, SecretKind};
use recordings_dialog::{RecordingsChoice, RecordingsDialog};
use sessions::{ActiveTab, INITIAL_SIZE, OpenSession, SessionKind, TypedInput};
use ssh_manager_vault::backup::{self, Backup};

const NOTICE_DURATION: Duration = Duration::from_secs(4);
/// Ligne grisée insérée dans le terminal à chaque reconnexion.
const RECONNECTION_MARKER: &str = "\r\n\x1b[2m── reconnexion ──\x1b[0m\r\n";

/// Message éphémère en bas de l'écran.
pub struct Notice {
    pub text: String,
    pub ok: bool,
    shown_at: Instant,
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
    pub settings: Settings,
    settings_open: bool,
    recordings_dialog: Option<RecordingsDialog>,
    clipboard: SecureClipboard,
    toolbox: toolbox::Toolbox,
    palette: Option<CommandPalette>,
    backup_phrase: Option<PhrasePrompt>,
    pub mosaic: Mosaic,
    /// Sessions dont la saisie est répliquée dans les autres.
    pub synced: BTreeSet<u64>,
    confirm_plain_export: bool,
    /// Onglet à relancer en place une fois le mot de passe saisi.
    reconnect_after_prompt: Option<u64>,
}

impl Workspace {
    pub fn new(ctx: egui::Context, vault: Vault, data: VaultData, notice: Option<String>) -> Self {
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
            runtime: network_runtime(),
            sessions: Vec::new(),
            next_session_id: 1,
            active: ActiveTab::Home,
            settings: Settings::load(),
            settings_open: false,
            recordings_dialog: None,
            clipboard: SecureClipboard::default(),
            toolbox: toolbox::Toolbox::default(),
            palette: None,
            backup_phrase: None,
            mosaic: Mosaic::default(),
            synced: BTreeSet::new(),
            confirm_plain_export: false,
            reconnect_after_prompt: None,
        };
        if let Some(text) = notice {
            workspace.notify(text, true);
        }
        workspace.purge_expired_recordings();
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
        self.clipboard.tick(ui.ctx());
        self.drive_auto_reconnect(ui.ctx());
        self.global_shortcuts(ui.ctx(), &mut actions);

        top_bar::show(self, ui, &mut actions);
        sessions::tab_bar(self, ui, &mut actions);
        self.status_bar(ui);

        let mut typed: Vec<TypedInput> = Vec::new();
        egui::CentralPanel::no_frame().show(ui, |ui| {
            Frame::new()
                .inner_margin(Margin {
                    left: 14,
                    right: 14,
                    top: 0,
                    bottom: 14,
                })
                .show(ui, |ui| typed = self.active_tab(ui, &mut actions));
        });

        self.replay_synced_input(typed);
        self.modals(ui, &mut actions);
        self.show_notice(ui);
        for action in actions {
            self.apply(action);
        }
    }

    /// Contenu de l'onglet actif ; renvoie la saisie faite dans un terminal.
    fn active_tab(&mut self, ui: &mut Ui, actions: &mut Vec<Action>) -> Vec<TypedInput> {
        let prefs = TerminalOptions::from_settings(&self.settings);
        match self.active {
            ActiveTab::Home => self.home(ui, actions),
            ActiveTab::Mosaic => {
                return sessions::mosaic_panel(
                    &mut self.sessions,
                    &self.mosaic,
                    &self.synced,
                    &prefs,
                    ui,
                    actions,
                );
            }
            ActiveTab::Toolbox => {
                if self
                    .toolbox
                    .show(ui, &mut self.clipboard, &mut self.settings)
                {
                    self.save_settings();
                }
            }
            ActiveTab::Session(id) => {
                if let Some(session) = self.sessions.iter_mut().find(|session| session.id == id) {
                    return sessions::session_panel(session, &prefs, ui, actions);
                }
            }
        }
        Vec::new()
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
        self.form_modal(ui.ctx(), actions);
        self.password_modal(ui.ctx());
        self.recordings_modal(ui.ctx());
        self.backup_modals(ui.ctx());
        self.palette_modal(ui.ctx(), actions);
        if self.settings_open && settings_dialog::show(ui.ctx(), &mut self.settings, actions) {
            self.settings_open = false;
            self.save_settings();
            self.purge_expired_recordings();
        }
    }

    fn form_modal(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        if let Some(form) = &mut self.form
            && let Some(result) = form.show(
                ctx,
                &self.data.library.groups,
                &mut self.clipboard,
                &self.settings,
            )
        {
            if let form::FormResult::Saved(connection) = result {
                actions.push(Action::Save(connection));
            }
            self.form = None;
        }
    }

    fn password_modal(&mut self, ctx: &egui::Context) {
        let Some(result) = self
            .password_prompt
            .as_mut()
            .and_then(|prompt| prompt.show(ctx))
        else {
            return;
        };
        self.password_prompt = None;
        match result {
            PromptResult::Confirmed {
                connection,
                kind,
                remember,
            } => {
                // Seul le secret saisi est enregistré : la connexion a pu être
                // ouverte sous un autre protocole (SSH ouverte en SFTP).
                if remember {
                    let library = &mut self.data.library;
                    match kind {
                        SecretKind::Password => {
                            library.set_password(&connection.id, connection.password.clone());
                        }
                        SecretKind::KeyPassphrase { .. } => library
                            .set_key_passphrase(&connection.id, connection.key_passphrase.clone()),
                    }
                    self.persist();
                }
                match self.reconnect_after_prompt.take() {
                    Some(id) => self.restart_in_place(id, Some(connection)),
                    None => self.start_session(connection),
                }
            }
            PromptResult::Cancelled => self.reconnect_after_prompt = None,
        }
    }

    fn palette_modal(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        if let Some(palette) = &mut self.palette {
            match palette.show(ctx, &self.data.library.connections) {
                Some(PaletteOutcome::Run(action)) => {
                    self.palette = None;
                    actions.push(action);
                }
                Some(PaletteOutcome::Close) => self.palette = None,
                None => {}
            }
        }
    }

    /// Raccourcis de l'application, retirés du flux avant que le terminal ne
    /// les voie (comme dans l'ancienne app, ils priment sur le shell).
    fn global_shortcuts(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let ctrl_shift = egui::Modifiers::CTRL | egui::Modifiers::SHIFT;
        let (palette, new_connection, toggle_mosaic, toggle_sync) = ctx.input_mut(|input| {
            (
                input.consume_key(egui::Modifiers::CTRL, egui::Key::K),
                input.consume_key(egui::Modifiers::CTRL, egui::Key::N),
                input.consume_key(egui::Modifiers::CTRL, egui::Key::M),
                input.consume_key(ctrl_shift, egui::Key::S),
            )
        });
        if toggle_mosaic {
            actions.push(Action::ToggleMosaic);
        }
        if toggle_sync && let ActiveTab::Session(id) = self.active {
            actions.push(Action::ToggleSync(id));
        }
        if palette {
            self.palette = match self.palette {
                Some(_) => None,
                None => Some(CommandPalette::default()),
            };
        }
        if new_connection && self.form.is_none() {
            actions.push(Action::NewConnection { group: None });
        }
    }

    fn save_settings(&mut self) {
        if let Err(error) = self.settings.save() {
            tracing::error!(%error, "failed to save the settings");
            self.notify(format!("Réglages non enregistrés : {error}"), false);
        }
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::Open(id) => self.open(&id),
            Action::OpenFiles(id) => self.open_files(&id),
            Action::OpenExternal { id, shell } => self.open_external(&id, shell),
            Action::ForgetHostKey(id) => self.forget_host_key(&id),
            Action::QuickConnect(connection) => {
                tracing::info!(target = %connection.target(), "quick connection opened");
                self.open_connection(*connection);
            }
            Action::SaveQuickTarget(connection) => {
                self.form = Some(ConnectionForm::new(*connection, false));
            }
            Action::CloseSession(id) => self.close_session(id),
            Action::Reconnect(id) => self.restart_in_place(id, None),
            Action::DuplicateSession(id) => self.duplicate_session(id),
            Action::CancelAutoReconnect(id) => {
                if let Some(session) = self.sessions.iter_mut().find(|session| session.id == id) {
                    session.auto_reconnect.give_up();
                }
            }
            Action::ExportSession(id) => self.export_session(id),
            Action::RetryWithPassword(id) => self.reconnect_asking(id, SecretKind::Password),
            Action::RetryWithKeyPassphrase { session, wrong } => {
                self.reconnect_asking(session, SecretKind::KeyPassphrase { wrong });
            }
            Action::TrustHostKey {
                session,
                fingerprint,
            } => self.trust_host_key(session, fingerprint),
            other => self.apply_interface(other),
        }
    }

    /// Fenêtres, réglages et disposition.
    fn apply_interface(&mut self, action: Action) {
        match action {
            Action::OpenSettings => self.settings_open = true,
            Action::OpenRecordings => self.open_recordings(),
            Action::OpenToolbox => self.active = ActiveTab::Toolbox,
            Action::ExportBackup { plain: false } => self.export_backup(false),
            Action::ExportBackup { plain: true } => self.confirm_plain_export = true,
            Action::ImportBackup => self.import_backup(),
            Action::ToggleMosaic => self.toggle_mosaic(),
            Action::PlaceInMosaic { session, slot } => {
                self.mosaic.place(session, slot);
                self.active = ActiveTab::Mosaic;
            }
            Action::RemoveFromMosaic(id) => {
                self.mosaic.remove(id);
                if self.mosaic.is_empty() && self.active == ActiveTab::Mosaic {
                    self.active = ActiveTab::Session(id);
                }
            }
            Action::ToggleSync(id) => self.toggle_sync(id),
            Action::SetTerminalFontSize(size) => {
                self.settings.terminal_font_size = size;
                self.save_settings();
            }
            Action::SetTerminalTheme(theme) => {
                self.settings.terminal_theme = theme;
                self.save_settings();
            }
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
            Action::Delete(id) => self.delete_connection(&id),
            Action::ToggleFavorite(id) => library.toggle_favorite(&id),
            other => return self.apply_groups(other),
        }
        self.persist();
    }

    /// Supprime une connexion. L'empreinte du serveur part avec la dernière
    /// connexion vers lui, pour qu'une réinstallation future puisse être épinglée.
    fn delete_connection(&mut self, id: &str) {
        let library = &mut self.data.library;
        if let Some(host) = library.find(id).map(Connection::host_key_id) {
            library.remove(id);
            let still_used = library
                .connections
                .iter()
                .any(|other| other.host_key_id() == host);
            if !still_used && self.data.host_keys.remove(&host).is_some() {
                tracing::info!(%host, "host key forgotten with its last connection");
            }
        }
        self.selected = None;
    }

    fn apply_groups(&mut self, action: Action) {
        let library = &mut self.data.library;
        match action {
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
}

/// Moteur asynchrone des sessions réseau : deux fils suffisent, les sessions
/// passent l'essentiel de leur temps à attendre le réseau.
fn network_runtime() -> Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("ssh-session")
        .enable_all()
        .build()
        .expect("le système refuse la création des fils réseau")
}

pub fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}
