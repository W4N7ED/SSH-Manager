//! Côté interface du navigateur de fichiers : l'état affiché, et les commandes
//! envoyées à la tâche réseau qui détient le client.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use ssh_manager_core::Connection;
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use super::transfer::TransferHooks;
use super::{Backend, FsError, RemoteEntry, remote_path, upload_plan};
use crate::session::transport::{LearnedHostKey, Route, SessionState, SharedStatus, lock};

/// Avancement du transfert en cours.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferStatus {
    /// « Envoi de app.js (2/14) ».
    pub label: String,
    pub done: u64,
    pub total: Option<u64>,
}

/// Ce que le panneau affiche.
#[derive(Default, Clone)]
pub struct Listing {
    pub path: String,
    pub entries: Vec<RemoteEntry>,
    /// Opération en cours (« Suppression de… »), qui grise les actions.
    pub busy: Option<String>,
    pub transfer: Option<TransferStatus>,
    /// Dernier résultat : texte et réussite.
    pub message: Option<(String, bool)>,
}

enum Command {
    Open(String),
    Refresh,
    CreateDir(String),
    Delete {
        name: String,
        is_dir: bool,
    },
    Rename {
        from: String,
        to: String,
    },
    SetPermissions {
        name: String,
        mode: u32,
    },
    Download {
        name: String,
        size: u64,
        target: PathBuf,
    },
    /// Fichier ou dossier local, envoyé dans le dossier courant.
    Upload {
        source: PathBuf,
    },
}

/// Un transfert à exécuter par le client.
enum Job<'a> {
    Download {
        remote: &'a str,
        target: &'a std::path::Path,
    },
    Upload {
        local: &'a std::path::Path,
        remote: &'a str,
    },
}

pub struct FileBrowser {
    status: SharedStatus,
    listing: Arc<Mutex<Listing>>,
    commands: UnboundedSender<Command>,
    /// Levé par l'utilisateur : interrompt le transfert en cours et ceux en attente.
    cancel: Arc<AtomicBool>,
}

impl FileBrowser {
    pub fn start(
        runtime: &Runtime,
        ctx: egui::Context,
        connection: Connection,
        route: Route,
    ) -> Self {
        let (commands, receiver) = unbounded_channel();
        let browser = Self {
            status: SharedStatus::new(ctx),
            listing: Arc::new(Mutex::new(Listing::default())),
            commands,
            cancel: Arc::new(AtomicBool::new(false)),
        };
        let worker = Worker {
            status: browser.status.clone(),
            listing: browser.listing.clone(),
            cancel: browser.cancel.clone(),
        };
        runtime.spawn(worker.run(connection, route, receiver));
        browser
    }

    pub fn state(&self) -> SessionState {
        self.status.state()
    }

