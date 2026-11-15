//! Connexion SSH authentifiée, partagée par le terminal et le navigateur SFTP :
//! vérification de la clé d'hôte, authentification, rebond éventuel par un
//! autre serveur, et état observable par l'interface.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::{self, Handle};
use russh::keys::{HashAlg, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use ssh_manager_core::Connection;

use super::private_key::{self, KeyLoadError};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// Message de maintien : évite qu'un pare-feu ferme une session inactive.
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, thiserror::Error)]
pub enum SessionError {
    /// `host` (`hôte:port`) désigne le serveur concerné : la cible ou le rebond.
    #[error("la clé d'hôte du serveur {host} a changé")]
    HostKeyChanged {
        host: String,
        known: String,
        received: String,
    },
    #[error("authentification refusée par le serveur")]
    Authentication,
    #[error("clé privée illisible : {0}")]
    PrivateKey(String),
    /// La clé privée est chiffrée : phrase absente (`wrong` faux) ou incorrecte.
    #[error("phrase de passe de la clé privée requise")]
    KeyPassphrase { wrong: bool },
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
        /// Vrai si la connexion a été coupée (réseau, délai, veille), et non
        /// fermée volontairement : seul ce cas justifie une reconnexion automatique.
        connection_lost: bool,
    },
    Failed(SessionError),
}

impl SessionState {
    pub fn is_live(&self) -> bool {
        matches!(self, Self::Connecting | Self::Connected)
    }
}

/// Un verrou empoisonné signifie qu'un fil a paniqué en le tenant ; les
/// données restent cohérentes pour l'affichage, on poursuit donc avec elles.
pub fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Empreinte vue pour la première fois sur un serveur, à épingler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LearnedHostKey {
    /// `hôte:port`, comme la clé d'épinglage du coffre.
    pub host: String,
    pub fingerprint: String,
}

/// État d'une session tel que l'interface l'observe, et les empreintes
/// découvertes. Partagé entre l'interface et la tâche réseau.
#[derive(Clone)]
pub struct SharedStatus {
    ctx: egui::Context,
    state: Arc<Mutex<SessionState>>,
    learned_host_keys: Arc<Mutex<Vec<LearnedHostKey>>>,
}

