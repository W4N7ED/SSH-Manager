//! Session SSH interactive : PTY, puis relais des octets entre le serveur et
//! l'émulateur. La connexion elle-même est établie par [`super::transport`].

use std::sync::{Arc, Mutex};

use russh::{ChannelMsg, Disconnect, client};
use ssh_manager_core::Connection;
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use super::emulator::{Emulator, GridSize};
use super::transport::{self, SharedStatus, network};
pub use super::transport::{SessionError, SessionState, lock};
use crate::recordings::Recorder;

const TERMINAL_TYPE: &str = "xterm-256color";

/// Fin d'une session ouverte.
struct SessionEnd {
    reason: String,
    connection_lost: bool,
}

impl SessionEnd {
    fn closed(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            connection_lost: false,
        }
    }

    fn lost(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            connection_lost: true,
        }
    }
}

/// Ce qui survit aux reconnexions d'un onglet : l'écran avec son historique,
/// et l'enregistrement de la session.
#[derive(Clone)]
pub struct TerminalBuffers {
    pub emulator: Arc<Mutex<Emulator>>,
    pub recorder: Arc<Mutex<Recorder>>,
}

impl TerminalBuffers {
    pub fn new(size: GridSize, recorder: Recorder) -> Self {
        Self {
            emulator: Arc::new(Mutex::new(Emulator::new(size, None))),
            recorder: Arc::new(Mutex::new(recorder)),
        }
    }
}

enum Input {
    Data(Vec<u8>),
    Resize(GridSize),
    Close,
}

/// Côté interface d'une session : l'émulateur à afficher et l'envoi des saisies.
pub struct SshSession {
    pub emulator: Arc<Mutex<Emulator>>,
    status: SharedStatus,
    input: UnboundedSender<Input>,
}

impl SshSession {
    /// Lance la connexion sur `runtime`, en affichant dans `buffers`.
    /// `pinned_host_key` est l'empreinte déjà épinglée pour ce serveur, s'il y en a une.
    pub fn start(
        runtime: &Runtime,
        ctx: egui::Context,
        connection: Connection,
        pinned_host_key: Option<String>,
        buffers: TerminalBuffers,
    ) -> Self {
        let (responses_tx, responses_rx) = unbounded_channel();
        let (input_tx, input_rx) = unbounded_channel();
        let size = {
            let emulator = lock(&buffers.emulator);
            emulator.connect_responses(responses_tx);
            emulator.size()
        };
        let session = Self {
            emulator: buffers.emulator.clone(),
            status: SharedStatus::new(ctx),
            input: input_tx,
        };
        let task = Task {
            status: session.status.clone(),
            buffers,
        };
        runtime.spawn(task.run(connection, pinned_host_key, size, input_rx, responses_rx));
        session
    }

    pub fn state(&self) -> SessionState {
        self.status.state()
    }

    /// Empreinte découverte à la première connexion ; renvoyée une seule fois.
    pub fn take_learned_host_key(&self) -> Option<String> {
        self.status.take_learned_host_key()
    }

    pub fn send(&self, bytes: Vec<u8>) {
        // L'envoi n'échoue que si la session est terminée : la saisie est sans objet.
        let _ = self.input.send(Input::Data(bytes));
    }

    pub fn resize(&self, size: GridSize) {
        let _ = self.input.send(Input::Resize(size));
    }

    pub fn close(&self) {
        let _ = self.input.send(Input::Close);
    }
}

impl Drop for SshSession {
    fn drop(&mut self) {
        self.close();
    }
}

struct Task {
    status: SharedStatus,
    buffers: TerminalBuffers,
}

