//! Côté interface du navigateur de fichiers : l'état affiché, et les commandes
//! envoyées à la tâche réseau qui détient le client.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use ssh_manager_core::Connection;
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use super::{Backend, FsError, RemoteEntry, remote_path};
use crate::session::transport::{SessionState, SharedStatus, lock};

/// Ce que le panneau affiche.
#[derive(Default, Clone)]
pub struct Listing {
    pub path: String,
    pub entries: Vec<RemoteEntry>,
    /// Opération en cours (« Téléchargement de… »), qui grise les actions.
    pub busy: Option<String>,
    /// Dernier résultat : texte et réussite.
    pub message: Option<(String, bool)>,
}

enum Command {
    Open(String),
    Refresh,
    CreateDir(String),
    Delete { name: String, is_dir: bool },
    Download { name: String, target: PathBuf },
    Upload { source: PathBuf },
}

pub struct FileBrowser {
    status: SharedStatus,
    listing: Arc<Mutex<Listing>>,
    commands: UnboundedSender<Command>,
}

impl FileBrowser {
    pub fn start(
        runtime: &Runtime,
        ctx: egui::Context,
        connection: Connection,
        pinned_host_key: Option<String>,
    ) -> Self {
        let (commands, receiver) = unbounded_channel();
        let browser = Self {
            status: SharedStatus::new(ctx),
            listing: Arc::new(Mutex::new(Listing::default())),
            commands,
        };
        let worker = Worker {
            status: browser.status.clone(),
            listing: browser.listing.clone(),
        };
        runtime.spawn(worker.run(connection, pinned_host_key, receiver));
        browser
    }

    pub fn state(&self) -> SessionState {
        self.status.state()
    }

    pub fn take_learned_host_key(&self) -> Option<String> {
        self.status.take_learned_host_key()
    }

    pub fn listing(&self) -> Listing {
        lock(&self.listing).clone()
    }

    fn send(&self, command: Command) {
        // Un envoi n'échoue que si la session est terminée : l'action est sans objet.
        let _ = self.commands.send(command);
    }

    pub fn open(&self, path: String) {
        self.send(Command::Open(path));
    }

    pub fn refresh(&self) {
        self.send(Command::Refresh);
    }

    pub fn create_dir(&self, name: String) {
        self.send(Command::CreateDir(name));
    }

    pub fn delete(&self, name: String, is_dir: bool) {
        self.send(Command::Delete { name, is_dir });
    }

    pub fn download(&self, name: String, target: PathBuf) {
        self.send(Command::Download { name, target });
    }

    pub fn upload(&self, source: PathBuf) {
        self.send(Command::Upload { source });
    }
}

struct Worker {
    status: SharedStatus,
    listing: Arc<Mutex<Listing>>,
}

impl Worker {
    async fn run(
        self,
        connection: Connection,
        pinned: Option<String>,
        mut commands: UnboundedReceiver<Command>,
    ) {
        let target = connection.target();
        tracing::info!(%target, protocol = connection.kind.label(), "file session connecting");
        let mut backend = match Backend::connect(&connection, pinned, &self.status).await {
            Ok(backend) => backend,
            Err(error) => {
                tracing::warn!(%target, %error, "file session failed");
                let error = match error {
                    FsError::Session(error) => error,
                    FsError::Operation(message) => {
                        crate::session::transport::SessionError::Connect(message)
                    }
                };
                self.status.set(SessionState::Failed(error));
                return;
            }
        };
        self.status.set(SessionState::Connected);

        let home = backend.home().await.unwrap_or_else(|_| "/".to_owned());
        self.show(&mut backend, home).await;
        // La boucle se termine quand l'onglet est fermé (canal abandonné).
        while let Some(command) = commands.recv().await {
            self.execute(&mut backend, command).await;
        }
        backend.close().await;
        tracing::info!(%target, "file session closed");
        self.status.set(SessionState::Closed { reason: None });
    }

    fn current_path(&self) -> String {
        lock(&self.listing).path.clone()
    }

    fn update(&self, change: impl FnOnce(&mut Listing)) {
        change(&mut lock(&self.listing));
        self.status.repaint();
    }

    /// Liste `path` et l'affiche ; en cas d'échec, le dossier précédent reste affiché.
    async fn show(&self, backend: &mut Backend, path: String) {
        match backend.list(&path).await {
            Ok(entries) => self.update(|listing| {
                listing.path = path;
                listing.entries = entries;
            }),
            Err(error) => self.update(|listing| listing.message = Some((error.to_string(), false))),
        }
    }

    async fn execute(&self, backend: &mut Backend, command: Command) {
        let current = self.current_path();
        let (activity, outcome) = match command {
            Command::Open(path) => {
                self.show(backend, path).await;
                return;
            }
            Command::Refresh => {
                self.show(backend, current).await;
                return;
            }
            Command::CreateDir(name) => {
                self.begin(format!("Création de « {name} »…"));
                (
                    "Dossier créé",
                    backend
                        .create_dir(&remote_path::join(&current, &name))
                        .await,
                )
            }
            Command::Delete { name, is_dir } => {
                self.begin(format!("Suppression de « {name} »…"));
                let path = remote_path::join(&current, &name);
                let result = if is_dir {
                    backend.remove_tree(&path).await
                } else {
                    backend.remove_file(&path).await
                };
                ("Supprimé", result)
            }
            Command::Download { name, target } => {
                self.begin(format!("Téléchargement de « {name} »…"));
                let result = backend
                    .download(&remote_path::join(&current, &name), &target)
                    .await
                    .map(|_| ());
                ("Téléchargement terminé", result)
            }
            Command::Upload { source } => {
                let name = source
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.begin(format!("Envoi de « {name} »…"));
                (
                    "Envoi terminé",
                    backend
                        .upload(&source, &remote_path::join(&current, &name))
                        .await
                        .map(|_| ()),
                )
            }
        };

        let message = match outcome {
            Ok(()) => (activity.to_owned(), true),
            Err(error) => {
                tracing::warn!(%error, "file operation failed");
                (error.to_string(), false)
            }
        };
        self.update(|listing| {
            listing.busy = None;
            listing.message = Some(message);
        });
        self.show(backend, current).await;
    }

    fn begin(&self, activity: String) {
        self.update(|listing| {
            listing.busy = Some(activity);
            listing.message = None;
        });
    }
}