impl SharedStatus {
    pub fn new(ctx: egui::Context) -> Self {
        Self {
            ctx,
            state: Arc::new(Mutex::new(SessionState::Connecting)),
            learned_host_keys: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn state(&self) -> SessionState {
        lock(&self.state).clone()
    }

    pub fn set(&self, state: SessionState) {
        *lock(&self.state) = state;
        self.repaint();
    }

    pub fn repaint(&self) {
        self.ctx.request_repaint();
    }

    /// Empreintes découvertes depuis le dernier appel ; chacune n'est livrée qu'une fois.
    pub fn take_learned_host_keys(&self) -> Vec<LearnedHostKey> {
        std::mem::take(&mut lock(&self.learned_host_keys))
    }

    fn learn_host_key(&self, host: String, fingerprint: String) {
        lock(&self.learned_host_keys).push(LearnedHostKey { host, fingerprint });
    }
}

/// Vérifie la clé d'hôte selon le principe « confiance à la première
/// utilisation » : une clé inconnue est acceptée et mémorisée, une clé
/// différente de celle épinglée est refusée.
pub struct HostKeyGuard {
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

pub type SshHandle = Handle<HostKeyGuard>;

/// Serveur de rebond : la connexion enregistrée et son empreinte épinglée.
#[derive(Debug, Clone)]
pub struct JumpHop {
    pub connection: Connection,
    pub pinned: Option<String>,
}

/// Comment joindre la cible : son empreinte épinglée, et le rebond éventuel.
#[derive(Debug, Clone, Default)]
pub struct Route {
    pub pinned: Option<String>,
    pub jump: Option<JumpHop>,
}

impl Route {
    /// Connexion directe, sans rebond (raccourci des tests).
    #[cfg(test)]
    pub fn direct(pinned: Option<String>) -> Self {
        Self { pinned, jump: None }
    }
}

/// Connexion établie. Le rebond, s'il y en a un, vit aussi longtemps qu'elle :
/// le libérer couperait le tunnel qui la porte.
pub struct SshLink {
    pub handle: Arc<SshHandle>,
    _jump: Option<SshHandle>,
}

fn client_config() -> Arc<client::Config> {
    Arc::new(client::Config {
        keepalive_interval: Some(KEEPALIVE_INTERVAL),
        ..Default::default()
    })
}

/// Ouvre une connexion SSH authentifiée, directement ou par le rebond de
/// `route`. Une empreinte vue pour la première fois est transmise à
/// `status`, pour être épinglée dans le coffre.
pub async fn open(
    connection: &Connection,
    route: &Route,
    status: &SharedStatus,
) -> Result<SshLink, SessionError> {
    let Some(jump) = &route.jump else {
        let mut handle = connect_direct(connection, route.pinned.clone(), status).await?;
        authenticate(&mut handle, connection).await?;
        return Ok(SshLink {
            handle: Arc::new(handle),
            _jump: None,
        });
    };
    let jump_name = jump.connection.name.clone();
    let in_jump = |error: SessionError| match error {
        // Le refus de clé d'hôte garde son serveur : l'utilisateur peut y faire confiance.
        changed @ SessionError::HostKeyChanged { .. } => changed,
        other => SessionError::Connect(format!("rebond « {jump_name} » : {other}")),
    };
    let mut jump_handle = connect_direct(&jump.connection, jump.pinned.clone(), status)
        .await
        .map_err(in_jump)?;
    authenticate(&mut jump_handle, &jump.connection)
        .await
        .map_err(in_jump)?;
    let channel = jump_handle
        .channel_open_direct_tcpip(
            connection.host.clone(),
            u32::from(connection.port),
            "127.0.0.1",
            0,
        )
        .await
        .map_err(|error| {
            SessionError::Connect(format!(
                "le rebond « {} » ne joint pas {} : {error}",
                jump.connection.name,
                connection.host_key_id()
            ))
        })?;
    let observed = Arc::new(Mutex::new(None));
    let guard = HostKeyGuard {
        pinned: route.pinned.clone(),
        observed: observed.clone(),
    };
    let attempt = tokio::time::timeout(
        CONNECT_TIMEOUT,
        client::connect_stream(client_config(), channel.into_stream(), guard),
    )
    .await;
    let mut handle = settle(
        connection,
        route.pinned.clone(),
        status,
        &observed,
        attempt.map_err(|_| ()),
    )?;
    authenticate(&mut handle, connection).await?;
    tracing::info!(target = %connection.target(), jump = %jump.connection.target(), "ssh connection opened through a jump host");
    Ok(SshLink {
        handle: Arc::new(handle),
        _jump: Some(jump_handle),
    })
}

async fn connect_direct(
    connection: &Connection,
    pinned: Option<String>,
    status: &SharedStatus,
) -> Result<SshHandle, SessionError> {
    let observed = Arc::new(Mutex::new(None));
    let guard = HostKeyGuard {
        pinned: pinned.clone(),
        observed: observed.clone(),
    };
    let address = (connection.host.clone(), connection.port);
    let attempt = tokio::time::timeout(
        CONNECT_TIMEOUT,
        client::connect(client_config(), address, guard),
    )
    .await;
    settle(
        connection,
        pinned,
        status,
        &observed,
        attempt.map_err(|_| ()),
    )
}

/// Interprète la fin de la négociation : délai dépassé, clé d'hôte changée,
/// erreur réseau, ou succès (une première empreinte est alors apprise).
fn settle(
    connection: &Connection,
    pinned: Option<String>,
    status: &SharedStatus,
    observed: &Mutex<Option<String>>,
    attempt: Result<Result<SshHandle, russh::Error>, ()>,
) -> Result<SshHandle, SessionError> {
    let received = lock(observed).clone();
    match attempt {
        Err(()) => Err(SessionError::Connect(format!(
            "aucune réponse après {} s",
            CONNECT_TIMEOUT.as_secs()
        ))),
        Ok(Err(error)) => match (pinned, received) {
            (Some(known), Some(received)) if known != received => {
                Err(SessionError::HostKeyChanged {
                    host: connection.host_key_id(),
                    known,
                    received,
                })
            }
            _ => Err(network(error)),
        },
        Ok(Ok(handle)) => {
            if let (None, Some(fingerprint)) = (pinned, received) {
                status.learn_host_key(connection.host_key_id(), fingerprint);
            }
            Ok(handle)
        }
    }
}

async fn authenticate(handle: &mut SshHandle, connection: &Connection) -> Result<(), SessionError> {
    let accepted = match &connection.private_key_path {
        Some(path) => {
            let key = private_key::load(
                std::path::Path::new(path),
                connection.key_passphrase.as_deref(),
            )
            .map_err(|error| match error {
                KeyLoadError::PassphraseRequired => SessionError::KeyPassphrase { wrong: false },
                KeyLoadError::WrongPassphrase => SessionError::KeyPassphrase { wrong: true },
                KeyLoadError::Unreadable(reason) => SessionError::PrivateKey(reason),
            })?;
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

pub fn network(error: russh::Error) -> SessionError {
    SessionError::Connect(error.to_string())
}
