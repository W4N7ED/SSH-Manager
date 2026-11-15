//! Tunnels locaux (`ssh -L`) d'une session : chaque port local écoute sur
//! 127.0.0.1 — jamais sur le réseau, qui pourrait sinon emprunter le tunnel —
//! et chaque connexion reçue part dans un canal SSH vers la destination.

use std::sync::{Arc, Mutex};

use ssh_manager_core::LocalForward;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use super::transport::{SharedStatus, SshHandle, lock};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TunnelState {
    Listening,
    /// Port local indisponible (déjà utilisé, refusé par le système…).
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunnelStatus {
    pub forward: LocalForward,
    pub state: TunnelState,
    /// Connexions relayées depuis l'ouverture.
    pub connections: u32,
}

/// État des tunnels, lu par l'interface.
pub type SharedTunnels = Arc<Mutex<Vec<TunnelStatus>>>;

/// Les tâches d'écoute d'une session ; elles s'arrêtent avec elle.
pub struct Tunnels {
    tasks: Vec<JoinHandle<()>>,
}

impl Drop for Tunnels {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

/// Ouvre les tunnels de `forwards` sur la connexion `handle`.
pub async fn open(
    handle: &Arc<SshHandle>,
    forwards: &[LocalForward],
    shared: &SharedTunnels,
    status: &SharedStatus,
) -> Tunnels {
    let mut tasks = Vec::new();
    let mut statuses = Vec::new();
    for forward in forwards {
        let state = match TcpListener::bind(("127.0.0.1", forward.local_port)).await {
            Ok(listener) => {
                tracing::info!(tunnel = %forward.describe(), "local tunnel listening");
                let index = statuses.len();
                tasks.push(tokio::spawn(serve(
                    listener,
                    handle.clone(),
                    forward.clone(),
                    shared.clone(),
                    index,
                    status.clone(),
                )));
                TunnelState::Listening
            }
            Err(error) => {
                tracing::warn!(tunnel = %forward.describe(), %error, "local tunnel could not listen");
                TunnelState::Failed(format!(
                    "port {} indisponible : {error}",
                    forward.local_port
                ))
            }
        };
        statuses.push(TunnelStatus {
            forward: forward.clone(),
            state,
            connections: 0,
        });
    }
    *lock(shared) = statuses;
    status.repaint();
    Tunnels { tasks }
}

/// Accepte les connexions locales et relaie chacune dans son propre canal.
async fn serve(
    listener: TcpListener,
    handle: Arc<SshHandle>,
    forward: LocalForward,
    shared: SharedTunnels,
    index: usize,
    status: SharedStatus,
) {
    while let Ok((mut socket, peer)) = listener.accept().await {
        if let Some(tunnel) = lock(&shared).get_mut(index) {
            tunnel.connections += 1;
        }
        status.repaint();
        let handle = handle.clone();
        let forward = forward.clone();
        tokio::spawn(async move {
            let channel = match handle
                .channel_open_direct_tcpip(
                    forward.remote_host.clone(),
                    u32::from(forward.remote_port),
                    peer.ip().to_string(),
                    u32::from(peer.port()),
                )
                .await
            {
                Ok(channel) => channel,
                Err(error) => {
                    tracing::warn!(tunnel = %forward.describe(), %error, "server refused the tunnel channel");
                    return;
                }
            };
            let mut stream = channel.into_stream();
            if let Err(error) = tokio::io::copy_bidirectional(&mut socket, &mut stream).await {
                tracing::debug!(tunnel = %forward.describe(), %error, "tunnel connection ended with an error");
            }
        });
    }
}
