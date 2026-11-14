//! Tests de bout en bout du client SSH contre un vrai serveur SSH (russh)
//! lancé en local sur un port éphémère.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use russh::keys::{Algorithm, HashAlg, PrivateKey};
use russh::server::{self, Auth, Msg, Session};
use russh::{Channel, ChannelId};
use ssh_manager_core::Connection;
use tokio::net::TcpListener;
use tokio::runtime::Runtime;

use super::emulator::GridSize;
use super::ssh::{SessionError, SessionState, SshSession, lock};

const USER: &str = "tester";
const PASSWORD: &str = "correct-horse";
const SIZE: GridSize = GridSize {
    columns: 80,
    rows: 24,
};

/// Tailles de terminal reçues par le serveur (demande de PTY puis changements).
type ReceivedSizes = Arc<Mutex<Vec<(u32, u32)>>>;

#[derive(Clone)]
struct EchoShell {
    sizes: ReceivedSizes,
}

impl server::Handler for EchoShell {
    type Error = russh::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        Ok(if user == USER && password == PASSWORD {
            Auth::Accept
        } else {
            Auth::reject()
        })
    }

    async fn channel_open_session(
        &mut self,
        _channel: Channel<Msg>,
        reply: server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        columns: u32,
        rows: u32,
        _pixel_width: u32,
        _pixel_height: u32,
        _modes: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        lock(&self.sizes).push((columns, rows));
        session.channel_success(channel)
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.data(channel, b"welcome\r\n".to_vec())?;
        session.channel_success(channel)
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.data(channel, data.to_vec())
    }

    async fn window_change_request(
        &mut self,
        _channel: ChannelId,
        columns: u32,
        rows: u32,
        _pixel_width: u32,
        _pixel_height: u32,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        lock(&self.sizes).push((columns, rows));
        Ok(())
    }
}

struct TestServer {
    port: u16,
    fingerprint: String,
    sizes: ReceivedSizes,
}

fn start_server(runtime: &Runtime) -> TestServer {
    let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519)
        .expect("génération de la clé d'hôte");
    let fingerprint = key.public_key().fingerprint(HashAlg::Sha256).to_string();
    let config = Arc::new(server::Config {
        keys: vec![key],
        ..Default::default()
    });
    let sizes: ReceivedSizes = Arc::new(Mutex::new(Vec::new()));
    let listener = runtime
        .block_on(TcpListener::bind("127.0.0.1:0"))
        .expect("port local libre");
    let port = listener.local_addr().expect("adresse locale").port();

    let handler = EchoShell {
        sizes: sizes.clone(),
    };
    runtime.spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            if let Ok(session) = server::run_stream(config.clone(), stream, handler.clone()).await {
                tokio::spawn(session);
            }
        }
    });
    TestServer {
        port,
        fingerprint,
        sizes,
    }
}

fn connection(port: u16, password: &str) -> Connection {
    Connection {
        name: "test".into(),
        host: "127.0.0.1".into(),
        port,
        username: USER.into(),
        password: Some(password.into()),
        ..Connection::new(0)
    }
}

/// Attend qu'une condition devienne vraie, ou échoue après 10 s.
fn wait_until(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "délai dépassé en attendant : {what}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn screen_contains(session: &SshSession, text: &str) -> bool {
    lock(&session.emulator)
        .screen_text()
        .iter()
        .any(|line| line.contains(text))
}

#[test]
fn should_connect_pin_the_host_key_and_relay_bytes() {
    let runtime = Runtime::new().unwrap();
    let server = start_server(&runtime);
    let session = SshSession::start(
        &runtime,
        egui::Context::default(),
        connection(server.port, PASSWORD),
        None,
        SIZE,
    );

    wait_until("connexion établie", || {
        matches!(session.state(), SessionState::Connected)
    });
    assert_eq!(
        session.take_learned_host_key(),
        Some(server.fingerprint.clone()),
        "empreinte à épingler"
    );
    assert_eq!(
        session.take_learned_host_key(),
        None,
        "empreinte livrée une seule fois"
    );

    wait_until("message d'accueil affiché", || {
        screen_contains(&session, "welcome")
    });
    session.send(b"bonjour".to_vec());
    wait_until("écho de la saisie", || {
        screen_contains(&session, "bonjour")
    });

    session.resize(GridSize {
        columns: 100,
        rows: 30,
    });
    wait_until("changement de taille transmis", || {
        lock(&server.sizes).contains(&(100, 30))
    });
    assert_eq!(lock(&server.sizes)[0], (80, 24), "taille initiale du PTY");
}

#[test]
fn should_accept_the_pinned_host_key_without_learning_it_again() {
    let runtime = Runtime::new().unwrap();
    let server = start_server(&runtime);
    let session = SshSession::start(
        &runtime,
        egui::Context::default(),
        connection(server.port, PASSWORD),
        Some(server.fingerprint.clone()),
        SIZE,
    );
    wait_until("connexion établie", || {
        matches!(session.state(), SessionState::Connected)
    });
    assert_eq!(session.take_learned_host_key(), None);
}

#[test]
fn should_refuse_a_server_whose_host_key_changed() {
    let runtime = Runtime::new().unwrap();
    let server = start_server(&runtime);
    let pinned = "SHA256:cle-epinglee-auparavant".to_owned();
    let session = SshSession::start(
        &runtime,
        egui::Context::default(),
        connection(server.port, PASSWORD),
        Some(pinned.clone()),
        SIZE,
    );

    wait_until("échec de connexion", || {
        matches!(session.state(), SessionState::Failed(_))
    });
    match session.state() {
        SessionState::Failed(SessionError::HostKeyChanged { known, received }) => {
            assert_eq!(known, pinned);
            assert_eq!(received, server.fingerprint);
        }
        other => panic!("refus attendu pour clé changée, obtenu : {other:?}"),
    }
}

#[test]
fn should_report_a_rejected_password() {
    let runtime = Runtime::new().unwrap();
    let server = start_server(&runtime);
    let session = SshSession::start(
        &runtime,
        egui::Context::default(),
        connection(server.port, "mauvais"),
        None,
        SIZE,
    );
    wait_until("échec d'authentification", || {
        matches!(session.state(), SessionState::Failed(_))
    });
    assert!(matches!(
        session.state(),
        SessionState::Failed(SessionError::Authentication)
    ));
}

#[test]
fn should_report_an_unreachable_server() {
    let runtime = Runtime::new().unwrap();
    // Port fermé : on réserve un port puis on libère l'écoute.
    let port = runtime
        .block_on(TcpListener::bind("127.0.0.1:0"))
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let session = SshSession::start(
        &runtime,
        egui::Context::default(),
        connection(port, PASSWORD),
        None,
        SIZE,
    );
    wait_until("échec de connexion", || {
        matches!(session.state(), SessionState::Failed(_))
    });
    assert!(matches!(
        session.state(),
        SessionState::Failed(SessionError::Connect(_))
    ));
}
