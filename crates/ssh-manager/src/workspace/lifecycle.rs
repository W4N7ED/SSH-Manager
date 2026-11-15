//! Cycle de vie des sessions : ouverture (avec demande de secret si besoin),
//! reconnexion dans le même onglet, fermeture, et clés d'hôte.

use super::*;

impl Workspace {
    /// Épingle dans le coffre les clés d'hôte découvertes à la première connexion.
    pub(super) fn pin_learned_host_keys(&mut self) {
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

    /// Ouvre une connexion enregistrée avec son propre protocole : terminal pour
    /// SSH, navigateur de fichiers pour SFTP et FTP.
    pub(super) fn open(&mut self, id: &str) {
        if let Some(connection) = self.data.library.find(id).cloned() {
            self.open_connection(connection);
        }
    }

    /// Ouvre une connexion SSH en navigateur de fichiers, sur le même serveur.
    pub(super) fn open_files(&mut self, id: &str) {
        if let Some(mut connection) = self.data.library.find(id).cloned() {
            connection.kind = ssh_manager_core::ConnectionKind::Sftp;
            self.open_connection(connection);
        }
    }

    pub(super) fn open_external(
        &mut self,
        id: &str,
        shell: crate::external_terminal::ExternalShell,
    ) {
        let Some(connection) = self.data.library.find(id).cloned() else {
            return;
        };
        if let Err(error) = crate::external_terminal::open(&connection, shell) {
            tracing::error!(%error, target = %connection.target(), "failed to open an external terminal");
            self.notify(format!("Ouverture impossible : {error}"), false);
        }
    }

    pub(super) fn forget_host_key(&mut self, id: &str) {
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
    pub(super) fn open_connection(&mut self, connection: Connection) {
        if connection.needs_password_prompt() {
            self.password_prompt = Some(PasswordPrompt::new(connection));
        } else if key_passphrase_missing(&connection) {
            // Demandée avant de se connecter : sinon le serveur verrait d'abord
            // une tentative vouée à l'échec.
            let kind = SecretKind::KeyPassphrase { wrong: false };
            self.password_prompt = Some(PasswordPrompt::asking(connection, kind));
        } else {
            self.start_session(connection);
        }
    }

    pub(super) fn start_session(&mut self, connection: Connection) {
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

    pub(super) fn close_session(&mut self, id: u64) {
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

    pub(super) fn pinned_host_key(&self, connection: &Connection) -> Option<String> {
        self.data.host_keys.get(&connection.host_key_id()).cloned()
    }

    pub(super) fn start_files(&self, connection: &Connection) -> SessionKind {
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

    pub(super) fn start_terminal(
        &self,
        connection: &Connection,
        buffers: TerminalBuffers,
    ) -> SessionKind {
        let pinned = self.pinned_host_key(connection);
        let size = lock(&buffers.emulator).size();
        let ssh = SshSession::start(
            &self.runtime,
            self.ctx.clone(),
            connection.clone(),
            pinned,
            buffers.clone(),
        );
        SessionKind::Terminal {
            ssh,
            size,
            buffers,
            search: None,
        }
    }

    /// Redemande un secret, puis relance la session dans le même onglet.
    pub(super) fn reconnect_asking(&mut self, id: u64, kind: SecretKind) {
        let Some(session) = self.sessions.iter().find(|session| session.id == id) else {
            return;
        };
        let mut connection = session.connection.clone();
        match kind {
            SecretKind::Password => connection.password = None,
            SecretKind::KeyPassphrase { .. } => connection.key_passphrase = None,
        }
        self.reconnect_after_prompt = Some(id);
        self.password_prompt = Some(PasswordPrompt::asking(connection, kind));
    }

    /// Reconnecte l'onglet sans le déplacer. Un terminal garde son écran, son
    /// historique et son enregistrement : la reprise y est simplement signalée.
    pub(super) fn restart_in_place(&mut self, id: u64, connection: Option<Connection>) {
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
    pub(super) fn duplicate_session(&mut self, id: u64) {
        if let Some(session) = self.sessions.iter().find(|session| session.id == id) {
            let connection = session.connection.clone();
            self.open_connection(connection);
        }
    }

    pub(super) fn trust_host_key(&mut self, id: u64, fingerprint: String) {
        let Some(session) = self.sessions.iter().find(|session| session.id == id) else {
            return;
        };
        let host = session.connection.host_key_id();
        tracing::warn!(%host, %fingerprint, "host key replaced at the user's request");
        self.data.host_keys.insert(host, fingerprint);
        self.persist();
        self.restart_in_place(id, None);
    }
}

/// Vrai si la clé privée de la connexion est chiffrée et qu'aucune phrase
/// de passe n'est mémorisée.
pub(super) fn key_passphrase_missing(connection: &Connection) -> bool {
    connection.key_passphrase.is_none()
        && connection
            .private_key_path
            .as_deref()
            .is_some_and(|path| crate::session::private_key::requires_passphrase(Path::new(path)))
}
