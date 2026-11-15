//! Onglets de session : terminal SSH ou navigateur de fichiers.

use std::collections::BTreeSet;

use egui::{Color32, CornerRadius, Frame, Margin, Rect, RichText, Sense, Stroke, Ui, Vec2};
use ssh_manager_core::Connection;

use super::auto_reconnect::{self, AutoReconnect, AutoReconnectStatus};
use super::mosaic::{self, Mosaic};
use super::search_bar::{self, SearchBar};
use super::{Action, Workspace};
use crate::files::FileBrowser;
use crate::files::view::{self as files_view, BrowserView};
use crate::session::emulator::GridSize;
use crate::session::ssh::{SessionError, SessionState, SshSession, TerminalBuffers};
use crate::session::view::{self, TerminalOptions};
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
        /// Barre de recherche, quand elle est ouverte.
        search: Option<SearchBar>,
    },
    Files {
        browser: FileBrowser,
        view: BrowserView,
    },
}

/// Saisie faite dans une session, à rejouer dans les sessions synchronisées.
pub struct TypedInput {
    pub source: u64,
    pub bytes: Vec<u8>,
}

pub struct OpenSession {
    pub id: u64,
    pub connection: Connection,
    pub kind: SessionKind,
    pub auto_reconnect: AutoReconnect,
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
    Toolbox,
    Mosaic,
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
                    if tab(
                        ui,
                        workspace.active == ActiveTab::Toolbox,
                        None,
                        "⚒  Boîte à outils",
                        false,
                    )
                    .0
                    .clicked()
                    {
                        workspace.active = ActiveTab::Toolbox;
                    }
                    if !workspace.mosaic.is_empty() {
                        let label = format!("▦  Mosaïque · {}", workspace.mosaic.count());
                        if tab(
                            ui,
                            workspace.active == ActiveTab::Mosaic,
                            None,
                            &label,
                            false,
                        )
                        .0
                        .on_hover_text("Sessions côte à côte (Ctrl+M)")
                        .clicked()
                        {
                            workspace.active = ActiveTab::Mosaic;
                        }
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
                        let synced = workspace.synced.contains(&session.id);
                        let label = match (workspace.mosaic.contains(session.id), synced) {
                            (true, true) => format!("{label}  ▦ ⟳"),
                            (true, false) => format!("{label}  ▦"),
                            (false, true) => format!("{label}  ⟳"),
                            (false, false) => label,
                        };
                        let (response, close) =
                            tab(ui, active, Some(status_color(&state)), &label, true);
                        response.context_menu(|ui| {
                            tab_menu(
                                session,
                                &state,
                                &workspace.mosaic,
                                &workspace.synced,
                                ui,
                                actions,
                            )
                        });
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
fn tab_menu(
    session: &OpenSession,
    state: &SessionState,
    mosaic: &Mosaic,
    synced: &BTreeSet<u64>,
    ui: &mut Ui,
    actions: &mut Vec<Action>,
) {
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
    if session.is_terminal() {
        let label = if synced.contains(&session.id) {
            "⟳ Désynchroniser la saisie"
        } else {
            "⟳ Synchroniser la saisie"
        };
        if ui
            .button(label)
            .on_hover_text(
                "Ce qui est tapé dans une session synchronisée est envoyé à toutes les autres",
            )
            .clicked()
        {
            actions.push(Action::ToggleSync(session.id));
            ui.close();
        }
    }
    if mosaic.contains(session.id) {
        if ui.button("Retirer de la mosaïque").clicked() {
            actions.push(Action::RemoveFromMosaic(session.id));
            ui.close();
        }
    } else {
        ui.menu_button("Ajouter à la mosaïque", |ui| {
            for (slot, label) in mosaic::SLOT_LABELS.iter().enumerate() {
                let occupied = mosaic.slot(slot).is_some();
                let text = if occupied {
                    format!("{label} (remplacer)")
                } else {
                    (*label).to_owned()
                };
                if ui.button(text).clicked() {
                    actions.push(Action::PlaceInMosaic {
                        session: session.id,
                        slot,
                    });
                    ui.close();
                }
            }
        });
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

/// Affiche une session ; renvoie ce que l'utilisateur y a tapé.
pub fn session_panel(
    session: &mut OpenSession,
    prefs: &TerminalOptions,
    ui: &mut Ui,
    actions: &mut Vec<Action>,
) -> Vec<TypedInput> {
    let state = session.state();
    // Pendant la reconnexion automatique, un serveur encore injoignable ne
    // remplace pas l'écran : le bandeau indique la prochaine tentative.
    let retrying = session.is_terminal()
        && auto_reconnect::should_retry(&state, session.auto_reconnect.attempts);
    if let SessionState::Failed(error) = &state
        && !retrying
    {
        Frame::new()
            .fill(color::SURFACE)
            .corner_radius(CornerRadius::same(radius::MEDIUM))
            .show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                toolbar(session, &state, ui, actions);
                failure(session, error, ui, actions);
            });
        return Vec::new();
    }
    match &mut session.kind {
        SessionKind::Files { browser, view } => {
            if matches!(state, SessionState::Connecting) {
                connecting(ui, &session.connection);
            } else if files_view::show(ui, browser, view).is_some() {
                actions.push(Action::CloseSession(session.id));
            }
            Vec::new()
        }
        SessionKind::Terminal { .. } => terminal_panel(session, &state, prefs, ui, actions),
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
    prefs: &TerminalOptions,
    ui: &mut Ui,
    actions: &mut Vec<Action>,
) -> Vec<TypedInput> {
    let mut typed = Vec::new();
    Frame::new()
        .fill(prefs.colors.background)
        .stroke(Stroke::new(1.0, color::BORDER_SUBTLE))
        .corner_radius(CornerRadius::same(radius::MEDIUM))
        .show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            toolbar(session, state, ui, actions);
            ui.add(egui::Separator::default().spacing(0.0));
            let ended = match state {
                SessionState::Closed { reason, .. } => {
                    Some(reason.clone().unwrap_or_else(|| "déconnecté".into()))
                }
                SessionState::Failed(error) => Some(format!("reconnexion impossible : {error}")),
                _ => None,
            };
            if let Some(reason) = ended {
                let auto = session
                    .auto_reconnect
                    .status(state, std::time::Instant::now());
                disconnected_banner(session.id, &reason, auto, ui, actions);
            }
            if let SessionKind::Terminal {
                ssh, size, search, ..
            } = &mut session.kind
            {
                let ctrl_shift = egui::Modifiers::CTRL | egui::Modifiers::SHIFT;
                if ui.input_mut(|input| input.consume_key(ctrl_shift, egui::Key::F)) {
                    search.get_or_insert_default();
                }
                if let Some(bar) = search
                    && search_bar::show(ui, bar, &ssh.emulator)
                {
                    *search = None;
                }
                let terminal = view::show(ui, ssh, prefs, state.is_live());
                *size = terminal.size;
                if let Some(font_size) = terminal.font_size_request {
                    actions.push(Action::SetTerminalFontSize(font_size));
                }
                if terminal.reconnect_requested {
                    actions.push(Action::Reconnect(session.id));
                }
                typed.extend(terminal.typed.into_iter().map(|bytes| TypedInput {
                    source: session.id,
                    bytes,
                }));
            }
        });
    typed
}

/// La mosaïque : quatre quarts, chacun une session ou un emplacement libre.
pub fn mosaic_panel(
    sessions: &mut [OpenSession],
    layout: &Mosaic,
    synced: &BTreeSet<u64>,
    prefs: &TerminalOptions,
    ui: &mut Ui,
    actions: &mut Vec<Action>,
) -> Vec<TypedInput> {
    let synced_tiles = (0..mosaic::SLOT_COUNT)
        .filter_map(|slot| layout.slot(slot))
        .filter(|id| synced.contains(id))
        .count();
    ui.horizontal(|ui| {
        ui.label(RichText::new("Mosaïque").font(theme::sans_bold(15.0)));
        let badge = if synced_tiles > 1 {
            format!("SYNC · {synced_tiles} panneaux")
        } else {
            format!("{} panneau(x)", layout.count())
        };
        let badge_color = if synced_tiles > 1 {
            color::WARNING
        } else {
            color::TEXT_DIM
        };
        ui.label(
            RichText::new(badge)
                .font(theme::mono(11.0))
                .color(badge_color),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::secondary_button(ui, "Quitter la mosaïque").clicked() {
                actions.push(Action::ToggleMosaic);
            }
            ui.label(
                RichText::new("Clic droit sur un onglet pour l'ajouter ou le synchroniser")
                    .font(theme::sans(12.0))
                    .color(color::TEXT_DIM),
            );
        });
    });
    ui.add_space(8.0);

