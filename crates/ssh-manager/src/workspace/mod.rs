//! L'espace de travail : tout ce qui s'affiche une fois le coffre ouvert.

mod backup_dialogs;
mod command_palette;
mod form;
mod inspector;
mod key_generator;
mod list;
mod mosaic;
mod password_prompt;
mod rail;
mod recordings_dialog;
mod sessions;
mod settings_dialog;
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

use backup_dialogs::{PhraseOutcome, PhrasePrompt};
use command_palette::{CommandPalette, PaletteOutcome};
pub use form::ConnectionForm;
pub use list::SortState;
use mosaic::Mosaic;
use password_prompt::{PasswordPrompt, PromptResult};
use recordings_dialog::{RecordingsChoice, RecordingsDialog};
use sessions::{ActiveTab, INITIAL_SIZE, OpenSession, SessionKind, TerminalPrefs, TypedInput};
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

/// Demande émise par un panneau, traitée après le rendu de l'image : on évite
/// ainsi de modifier les données pendant qu'on les parcourt pour l'affichage.
pub enum Action {
    Select(String),
    Open(String),
    /// Ouvre une connexion SSH en navigateur de fichiers (SFTP).
    OpenFiles(String),
    /// Ouvre une connexion SSH dans PowerShell ou l'invite de commandes.
    OpenExternal {
        id: String,
        shell: crate::external_terminal::ExternalShell,
    },
    /// Retire l'empreinte épinglée d'un serveur (réinstallation légitime).
    ForgetHostKey(String),
    /// Connexion ponctuelle saisie dans la recherche, non enregistrée.
    QuickConnect(Box<Connection>),
    /// Ouvre le formulaire prérempli avec une saisie `user@hôte:port`.
    SaveQuickTarget(Box<Connection>),
    OpenSettings,
    OpenRecordings,
    OpenToolbox,
    /// Exporte toute la bibliothèque : `.enc` chiffré, ou `.json` en clair.
    ExportBackup {
        plain: bool,
    },
    ImportBackup,
    /// Ctrl+M : répartit les sessions en mosaïque, ou revient aux onglets.
    ToggleMosaic,
    PlaceInMosaic {
        session: u64,
        slot: usize,
    },
    RemoveFromMosaic(u64),
    /// Inclut ou retire une session de la saisie synchronisée.
    ToggleSync(u64),
    SetTerminalTheme(crate::session::terminal_theme::TerminalTheme),
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
    DuplicateSession(u64),
    ExportSession(u64),
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
        self.global_shortcuts(ui.ctx(), &mut actions);

        top_bar::show(self, ui, &mut actions);
        sessions::tab_bar(self, ui, &mut actions);
        self.status_bar(ui);

