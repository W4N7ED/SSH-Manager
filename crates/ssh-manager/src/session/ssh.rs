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

const TERMINAL_TYPE: &str = "xterm-256color";

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
    /// Lance la connexion sur `runtime`. `pinned_host_key` est l'empreinte déjà
    /// épinglée pour ce serveur, s'il y en a une.
    pub fn start(
        runtime: &Runtime,
        ctx: egui::Context,
        connection: Connection,
        pinned_host_key: Option<String>,
        size: GridSize,
    ) -> Self {
        let (responses_tx, responses_rx) = unbounded_channel();
        let (input_tx, input_rx) = unbounded_channel();
        let session = Self {
            emulator: Arc::new(Mutex::new(Emulator::new(size, Some(responses_tx)))),
            status: SharedStatus::new(ctx),
            input: input_tx,
        };
        let task = Task {
            status: session.status.clone(),
            emulator: session.emulator.clone(),
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
    emulator: Arc<Mutex<Emulator>>,
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
            Ok(reason) => {
                tracing::info!(%target, reason = reason.as_deref().unwrap_or("-"), "ssh session closed");
                self.status.set(SessionState::Closed { reason });
            }
            Err(error) => {
                tracing::warn!(%target, %error, "ssh session failed");
                self.status.set(SessionState::Failed(error));
            }
        }
    }

    async fn connect_and_relay(
        &self,
        connection: &Connection,
        pinned: Option<String>,
        size: GridSize,
        input: UnboundedReceiver<Input>,
        responses: UnboundedReceiver<Vec<u8>>,
    ) -> Result<Option<String>, SessionError> {
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

        let reason = self.relay(channel, input, responses).await;
        // Le serveur a pu fermer de lui-même : un échec de la déconnexion est sans conséquence.
        if let Err(error) = handle.disconnect(Disconnect::ByApplication, "", "fr").await {
            tracing::debug!(%error, "SSH disconnect after terminal session failed");
        }
        reason
    }

    /// Relaie les octets jusqu'à la fin de la session. Renvoie la raison de fin.
    async fn relay(
        &self,
        channel: russh::Channel<client::Msg>,
        mut input: UnboundedReceiver<Input>,
        mut responses: UnboundedReceiver<Vec<u8>>,
    ) -> Result<Option<String>, SessionError> {
        let (mut reader, writer) = channel.split();
        loop {
            tokio::select! {
                message = reader.wait() => match message {
                    Some(ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. }) => {
                        lock(&self.emulator).advance(&data);
                        self.status.repaint();
                    }
                    Some(ChannelMsg::ExitStatus { exit_status }) => {
                        return Ok(Some(format!("le shell s'est terminé (code {exit_status})")));
                    }
                    Some(ChannelMsg::Eof | ChannelMsg::Close) | None => return Ok(None),
                    Some(_) => {}
                },
                command = input.recv() => match command {
                    Some(Input::Data(bytes)) => writer.data(&bytes[..]).await.map_err(network)?,
                    Some(Input::Resize(size)) => writer
                        .window_change(u32::from(size.columns), u32::from(size.rows), 0, 0)
                        .await
                        .map_err(network)?,
                    Some(Input::Close) | None => return Ok(Some("session fermée".into())),
                },
                Some(response) = responses.recv() => writer.data(&response[..]).await.map_err(network)?,
            }
        }
    }
}
