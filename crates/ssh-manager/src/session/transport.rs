//! Connexion SSH authentifiée, partagée par le terminal et le navigateur SFTP :
//! vérification de la clé d'hôte, authentification, et état observable par
//! l'interface.

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
    #[error("la clé d'hôte du serveur a changé")]
    HostKeyChanged { known: String, received: String },
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

/// État d'une session tel que l'interface l'observe, et l'empreinte découverte
/// à la première connexion. Partagé entre l'interface et la tâche réseau.
#[derive(Clone)]
pub struct SharedStatus {
    ctx: egui::Context,
    state: Arc<Mutex<SessionState>>,
    learned_host_key: Arc<Mutex<Option<String>>>,
}

impl SharedStatus {
    pub fn new(ctx: egui::Context) -> Self {
        Self {
            ctx,
            state: Arc::new(Mutex::new(SessionState::Connecting)),
            learned_host_key: Arc::new(Mutex::new(None)),
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

    /// Empreinte découverte à la première connexion ; renvoyée une seule fois.
    pub fn take_learned_host_key(&self) -> Option<String> {
        lock(&self.learned_host_key).take()
    }

    fn learn_host_key(&self, fingerprint: String) {
        *lock(&self.learned_host_key) = Some(fingerprint);
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

/// Ouvre une connexion SSH authentifiée. Une empreinte vue pour la première
/// fois est transmise à `status`, pour être épinglée dans le coffre.
pub async fn open(
    connection: &Connection,
    pinned: Option<String>,
    status: &SharedStatus,
) -> Result<SshHandle, SessionError> {
    let mut handle = connect(connection, pinned, status).await?;
    authenticate(&mut handle, connection).await?;
    Ok(handle)
}

async fn connect(
    connection: &Connection,
    pinned: Option<String>,
    status: &SharedStatus,
) -> Result<SshHandle, SessionError> {
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
            if let (None, Some(fingerprint)) = (pinned, received) {
                status.learn_host_key(fingerprint);
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
