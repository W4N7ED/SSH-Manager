//! Session SSH interactive : connexion, vérification de la clé d'hôte,
//! authentification, PTY, puis relais des octets entre le serveur et l'émulateur.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::{self, Handle};
use russh::keys::{HashAlg, PrivateKeyWithHashAlg, PublicKeyOrCertificate, load_secret_key};
use russh::{ChannelMsg, Disconnect};
use ssh_manager_core::Connection;
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use super::emulator::{Emulator, GridSize};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// Message de maintien : évite qu'un pare-feu ferme une session inactive.
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);
const TERMINAL_TYPE: &str = "xterm-256color";

#[derive(Debug, Clone, thiserror::Error)]
pub enum SessionError {
    #[error("la clé d'hôte du serveur a changé")]
    HostKeyChanged { known: String, received: String },
    #[error("authentification refusée par le serveur")]
    Authentication,
    #[error("clé privée illisible : {0}")]
    PrivateKey(String),
    #[error("connexion impossible : {0}")]
    Connect(String),
}

#[derive(Debug, Clone)]
pub enum SessionState {
    Connecting,
    Connected,
    /// Session terminée normalement ; `reason` décrit la fin si elle est connue.
    Closed {
        reason: Option<String>,
    },
    Failed(SessionError),
}

impl SessionState {
    pub fn is_live(&self) -> bool {
        matches!(self, Self::Connecting | Self::Connected)
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
    state: Arc<Mutex<SessionState>>,
    /// Empreinte vue lors d'une première connexion, à épingler dans le coffre.
    learned_host_key: Arc<Mutex<Option<String>>>,
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
            state: Arc::new(Mutex::new(SessionState::Connecting)),
            learned_host_key: Arc::new(Mutex::new(None)),
            input: input_tx,
        };

        let task = Task {
            ctx,
            emulator: session.emulator.clone(),
            state: session.state.clone(),
            learned_host_key: session.learned_host_key.clone(),
        };
        runtime.spawn(task.run(connection, pinned_host_key, size, input_rx, responses_rx));
        session
    }

    pub fn state(&self) -> SessionState {
        lock(&self.state).clone()
    }

    /// Empreinte découverte à la première connexion ; renvoyée une seule fois.
    pub fn take_learned_host_key(&self) -> Option<String> {
        lock(&self.learned_host_key).take()
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

/// Un verrou empoisonné signifie qu'un fil a paniqué en le tenant ; les
/// données restent cohérentes pour l'affichage, on poursuit donc avec elles.
pub fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Vérifie la clé d'hôte selon le principe « confiance à la première
/// utilisation » : une clé inconnue est acceptée et mémorisée, une clé
/// différente de celle épinglée est refusée.
struct HostKeyGuard {
    pinned: Option<String>,
    observed: Arc<Mutex<Option<String>>>,
}

impl client::Handler for HostKeyGuard {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let key = match server_key {
            PublicKeyOrCertificate::PublicKey { key, .. } => key.clone(),
            PublicKeyOrCertificate::Certificate(certificate) => {
                certificate.public_key().clone().into()
            }
        };
        // Même format que l'app Electron : « SHA256: » + base64 sans remplissage.
        let fingerprint = key.fingerprint(HashAlg::Sha256).to_string();
        *lock(&self.observed) = Some(fingerprint.clone());
        Ok(self
            .pinned
            .as_ref()
            .is_none_or(|pinned| *pinned == fingerprint))
    }
}

struct Task {
    ctx: egui::Context,
    emulator: Arc<Mutex<Emulator>>,
    state: Arc<Mutex<SessionState>>,
    learned_host_key: Arc<Mutex<Option<String>>>,
}

impl Task {
    fn set_state(&self, state: SessionState) {
        *lock(&self.state) = state;
        self.ctx.request_repaint();
    }

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
        let result = self
            .connect_and_relay(&connection, pinned, size, input, responses)
            .await;
        match result {
            Ok(reason) => {
                tracing::info!(%target, reason = reason.as_deref().unwrap_or("-"), "ssh session closed");
                self.set_state(SessionState::Closed { reason });
            }
            Err(error) => {
                tracing::warn!(%target, %error, "ssh session failed");
                self.set_state(SessionState::Failed(error));
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
        let mut handle = self.connect(connection, pinned).await?;
        authenticate(&mut handle, connection).await?;

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
        self.set_state(SessionState::Connected);

        let reason = self.relay(channel, input, responses).await;
        // Le serveur a pu fermer de lui-même : un échec de la déconnexion est sans conséquence.
        let _ = handle.disconnect(Disconnect::ByApplication, "", "fr").await;
        reason
    }

    async fn connect(
        &self,
        connection: &Connection,
        pinned: Option<String>,
    ) -> Result<Handle<HostKeyGuard>, SessionError> {
        let config = Arc::new(client::Config {
            keepalive_interval: Some(KEEPALIVE_INTERVAL),
            ..Default::default()
        });
        let observed = Arc::new(Mutex::new(None));
        let guard = HostKeyGuard {
            pinned: pinned.clone(),
            observed: observed.clone(),
        };
        let address = (connection.host.clone(), connection.port);

        let attempt =
            tokio::time::timeout(CONNECT_TIMEOUT, client::connect(config, address, guard)).await;
        let received = lock(&observed).clone();
        match attempt {
            Err(_) => Err(SessionError::Connect(format!(
                "aucune réponse après {} s",
                CONNECT_TIMEOUT.as_secs()
            ))),
            Ok(Err(error)) => match (pinned, received) {
                (Some(known), Some(received)) if known != received => {
                    Err(SessionError::HostKeyChanged { known, received })
                }
                _ => Err(network(error)),
            },
            Ok(Ok(handle)) => {
                if pinned.is_none() {
                    *lock(&self.learned_host_key) = received;
                }
                Ok(handle)
            }
        }
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
                        self.ctx.request_repaint();
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

async fn authenticate(
    handle: &mut Handle<HostKeyGuard>,
    connection: &Connection,
) -> Result<(), SessionError> {
    let accepted = match &connection.private_key_path {
        Some(path) => {
            let key = load_secret_key(path, None)
                .map_err(|error| SessionError::PrivateKey(error.to_string()))?;
            let hash = handle
                .best_supported_rsa_hash()
                .await
                .map_err(network)?
                .flatten();
            handle
                .authenticate_publickey(
                    connection.username.clone(),
                    PrivateKeyWithHashAlg::new(Arc::new(key), hash),
                )
                .await
                .map_err(network)?
                .success()
        }
        None => handle
            .authenticate_password(
                connection.username.clone(),
                connection.password.clone().unwrap_or_default(),
            )
            .await
            .map_err(network)?
            .success(),
    };
    if accepted {
        Ok(())
    } else {
        Err(SessionError::Authentication)
    }
}

fn network(error: russh::Error) -> SessionError {
    SessionError::Connect(error.to_string())
}