impl Task {
    async fn run(
        self,
        connection: Connection,
        pinned: Option<String>,
        size: GridSize,
        input: UnboundedReceiver<Input>,
        responses: UnboundedReceiver<Vec<u8>>,
    ) {
        let target = connection.target();
        tracing::info!(%target, "ssh session connecting");
        match self
            .connect_and_relay(&connection, pinned, size, input, responses)
            .await
        {
            Ok(end) => {
                let reason = end.reason;
                tracing::info!(%target, %reason, connection_lost = end.connection_lost, "ssh session closed");
                lock(&self.buffers.recorder).note(&format!("session terminée : {reason}"));
                self.status.set(SessionState::Closed {
                    reason: Some(reason),
                    connection_lost: end.connection_lost,
                });
            }
            Err(error) => {
                tracing::warn!(%target, %error, "ssh session failed");
                self.status.set(SessionState::Failed(error));
            }
        }
    }

    /// Erreur tant que la session n'est pas ouverte ; une fois ouverte, toute
    /// fin (y compris une coupure réseau) est une fermeture avec sa raison :
    /// l'écran reste affiché et l'onglet peut être reconnecté.
    async fn connect_and_relay(
        &self,
        connection: &Connection,
        pinned: Option<String>,
        size: GridSize,
        input: UnboundedReceiver<Input>,
        responses: UnboundedReceiver<Vec<u8>>,
    ) -> Result<SessionEnd, SessionError> {
        let handle = transport::open(connection, pinned, &self.status).await?;
        let channel = handle.channel_open_session().await.map_err(network)?;
        channel
            .request_pty(
                false,
                TERMINAL_TYPE,
                u32::from(size.columns),
                u32::from(size.rows),
                0,
                0,
                &[],
            )
            .await
            .map_err(network)?;
        channel.request_shell(false).await.map_err(network)?;
        self.status.set(SessionState::Connected);

        let end = self.relay(channel, input, responses).await;
        // Le serveur a pu fermer de lui-même : un échec de la déconnexion est sans conséquence.
        if let Err(error) = handle.disconnect(Disconnect::ByApplication, "", "fr").await {
            tracing::debug!(%error, "SSH disconnect after terminal session failed");
        }
        Ok(end)
    }

    /// Relaie les octets jusqu'à la fin de la session. Renvoie la raison de fin.
    async fn relay(
        &self,
        channel: russh::Channel<client::Msg>,
        mut input: UnboundedReceiver<Input>,
        mut responses: UnboundedReceiver<Vec<u8>>,
    ) -> SessionEnd {
        let (mut reader, writer) = channel.split();
        loop {
            let written = tokio::select! {
                message = reader.wait() => match message {
                    Some(ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. }) => {
                        self.display(&data);
                        Ok(())
                    }
                    Some(ChannelMsg::ExitStatus { exit_status }) => {
                        return SessionEnd::closed(format!("le shell s'est terminé (code {exit_status})"));
                    }
                    Some(ChannelMsg::Eof | ChannelMsg::Close) => {
                        // Sans code de sortie : le serveur a coupé (délai d'inactivité côté
                        // serveur, redémarrage…), ce n'est pas l'utilisateur qui est parti.
                        return SessionEnd::lost("le serveur a fermé la session");
                    }
                    // Plus aucun message : la connexion a été coupée (délai d'inactivité,
                    // keepalive sans réponse, mise en veille du poste…).
                    None => return SessionEnd::lost("connexion perdue"),
                    Some(_) => Ok(()),
                },
                command = input.recv() => match command {
                    Some(Input::Data(bytes)) => writer.data(&bytes[..]).await,
                    Some(Input::Resize(size)) => {
                        writer
                            .window_change(u32::from(size.columns), u32::from(size.rows), 0, 0)
                            .await
                    }
                    Some(Input::Close) | None => return SessionEnd::closed("session fermée"),
                },
                Some(response) = responses.recv() => writer.data(&response[..]).await,
            };
            if let Err(error) = written {
                return SessionEnd::lost(format!("connexion perdue ({error})"));
            }
        }
    }

    fn display(&self, data: &[u8]) {
        lock(&self.buffers.emulator).advance(data);
        lock(&self.buffers.recorder).feed(data);
        self.status.repaint();
    }
}