    const GAP: f32 = 8.0;
    let area = ui.available_rect_before_wrap();
    let tile_size = Vec2::new((area.width() - GAP) / 2.0, (area.height() - GAP) / 2.0);
    let mut typed = Vec::new();
    for slot in 0..mosaic::SLOT_COUNT {
        let origin = area.min
            + Vec2::new(
                (slot % 2) as f32 * (tile_size.x + GAP),
                (slot / 2) as f32 * (tile_size.y + GAP),
            );
        let rect = Rect::from_min_size(origin, tile_size);
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(rect)
                .id_salt(("mosaic-tile", slot)),
            |ui| {
                let occupant = layout
                    .slot(slot)
                    .and_then(|id| sessions.iter_mut().find(|session| session.id == id));
                match occupant {
                    Some(session) => {
                        tile_header(session, slot, synced.contains(&session.id), ui, actions);
                        typed.extend(session_panel(session, prefs, ui, actions));
                    }
                    None => empty_tile(slot, sessions, layout, ui, actions),
                }
            },
        );
    }
    ui.allocate_rect(area, Sense::hover());
    typed
}

fn tile_header(
    session: &OpenSession,
    slot: usize,
    synced: bool,
    ui: &mut Ui,
    actions: &mut Vec<Action>,
) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("●")
                .size(9.0)
                .color(status_color(&session.state())),
        );
        ui.label(RichText::new(&session.connection.name).font(theme::sans_semibold(12.0)));
        if synced {
            ui.label(
                RichText::new("⟳ sync")
                    .font(theme::mono(10.0))
                    .color(color::WARNING),
            );
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let undock =
                egui::Button::new(RichText::new("⊡").color(color::TEXT_MUTED)).frame(false);
            if ui
                .add(undock)
                .on_hover_text("Retirer de la mosaïque")
                .clicked()
            {
                actions.push(Action::RemoveFromMosaic(session.id));
            }
            ui.label(
                RichText::new((slot + 1).to_string())
                    .font(theme::mono(10.0))
                    .color(color::TEXT_DIM),
            );
        });
    });
}