    pub fn take_learned_host_keys(&self) -> Vec<LearnedHostKey> {
        self.status.take_learned_host_keys()
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

    pub fn rename(&self, from: String, to: String) {
        self.send(Command::Rename { from, to });
    }

    pub fn set_permissions(&self, name: String, mode: u32) {
        self.send(Command::SetPermissions { name, mode });
    }

    pub fn download(&self, name: String, size: u64, target: PathBuf) {
        self.send(Command::Download { name, size, target });
    }

    pub fn upload(&self, source: PathBuf) {
        self.send(Command::Upload { source });
    }

    pub fn cancel_transfers(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

struct Worker {
    status: SharedStatus,
    listing: Arc<Mutex<Listing>>,
    cancel: Arc<AtomicBool>,
}

impl Worker {
    async fn run(
        self,
        connection: Connection,
        route: Route,
        mut commands: UnboundedReceiver<Command>,
    ) {
        let target = connection.target();
        tracing::info!(%target, protocol = connection.kind.label(), "file session connecting");
        let mut backend = match Backend::connect(&connection, &route, &self.status).await {
            Ok(backend) => backend,
            Err(error) => {
                tracing::warn!(%target, %error, "file session failed");
                let error = match error {
                    FsError::Session(error) => error,
                    other => crate::session::transport::SessionError::Connect(other.to_string()),
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
            // File vide : une annulation a produit tout son effet.
            if commands.is_empty() {
                self.cancel.store(false, Ordering::Relaxed);
            }
        }
        backend.close().await;
        tracing::info!(%target, "file session closed");
        self.status.set(SessionState::Closed {
            reason: None,
            connection_lost: false,
        });
    }

    fn current_path(&self) -> String {
        lock(&self.listing).path.clone()
    }

    fn update(&self, change: impl FnOnce(&mut Listing)) {
        change(&mut lock(&self.listing));
        self.status.repaint();
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
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
            Command::Open(path) => return self.show(backend, path).await,
            Command::Refresh => return self.show(backend, current).await,
            Command::CreateDir(name) => {
                self.begin(format!("Création de « {name} »…"));
                let result = backend
                    .create_dir(&remote_path::join(&current, &name))
                    .await;
                ("Dossier créé".to_owned(), result)
            }
            Command::Delete { name, is_dir } => {
                self.begin(format!("Suppression de « {name} »…"));
                let path = remote_path::join(&current, &name);
                let result = if is_dir {
                    backend.remove_tree(&path).await
                } else {
                    backend.remove_file(&path).await
                };
                ("Supprimé".to_owned(), result)
            }
            Command::Rename { from, to } => {
                self.begin(format!("Renommage de « {from} »…"));
                let result = backend
                    .rename(
                        &remote_path::join(&current, &from),
                        &remote_path::join(&current, &to),
                    )
                    .await;
                (format!("Renommé en « {to} »"), result)
            }
            Command::SetPermissions { name, mode } => {
                self.begin(format!("Droits de « {name} »…"));
                let result = backend
                    .set_permissions(&remote_path::join(&current, &name), mode)
                    .await;
                (format!("Droits {mode:o} appliqués"), result)
            }
            Command::Download { name, size, target } => {
                let label = format!("Téléchargement de {name}");
                let remote = remote_path::join(&current, &name);
                let job = Job::Download {
                    remote: &remote,
                    target: &target,
                };
                let result = self.transfer(backend, &label, Some(size), job).await;
                ("Téléchargement terminé".to_owned(), result)
            }
            Command::Upload { source } => {
                let result = self.upload(backend, &source, &current).await;
                ("Envoi terminé".to_owned(), result)
            }
        };
        self.finish(outcome.map(|()| activity));
        self.show(backend, current).await;
    }

    /// Exécute un transfert en publiant son avancement. Un transfert demandé
    /// après une annulation est sauté.
    async fn transfer(
        &self,
        backend: &mut Backend,
        label: &str,
        total: Option<u64>,
        job: Job<'_>,
    ) -> Result<(), FsError> {
        if self.cancelled() {
            return Err(FsError::Cancelled);
        }
        self.update(|listing| {
            listing.message = None;
            listing.transfer = Some(TransferStatus {
                label: label.to_owned(),
                done: 0,
                total,
            });
        });
        let listing = self.listing.clone();
        let status = self.status.clone();
        let mut progress = move |done: u64| {
            if let Some(transfer) = &mut lock(&listing).transfer {
                transfer.done = done;
            }
            status.repaint();
        };
        let hooks = TransferHooks {
            progress: &mut progress,
            cancelled: &self.cancel,
        };
        let result = match job {
            Job::Download { remote, target } => backend.download(remote, target, hooks).await,
            Job::Upload { local, remote } => backend.upload(local, remote, hooks).await,
        }
        .map(|_| ());
        self.update(|listing| listing.transfer = None);
        result
    }

    /// Envoie un fichier, ou un dossier et tout son contenu.
    async fn upload(
        &self,
        backend: &mut Backend,
        source: &std::path::Path,
        current: &str,
    ) -> Result<(), FsError> {
        let plan = upload_plan::plan(source, current).map_err(|error| {
            FsError::Operation(format!(
                "lecture de « {} » impossible : {error}",
                source.display()
            ))
        })?;
        for directory in &plan.directories {
            // Un dossier déjà présent n'empêche pas d'y envoyer des fichiers ;
            // un vrai problème de droits se verra à l'envoi du premier fichier.
            if let Err(error) = backend.create_dir(directory).await {
                tracing::debug!(%directory, %error, "remote folder not created (may already exist)");
            }
        }
        let count = plan.files.len();
        for (index, (local, remote, size)) in plan.files.iter().enumerate() {
            let name = local
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            let label = if count > 1 {
                format!("Envoi de {name} ({}/{count})", index + 1)
            } else {
                format!("Envoi de {name}")
            };
            let job = Job::Upload { local, remote };
            self.transfer(backend, &label, Some(*size), job).await?;
        }
        Ok(())
    }

    fn begin(&self, activity: String) {
        self.update(|listing| {
            listing.busy = Some(activity);
            listing.message = None;
        });
    }

    fn finish(&self, outcome: Result<String, FsError>) {
        let message = match outcome {
            Ok(text) => (text, true),
            Err(FsError::Cancelled) => ("Transfert annulé".to_owned(), false),
            Err(error) => {
                tracing::warn!(%error, "file operation failed");
                (error.to_string(), false)
            }
        };
        self.update(|listing| {
            listing.busy = None;
            listing.message = Some(message);
        });
    }
}
