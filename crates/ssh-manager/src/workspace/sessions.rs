//! Onglets de session : terminal SSH ou navigateur de fichiers.

use egui::{Color32, CornerRadius, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, Vec2};
use ssh_manager_core::Connection;

use super::{Action, Workspace};
use crate::files::FileBrowser;
use crate::files::view::{self as files_view, BrowserView};
use crate::session::emulator::GridSize;
use crate::session::ssh::{SessionError, SessionState, SshSession, TerminalBuffers};
use crate::session::view;
use crate::settings::PasteMode;
use crate::theme::{self, color, radius};
use crate::widgets;

/// Taille provisoire avant le premier affichage, qui donne la taille réelle.
pub const INITIAL_SIZE: GridSize = GridSize {
    columns: 120,
    rows: 32,
};

pub enum SessionKind {
    Terminal {
        ssh: SshSession,
        size: GridSize,
        /// Écran et enregistrement, conservés d'une reconnexion à l'autre.
        buffers: TerminalBuffers,
    },
    Files {
        browser: FileBrowser,
        view: BrowserView,
    },
}

pub struct OpenSession {
    pub id: u64,
    pub connection: Connection,
    pub kind: SessionKind,
}

impl OpenSession {
    pub fn state(&self) -> SessionState {
        match &self.kind {
            SessionKind::Terminal { ssh, .. } => ssh.state(),
            SessionKind::Files { browser, .. } => browser.state(),
        }
    }

    pub fn take_learned_host_key(&self) -> Option<String> {
        match &self.kind {
            SessionKind::Terminal { ssh, .. } => ssh.take_learned_host_key(),
            SessionKind::Files { browser, .. } => browser.take_learned_host_key(),
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self.kind, SessionKind::Terminal { .. })
    }

    fn is_files(&self) -> bool {
        matches!(self.kind, SessionKind::Files { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Home,
    Session(u64),
}

fn status_color(state: &SessionState) -> Color32 {
    match state {
        SessionState::Connecting => color::WARNING,
        SessionState::Connected => color::SUCCESS,
        SessionState::Closed { .. } => color::TEXT_DIM,
        SessionState::Failed(_) => color::DANGER,
    }
}

pub fn tab_bar(workspace: &mut Workspace, ui: &mut Ui, actions: &mut Vec<Action>) {
    egui::Panel::top("tab-bar")
        .exact_size(44.0)
        .show_separator_line(false)
        .frame(Frame::new().fill(color::BASE).inner_margin(Margin {
            left: 14,
            right: 14,
            top: 0,
            bottom: 12,
        }))
        .show(ui, |ui| {
            egui::ScrollArea::horizontal().show(ui, |ui| {
                ui.horizontal(|ui| {
                    if tab(
                        ui,
                        workspace.active == ActiveTab::Home,
                        None,
                        "⌂  Accueil",
                        false,
                    )
                    .0
                    .clicked()
                    {
                        workspace.active = ActiveTab::Home;
                    }
                    for session in &workspace.sessions {
                        let state = session.state();
                        let active = workspace.active == ActiveTab::Session(session.id);
                        let label = if session.is_files() {
                            format!(
                                "{}  {}",
                                session.connection.name,
                                session.connection.kind.label()
                            )
                        } else {
                            session.connection.name.clone()
                        };
                        let (response, close) =
                            tab(ui, active, Some(status_color(&state)), &label, true);
                        response.context_menu(|ui| tab_menu(session, &state, ui, actions));
                        if close {
                            actions.push(Action::CloseSession(session.id));
                        } else if response.clicked() {
                            workspace.active = ActiveTab::Session(session.id);
                        }
                    }
                });
            });
        });
}

/// Menu du clic droit sur un onglet de session.
fn tab_menu(session: &OpenSession, state: &SessionState, ui: &mut Ui, actions: &mut Vec<Action>) {
    if ui
        .add_enabled(!state.is_live(), egui::Button::new("Reconnecter"))
        .clicked()
    {
        actions.push(Action::Reconnect(session.id));
        ui.close();
    }
    if ui.button("Dupliquer l'onglet").clicked() {
        actions.push(Action::DuplicateSession(session.id));
        ui.close();
    }
    if session.is_terminal() && ui.button("Exporter la session…").clicked() {
        actions.push(Action::ExportSession(session.id));
        ui.close();
    }
    ui.separator();
    if ui.button("Fermer").clicked() {
        actions.push(Action::CloseSession(session.id));
        ui.close();
    }
}

/// Onglet en pastille ; renvoie la réponse et vrai si sa croix a été cliquée.
fn tab(
    ui: &mut Ui,
    active: bool,
    status: Option<Color32>,
    label: &str,
    closable: bool,
) -> (egui::Response, bool) {
    let font = if active {
        theme::sans_semibold(13.0)
    } else {
        theme::sans(13.0)
    };
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        font,
        if active {
            color::TEXT
        } else {
            color::TEXT_MUTED
        },
    );
    let status_width = if status.is_some() { 14.0 } else { 0.0 };
    let close_width = if closable { 22.0 } else { 0.0 };
    let width = 28.0 + status_width + galley.size().x + close_width;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 32.0), Sense::click());