fn empty_tile(
    slot: usize,
    sessions: &[OpenSession],
    layout: &Mosaic,
    ui: &mut Ui,
    actions: &mut Vec<Action>,
) {
    Frame::new()
        .stroke(Stroke::new(1.0, color::BORDER_SUBTLE))
        .corner_radius(CornerRadius::same(radius::MEDIUM))
        .show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() / 2.0 - 40.0);
                ui.label(RichText::new("+").size(22.0).color(color::TEXT_DIM));
                ui.label(RichText::new(mosaic::SLOT_LABELS[slot]).color(color::TEXT_MUTED));
                let candidates: Vec<&OpenSession> = sessions
                    .iter()
                    .filter(|session| !layout.contains(session.id))
                    .collect();
                if candidates.is_empty() {
                    ui.label(
                        RichText::new("Ouvrez une autre session pour la placer ici")
                            .font(theme::sans(12.0))
                            .color(color::TEXT_DIM),
                    );
                    return;
                }
                egui::ComboBox::from_id_salt(("mosaic-pick", slot))
                    .selected_text("Choisir une session…")
                    .show_ui(ui, |ui| {
                        for session in candidates {
                            if ui
                                .selectable_label(false, &session.connection.name)
                                .clicked()
                            {
                                actions.push(Action::PlaceInMosaic {
                                    session: session.id,
                                    slot,
                                });
                            }
                        }
                    });
            });
        });
}

/// Ce que dit le bandeau de la reconnexion automatique.
fn auto_reconnect_text(auto: Option<AutoReconnectStatus>) -> String {
    match auto {
        Some(AutoReconnectStatus::Scheduled {
            seconds_left,
            attempt,
        }) => format!(
            "Reconnexion automatique dans {seconds_left} s (tentative {attempt}/{}).",
            auto_reconnect::MAX_ATTEMPTS
        ),
        Some(AutoReconnectStatus::GaveUp) => {
            "Reconnexion automatique abandonnée. Appuyez sur Entrée pour réessayer.".into()
        }
        None => "Appuyez sur Entrée pour vous reconnecter.".into(),
    }
}

/// Bandeau d'une session terminée : la raison et le moyen de reprendre.
fn disconnected_banner(
    id: u64,
    reason: &str,
    auto: Option<AutoReconnectStatus>,
    ui: &mut Ui,
    actions: &mut Vec<Action>,
) {
    Frame::new()
        .fill(color::tint(color::WARNING, 28))
        .inner_margin(Margin::symmetric(14, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new("●").size(9.0).color(color::WARNING));
                ui.label(
                    RichText::new(format!("Session terminée — {reason}"))
                        .font(theme::sans_semibold(13.0)),
                );
                ui.label(
                    RichText::new(auto_reconnect_text(auto))
                        .font(theme::sans(12.0))
                        .color(color::TEXT_MUTED),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::primary_button(ui, "Reconnecter").clicked() {
                        actions.push(Action::Reconnect(id));
                    }
                    if matches!(auto, Some(AutoReconnectStatus::Scheduled { .. }))
                        && widgets::secondary_button(ui, "Annuler").clicked()
                    {
                        actions.push(Action::CancelAutoReconnect(id));
                    }
                });
            });
        });
}

fn toolbar(session: &OpenSession, state: &SessionState, ui: &mut Ui, actions: &mut Vec<Action>) {
    Frame::new()
        .fill(color::SURFACE)
        .corner_radius(CornerRadius {
            nw: radius::MEDIUM,
            ne: radius::MEDIUM,
            sw: 0,
            se: 0,
        })
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
                    SessionState::Closed { reason, .. } => {
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
                    SessionError::KeyPassphrase { wrong } => {
                        let explanation = if *wrong {
                            "La phrase de passe de la clé privée est incorrecte."
                        } else {
                            "La clé privée est protégée par une phrase de passe."
                        };
                        ui.label(explanation);
                        ui.add_space(12.0);
                        if widgets::primary_button(ui, "Saisir la phrase de passe").clicked() {
                            actions.push(Action::RetryWithKeyPassphrase {
                                session: session.id,
                                wrong: *wrong,
                            });
                        }
                    }
                    other => {
                        ui.label(RichText::new(other.to_string()).font(theme::mono(12.0)).color(color::TEXT_MUTED));
                    }
                }
            });
    });
}