        let prefs = TerminalPrefs::from_settings(&self.settings);
        let mut typed: Vec<TypedInput> = Vec::new();
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
                    ActiveTab::Mosaic => {
                        typed = sessions::mosaic_panel(
                            &mut self.sessions,
                            &self.mosaic,
                            &self.synced,
                            &prefs,
                            ui,
                            &mut actions,
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
                        if let Some(session) =
                            self.sessions.iter_mut().find(|session| session.id == id)
                        {
                            typed = sessions::session_panel(session, &prefs, ui, &mut actions);
                        }
                    }
                });
        });

        self.replay_synced_input(typed);
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
            && let Some(result) = form.show(
                ui.ctx(),
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
        if let Some(prompt) = &mut self.password_prompt
            && let Some(result) = prompt.show(ui.ctx())
        {
            self.password_prompt = None;
            if let PromptResult::Confirmed {
                connection,
                remember,
            } = result
            {
                // Seul le mot de passe est enregistré : la connexion a pu être
                // ouverte sous un autre protocole (SSH ouverte en SFTP).
                if remember {
                    self.data
                        .library
                        .set_password(&connection.id, connection.password.clone());
                    self.persist();
                }
                match self.reconnect_after_prompt.take() {
                    Some(id) => self.restart_in_place(id, Some(connection)),
                    None => self.start_session(connection),
                }
            } else {
                self.reconnect_after_prompt = None;
            }
        }
        self.recordings_modal(ui.ctx());
        self.backup_modals(ui.ctx());
        if let Some(palette) = &mut self.palette {
            match palette.show(ui.ctx(), &self.data.library.connections) {
                Some(PaletteOutcome::Run(action)) => {
                    self.palette = None;
                    actions.push(action);
                }
                Some(PaletteOutcome::Close) => self.palette = None,
                None => {}
            }
        }
        if self.settings_open && settings_dialog::show(ui.ctx(), &mut self.settings, actions) {
            self.settings_open = false;
            self.save_settings();
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

    /// Rejoue dans les autres sessions synchronisées ce qui a été tapé dans
    /// l'une d'elles. Seules les sessions connectées reçoivent la saisie.
    fn replay_synced_input(&self, typed: Vec<TypedInput>) {
        for input in typed {
            for target in mosaic::replicas(&self.synced, input.source) {
                let Some(session) = self.sessions.iter().find(|session| session.id == target)
                else {
                    continue;
                };
                if let SessionKind::Terminal { ssh, .. } = &session.kind
                    && session.state().is_live()
                {
                    ssh.send(input.bytes.clone());
                }
            }
        }
    }

    fn toggle_mosaic(&mut self) {
        if self.mosaic.is_empty() {
            self.mosaic
                .fill(self.sessions.iter().map(|session| session.id));
            if !self.mosaic.is_empty() {
                self.active = ActiveTab::Mosaic;
            } else {
                self.notify("Ouvrez au moins une session pour la mosaïque.", false);
            }
        } else {
            self.mosaic.clear();
            if self.active == ActiveTab::Mosaic {
                self.active = self
                    .sessions
                    .first()
                    .map_or(ActiveTab::Home, |session| ActiveTab::Session(session.id));
            }
        }
    }

    fn toggle_sync(&mut self, id: u64) {
        let is_terminal = self
            .sessions
            .iter()
            .any(|session| session.id == id && session.is_terminal());
        if !is_terminal {
            return;
        }
        if !self.synced.remove(&id) {
            self.synced.insert(id);
        }
        tracing::info!(
            session = id,
            synced = self.synced.len(),
            "synchronized input changed"
        );
        if self.synced.len() == 1 {
            self.notify(
                "Synchronisez au moins une autre session pour répliquer la saisie.",
                true,
            );
        }
    }

    fn save_settings(&mut self) {
        if let Err(error) = self.settings.save() {
            tracing::error!(%error, "failed to save the settings");
            self.notify(format!("Réglages non enregistrés : {error}"), false);
        }
    }

    /// Épingle dans le coffre les clés d'hôte découvertes à la première connexion.
    fn pin_learned_host_keys(&mut self) {
        let learned: Vec<(String, String)> = self
            .sessions
            .iter()
            .filter_map(|session| {
                session
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
            Action::SetTerminalTheme(theme) => {
                self.settings.terminal_theme = theme;
                self.save_settings();
            }
            Action::CloseSession(id) => self.close_session(id),
            Action::Reconnect(id) => self.reconnect(id, false),
            Action::DuplicateSession(id) => self.duplicate_session(id),
            Action::ExportSession(id) => self.export_session(id),
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
                // Comme l'ancienne app : l'empreinte part avec la dernière connexion
                // vers ce serveur, pour qu'une réinstallation future puisse être épinglée.
                if let Some(host) = library.find(&id).map(Connection::host_key_id) {
                    library.remove(&id);
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

    /// Ouvre une connexion enregistrée avec son propre protocole : terminal pour
    /// SSH, navigateur de fichiers pour SFTP et FTP.
    fn open(&mut self, id: &str) {
        if let Some(connection) = self.data.library.find(id).cloned() {
            self.open_connection(connection);
        }
    }

    /// Ouvre une connexion SSH en navigateur de fichiers, sur le même serveur.
    fn open_files(&mut self, id: &str) {
        if let Some(mut connection) = self.data.library.find(id).cloned() {
            connection.kind = ssh_manager_core::ConnectionKind::Sftp;
            self.open_connection(connection);
        }
    }

    fn open_external(&mut self, id: &str, shell: crate::external_terminal::ExternalShell) {
        let Some(connection) = self.data.library.find(id).cloned() else {
            return;
        };
        if let Err(error) = crate::external_terminal::open(&connection, shell) {
            tracing::error!(%error, target = %connection.target(), "failed to open an external terminal");
            self.notify(format!("Ouverture impossible : {error}"), false);
        }
    }

    fn forget_host_key(&mut self, id: &str) {
        let Some(host) = self.data.library.find(id).map(Connection::host_key_id) else {
            return;
        };
        if self.data.host_keys.remove(&host).is_some() {
            tracing::warn!(%host, "host key forgotten at the user's request");
            self.persist();
            self.notify(
                "Empreinte oubliée : elle sera épinglée à la prochaine connexion.",
                true,
            );
        }
    }

    /// Demande le mot de passe si besoin, puis démarre la session.
    fn open_connection(&mut self, connection: Connection) {
        if connection.needs_password_prompt() {
            self.password_prompt = Some(PasswordPrompt::new(connection));
        } else {
            self.start_session(connection);
        }
    }

    fn start_session(&mut self, connection: Connection) {
        self.data.library.mark_used(&connection.id, now_millis());
        self.persist();
        let kind = if connection.kind.is_file_transfer() {
            self.start_files(&connection)
        } else {
            let recorder = if self.settings.record_sessions {
                Recorder::start(
                    &ssh_manager_vault::paths::recordings_dir(),
                    self.vault.recording_key(),
                    &connection,
                )
            } else {
                Recorder::disabled()
            };
            self.start_terminal(&connection, TerminalBuffers::new(INITIAL_SIZE, recorder))
        };
        let id = self.next_session_id;
        self.next_session_id += 1;
        self.sessions.push(OpenSession {
            id,
            connection,
            kind,
        });
        self.active = ActiveTab::Session(id);
    }

    fn close_session(&mut self, id: u64) {
        // L'abandon de la session (Drop) ferme proprement la connexion.
        self.sessions.retain(|session| session.id != id);
        self.mosaic.remove(id);
        self.synced.remove(&id);
        let mosaic_emptied = self.active == ActiveTab::Mosaic && self.mosaic.is_empty();
        if self.active == ActiveTab::Session(id) || mosaic_emptied {
            self.active = self
                .sessions
                .last()
                .map_or(ActiveTab::Home, |session| ActiveTab::Session(session.id));
        }
    }

    fn pinned_host_key(&self, connection: &Connection) -> Option<String> {
        self.data.host_keys.get(&connection.host_key_id()).cloned()
    }

    fn start_files(&self, connection: &Connection) -> SessionKind {
        let pinned = self.pinned_host_key(connection);
        SessionKind::Files {
            browser: FileBrowser::start(
                &self.runtime,
                self.ctx.clone(),
                connection.clone(),
                pinned,
            ),
            view: BrowserView::default(),
        }
    }

    fn start_terminal(&self, connection: &Connection, buffers: TerminalBuffers) -> SessionKind {
        let pinned = self.pinned_host_key(connection);
        let size = lock(&buffers.emulator).size();
        let ssh = SshSession::start(
            &self.runtime,
            self.ctx.clone(),
            connection.clone(),
            pinned,
            buffers.clone(),
        );
        SessionKind::Terminal { ssh, size, buffers }
    }

    /// Relance une session dans le même onglet, avec un nouveau mot de passe si demandé.
    fn reconnect(&mut self, id: u64, ask_password: bool) {
        let Some(session) = self.sessions.iter().find(|session| session.id == id) else {
            return;
        };
        if ask_password {
            let mut connection = session.connection.clone();
            connection.password = None;
            self.reconnect_after_prompt = Some(id);
            self.password_prompt = Some(PasswordPrompt::new(connection));
        } else {
            self.restart_in_place(id, None);
        }
    }

    /// Reconnecte l'onglet sans le déplacer. Un terminal garde son écran, son
    /// historique et son enregistrement : la reprise y est simplement signalée.
    fn restart_in_place(&mut self, id: u64, connection: Option<Connection>) {
        let Some(index) = self.sessions.iter().position(|session| session.id == id) else {
            return;
        };
        if let Some(connection) = connection {
            self.sessions[index].connection = connection;
        }
        let connection = self.sessions[index].connection.clone();
        tracing::info!(target = %connection.target(), "reconnecting session in place");
        let kind = match &self.sessions[index].kind {
            SessionKind::Terminal { buffers, .. } => {
                lock(&buffers.recorder).note("reconnexion");
                lock(&buffers.emulator).advance(RECONNECTION_MARKER.as_bytes());
                self.start_terminal(&connection, buffers.clone())
            }
            SessionKind::Files { .. } => self.start_files(&connection),
        };
        // L'ancienne session, déjà terminée, est libérée ici.
        self.sessions[index].kind = kind;
        self.active = ActiveTab::Session(id);
    }

    /// Ouvre un nouvel onglet sur la même connexion, sous le même protocole.
    fn duplicate_session(&mut self, id: u64) {
        if let Some(session) = self.sessions.iter().find(|session| session.id == id) {
            let connection = session.connection.clone();
            self.open_connection(connection);
        }
    }

    fn export_session(&mut self, id: u64) {
        let Some(session) = self.sessions.iter().find(|session| session.id == id) else {
            return;
        };
        let SessionKind::Terminal { buffers, .. } = &session.kind else {
            return;
        };
        let suggested = recordings::export_file_name(
            &session.connection.name,
            chrono::Local::now().timestamp(),
        );
        let text = {
            let recorder = lock(&buffers.recorder);
            match recorder.path() {
                Some(path) => recordings::read_text(
                    path,
                    &self.vault.recording_key(),
                    &recorder.pending_line(),
                ),
                None => Ok(recordings::screen_document(
                    &session.connection,
                    &lock(&buffers.emulator).history_text(),
                )),
            }
        };
        self.save_export(&suggested, text);
    }

    /// Demande où enregistrer, puis écrit le texte exporté.
    fn save_export(&mut self, suggested: &str, text: Result<String, RecordingError>) {
        let Some(target) = rfd::FileDialog::new()
            .set_title("Exporter la session")
            .set_file_name(suggested)
            .add_filter("Texte", &["txt"])
            .save_file()
        else {
            return;
        };
        match text.and_then(|text| recordings::write_export(&target, &text)) {
            Ok(()) => {
                tracing::info!(path = %target.display(), "session exported");
                let name = target
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.notify(format!("Session exportée : {name}"), true);
            }
            Err(error) => {
                tracing::error!(path = %target.display(), %error, "failed to export the session");
                self.notify(format!("Export impossible : {error}"), false);
            }
        }
    }

    fn backup_modals(&mut self, ctx: &egui::Context) {
        if self.confirm_plain_export
            && let Some(confirmed) = backup_dialogs::confirm_plain_export(ctx)
        {
            self.confirm_plain_export = false;
            if confirmed {
                self.export_backup(true);
            }
        }
        let Some(prompt) = &mut self.backup_phrase else {
            return;
        };
        match prompt.show(ctx) {
            None => {}
            Some(PhraseOutcome::Cancelled) => self.backup_phrase = None,
            Some(PhraseOutcome::Submitted(phrase)) => match prompt.backup.open_with_phrase(&phrase)
            {
                Ok(imported) => {
                    self.backup_phrase = None;
                    self.merge_backup(imported);
                }
                Err(error) => {
                    tracing::warn!(%error, "backup decryption refused");
                    prompt.error = Some("Phrase incorrecte ou fichier corrompu.".into());
                }
            },
        }
    }

    fn export_backup(&mut self, plain: bool) {
        let (file_name, filter, extension) = if plain {
            ("ssh-manager-backup.json", "JSON non chiffré", "json")
        } else {
            (
                "ssh-manager-backup.enc",
                "Sauvegarde chiffrée SSH Manager",
                "enc",
            )
        };
        let Some(target) = rfd::FileDialog::new()
            .set_title("Exporter les connexions")
            .set_file_name(file_name)
            .add_filter(filter, &[extension])
            .save_file()
        else {
            return;
        };
        let content = if plain {
            backup::export_plain(&self.data)
        } else {
            self.vault.export_backup(&self.data)
        };
        match std::fs::write(&target, content) {
            Ok(()) => {
                tracing::info!(path = %target.display(), plain, connections = self.data.library.connections.len(), "backup exported");
                self.notify(format!("Exporté : {}", target.display()), true);
            }
            Err(error) => {
                tracing::error!(path = %target.display(), %error, "failed to write the backup");
                self.notify(format!("Export impossible : {error}"), false);
            }
        }
    }

    fn import_backup(&mut self) {
        let Some(source) = rfd::FileDialog::new()
            .set_title("Importer des connexions")
            .add_filter("Sauvegardes SSH Manager", &["enc", "json"])
            .pick_file()
        else {
            return;
        };
        let parsed = std::fs::read(&source)
            .map_err(|error| error.to_string())
            .and_then(|bytes| backup::parse(&bytes).map_err(|error| error.to_string()));
        match parsed {
            Ok(Backup::Plain(imported)) => self.merge_backup(imported),
            Ok(Backup::Encrypted(encrypted)) => match self.vault.open_own_backup(&encrypted) {
                Some(Ok(imported)) => self.merge_backup(imported),
                Some(Err(error)) => {
                    tracing::error!(path = %source.display(), %error, "backup of this vault could not be decrypted");
                    self.notify("Sauvegarde corrompue : déchiffrement impossible.", false);
                }
                // Sauvegarde d'un autre coffre : sa phrase est nécessaire.
                None => self.backup_phrase = Some(PhrasePrompt::new(encrypted)),
            },
            Err(error) => {
                tracing::warn!(path = %source.display(), %error, "backup file rejected");
                self.notify(format!("Fichier de sauvegarde invalide : {error}"), false);
            }
        }
    }

    fn merge_backup(&mut self, imported: ssh_manager_vault::VaultData) {
        let added = backup::merge_into(&mut self.data, imported);
        tracing::info!(added, "backup imported");
        self.persist();
        self.notify(format!("{added} connexion(s) importée(s)"), true);
    }

    fn open_recordings(&mut self) {
        let key = self.vault.recording_key();
        let list = recordings::list(&ssh_manager_vault::paths::recordings_dir(), &key);
        self.recordings_dialog = Some(RecordingsDialog::new(list));
    }

    /// Enregistrements des sessions encore ouvertes.
    fn live_recordings(&self) -> Vec<PathBuf> {
        self.sessions
            .iter()
            .filter_map(|session| match &session.kind {
                SessionKind::Terminal { buffers, .. } => {
                    lock(&buffers.recorder).path().map(Path::to_path_buf)
                }
                SessionKind::Files { .. } => None,
            })
            .collect()
    }

    fn recordings_modal(&mut self, ctx: &egui::Context) {
        let live = self.live_recordings();
        let enabled = self.settings.record_sessions;
        let Some(dialog) = &mut self.recordings_dialog else {
            return;
        };
        match dialog.show(ctx, &live, enabled) {
            None => {}
            Some(RecordingsChoice::Close) => self.recordings_dialog = None,
            Some(RecordingsChoice::Export(recording)) => {
                let suggested = recordings::export_file_name(
                    &recording.header.connection_name,
                    recording.header.started_at,
                );
                let text = recordings::read_text(&recording.path, &self.vault.recording_key(), "");
                self.save_export(&suggested, text);
            }
            Some(RecordingsChoice::Delete(path)) => match std::fs::remove_file(&path) {
                Ok(()) => {
                    tracing::info!(path = %path.display(), "session recording deleted");
                    dialog.forget(&path);
                }
                Err(error) => {
                    tracing::error!(path = %path.display(), %error, "failed to delete a session recording");
                    self.notify(format!("Suppression impossible : {error}"), false);
                }
            },
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
            .filter(|session| session.state().is_live())
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
                        let storage = if ssh_manager_vault::paths::is_portable() {
                            "portable · coffre chiffré"
                        } else {
                            "coffre chiffré"
                        };
                        ui.label(RichText::new(storage).font(font).color(color::TEXT_DIM))
                            .on_hover_text(format!(
                                "Données : {}",
                                ssh_manager_vault::paths::data_dir().display()
                            ));
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