    let fill = match (active, response.hovered()) {
        (true, _) => color::ELEVATED,
        (false, true) => color::SURFACE,
        (false, false) => Color32::TRANSPARENT,
    };
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(radius::MEDIUM), fill);
    let mut x = rect.left() + 14.0;
    if let Some(status) = status {
        painter.circle_filled(egui::pos2(x + 3.0, rect.center().y), 3.0, status);
        x += status_width;
    }
    painter.galley(
        egui::pos2(x, rect.center().y - galley.size().y / 2.0),
        galley.clone(),
        color::TEXT,
    );

    let mut close = false;
    if closable {
        let close_rect = Rect::from_center_size(
            egui::pos2(rect.right() - 16.0, rect.center().y),
            Vec2::splat(16.0),
        );
        let close_response = ui.interact(close_rect, response.id.with("close"), Sense::click());
        let tint = if close_response.hovered() {
            color::DANGER
        } else {
            color::TEXT_DIM
        };
        if response.hovered() || active {
            painter.text(
                close_rect.center(),
                egui::Align2::CENTER_CENTER,
                "✕",
                theme::sans(10.0),
                tint,
            );
        }
        close = close_response.clicked();
    }
    (response, close)
}

pub fn session_panel(
    session: &mut OpenSession,
    paste_mode: PasteMode,
    ui: &mut Ui,
    actions: &mut Vec<Action>,
) {
    let state = session.state();
    if let SessionState::Failed(error) = &state {
        Frame::new()
            .fill(color::SURFACE)
            .corner_radius(CornerRadius::same(radius::MEDIUM))
            .show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                toolbar(session, &state, ui, actions);
                failure(session, error, ui, actions);
            });
        return;
    }
    match &mut session.kind {
        SessionKind::Files { browser, view } => {
            if matches!(state, SessionState::Connecting) {
                connecting(ui, &session.connection);
            } else if files_view::show(ui, browser, view).is_some() {
                actions.push(Action::CloseSession(session.id));
            }
        }
        SessionKind::Terminal { .. } => terminal_panel(session, &state, paste_mode, ui, actions),
    }
}

fn connecting(ui: &mut Ui, connection: &Connection) {
    Frame::new()
        .fill(color::SURFACE)
        .corner_radius(CornerRadius::same(radius::MEDIUM))
        .show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() / 2.0 - 30.0);
                ui.spinner();
                ui.label(
                    RichText::new(format!("Connexion à {}…", connection.target()))
                        .font(theme::mono(12.0))
                        .color(color::TEXT_MUTED),
                );
            });
        });
}

fn terminal_panel(
    session: &mut OpenSession,
    state: &SessionState,
    paste_mode: PasteMode,
    ui: &mut Ui,
    actions: &mut Vec<Action>,
) {
    Frame::new()
        .fill(crate::session::palette::BACKGROUND)
        .stroke(Stroke::new(1.0, color::BORDER_SUBTLE))
        .corner_radius(CornerRadius::same(radius::MEDIUM))
        .show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            toolbar(session, state, ui, actions);
            ui.add(egui::Separator::default().spacing(0.0));
            if let SessionState::Closed { reason } = state {
                disconnected_banner(session.id, reason.as_deref(), ui, actions);
            }
            if let SessionKind::Terminal { ssh, size, .. } = &mut session.kind {
                let terminal = view::show(ui, ssh, paste_mode, state.is_live());
                *size = terminal.size;
                if terminal.reconnect_requested {
                    actions.push(Action::Reconnect(session.id));
                }
            }
        });
}

/// Bandeau d'une session terminée : la raison et le moyen de reprendre.
fn disconnected_banner(id: u64, reason: Option<&str>, ui: &mut Ui, actions: &mut Vec<Action>) {
    Frame::new()
        .fill(color::tint(color::WARNING, 28))
        .inner_margin(Margin::symmetric(14, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new("●").size(9.0).color(color::WARNING));
                ui.label(
                    RichText::new(format!(
                        "Session terminée — {}",
                        reason.unwrap_or("déconnecté")
                    ))
                    .font(theme::sans_semibold(13.0)),
                );
                ui.label(
                    RichText::new("Appuyez sur Entrée pour vous reconnecter.")
                        .font(theme::sans(12.0))
                        .color(color::TEXT_MUTED),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::primary_button(ui, "Reconnecter").clicked() {
                        actions.push(Action::Reconnect(id));
                    }
                });
            });
        });
}

fn toolbar(session: &OpenSession, state: &SessionState, ui: &mut Ui, actions: &mut Vec<Action>) {
    Frame::new()
        .inner_margin(Margin::symmetric(14, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(session.connection.target()).font(theme::mono(12.0)));
                let description = match state {
                    SessionState::Connecting => "connexion…".to_owned(),
                    SessionState::Connected => match &session.kind {
                        SessionKind::Terminal { size, .. } => {
                            format!("{}×{}", size.columns, size.rows)
                        }
                        SessionKind::Files { .. } => "connecté".to_owned(),
                    },
                    SessionState::Closed { reason } => {
                        reason.clone().unwrap_or_else(|| "déconnecté".into())
                    }
                    SessionState::Failed(_) => "échec".to_owned(),
                };
                ui.label(
                    RichText::new(description)
                        .font(theme::mono(11.0))
                        .color(color::TEXT_DIM),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let close =
                        egui::Button::new(RichText::new("✕").size(11.0).color(color::TEXT_MUTED))
                            .fill(color::SURFACE)
                            .corner_radius(CornerRadius::same(radius::PILL))
                            .min_size(Vec2::splat(28.0));
                    if ui.add(close).on_hover_text("Fermer la session").clicked() {
                        actions.push(Action::CloseSession(session.id));
                    }
                    // Le bandeau des sessions terminal offre déjà la reconnexion.
                    let banner_shown =
                        session.is_terminal() && matches!(state, SessionState::Closed { .. });
                    if !state.is_live()
                        && !banner_shown
                        && widgets::secondary_button(ui, "Reconnecter").clicked()
                    {
                        actions.push(Action::Reconnect(session.id));
                    }
                    if session.is_terminal()
                        && widgets::secondary_button(ui, "Exporter")
                            .on_hover_text(
                                "Enregistrer le texte de la session dans un fichier .txt",
                            )
                            .clicked()
                    {
                        actions.push(Action::ExportSession(session.id));
                    }
                });
            });
        });
}

fn failure(session: &OpenSession, error: &SessionError, ui: &mut Ui, actions: &mut Vec<Action>) {
    ui.vertical_centered(|ui| {
        ui.add_space(60.0);
        Frame::new()
            .fill(color::SURFACE)
            .corner_radius(CornerRadius::same(radius::LARGE))
            .inner_margin(Margin::same(28))
            .show(ui, |ui| {
                ui.set_max_width(520.0);
                ui.label(RichText::new("Connexion échouée").font(theme::sans_bold(18.0)).color(color::DANGER));
                ui.add_space(8.0);
                match error {
                    SessionError::HostKeyChanged { known, received } => {
                        ui.label(RichText::new(
                            "⚠ La clé d'hôte du serveur a changé. Cela peut indiquer une usurpation de serveur (« man-in-the-middle »), ou une réinstallation légitime. La connexion a été refusée.",
                        ));
                        ui.add_space(8.0);
                        ui.label(RichText::new(format!("Empreinte connue : {known}\nEmpreinte reçue : {received}")).font(theme::mono(11.0)).color(color::TEXT_MUTED));
                        ui.add_space(12.0);
                        if widgets::secondary_button(ui, "Je sais que le changement est légitime — faire confiance à la nouvelle clé").clicked() {
                            actions.push(Action::TrustHostKey { session: session.id, fingerprint: received.clone() });
                        }
                    }
                    SessionError::Authentication if session.connection.private_key_path.is_none() => {
                        ui.label("Mot de passe incorrect ou méthode d'authentification refusée.");
                        ui.add_space(12.0);
                        if widgets::primary_button(ui, "Réessayer avec un autre mot de passe").clicked() {
                            actions.push(Action::RetryWithPassword(session.id));
                        }
                    }
                    other => {
                        ui.label(RichText::new(other.to_string()).font(theme::mono(12.0)).color(color::TEXT_MUTED));
                    }
                }
            });
    });
}
