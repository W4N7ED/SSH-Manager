//! Tests de bout en bout du client SFTP contre un serveur russh local dont le
//! sous-système SFTP sert un système de fichiers en mémoire.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use russh::keys::{Algorithm, PrivateKey};
use russh::server::{self, Auth, Msg, Session};
use russh::{Channel, ChannelId};
use russh_sftp::protocol::{
    Attrs, Data, File, FileAttributes, FileMode, Handle, Name, OpenFlags, Status, StatusCode,
};
use ssh_manager_core::{Connection, ConnectionKind};
use tokio::net::TcpListener;
use tokio::runtime::Runtime;

use super::Backend;
use crate::session::transport::{SharedStatus, lock};

const USER: &str = "tester";
const PASSWORD: &str = "correct-horse";
const HOME: &str = "/home/tester";

#[derive(Debug, Clone)]
enum Node {
    Directory,
    File(Vec<u8>),
}

/// Système de fichiers partagé entre le serveur et les assertions du test.
type MemoryFs = Arc<Mutex<BTreeMap<String, Node>>>;

fn parent_of(path: &str) -> &str {
    match path.rfind('/') {
        Some(0) | None => "/",
        Some(index) => &path[..index],
    }
}

fn attributes(node: &Node) -> FileAttributes {
    let (mode, size) = match node {
        Node::Directory => (FileMode::DIR.bits() | 0o755, 0),
        Node::File(content) => (FileMode::REG.bits() | 0o644, content.len() as u64),
    };
    FileAttributes {
        size: Some(size),
        permissions: Some(mode),
        mtime: Some(1_700_000_000),
        ..FileAttributes::empty()
    }
}

fn ok(id: u32) -> Status {
    Status {
        id,
        status_code: StatusCode::Ok,
        error_message: "Ok".into(),
        language_tag: "en-US".into(),
    }
}

/// Sous-système SFTP minimal : de quoi exercer chaque opération du navigateur.
struct MemorySftp {
    fs: MemoryFs,
    /// Dossiers ouverts dont le contenu a déjà été envoyé.
    listed_handles: HashMap<String, bool>,
}

impl russh_sftp::server::Handler for MemorySftp {
    type Error = StatusCode;

    fn unimplemented(&self) -> Self::Error {
        StatusCode::OpUnsupported
    }

    async fn realpath(&mut self, id: u32, path: String) -> Result<Name, Self::Error> {
        let resolved = if path == "." { HOME.to_owned() } else { path };
        Ok(Name {
            id,
            files: vec![File::dummy(resolved)],
        })
    }

    async fn opendir(&mut self, id: u32, path: String) -> Result<Handle, Self::Error> {
        match lock(&self.fs).get(&path) {
            Some(Node::Directory) => {
                self.listed_handles.insert(path.clone(), false);
                Ok(Handle { id, handle: path })
            }
            _ => Err(StatusCode::NoSuchFile),
        }
    }

    async fn readdir(&mut self, id: u32, handle: String) -> Result<Name, Self::Error> {
        let already_listed = self.listed_handles.insert(handle.clone(), true);
        if already_listed != Some(false) {
            return Err(StatusCode::Eof);
        }
        let files = lock(&self.fs)
            .iter()
            .filter(|(path, _)| *path != &handle && parent_of(path) == handle)
            .map(|(path, node)| {
                File::new(
                    path[handle.len()..].trim_start_matches('/').to_owned(),
                    attributes(node),
                )
            })
            .collect();
        Ok(Name { id, files })
    }

    async fn close(&mut self, id: u32, handle: String) -> Result<Status, Self::Error> {
        self.listed_handles.remove(&handle);
        Ok(ok(id))
    }

    async fn mkdir(
        &mut self,
        id: u32,
        path: String,
        _attrs: FileAttributes,
    ) -> Result<Status, Self::Error> {
        let mut fs = lock(&self.fs);
        if fs.contains_key(&path) {
            return Err(StatusCode::Failure);
        }
        fs.insert(path, Node::Directory);
        Ok(ok(id))
    }

    async fn remove(&mut self, id: u32, filename: String) -> Result<Status, Self::Error> {
        let mut fs = lock(&self.fs);
        match fs.get(&filename) {
            Some(Node::File(_)) => {
                fs.remove(&filename);
                Ok(ok(id))
            }
            _ => Err(StatusCode::NoSuchFile),
        }
    }

    async fn rmdir(&mut self, id: u32, path: String) -> Result<Status, Self::Error> {
        let mut fs = lock(&self.fs);
        // Comme un vrai serveur : un dossier non vide ne peut pas être supprimé.
        let has_children = fs
            .keys()
            .any(|other| other != &path && parent_of(other) == path);
        match fs.get(&path) {
            Some(Node::Directory) if !has_children => {
                fs.remove(&path);
                Ok(ok(id))
            }
            _ => Err(StatusCode::Failure),
        }
    }

    async fn open(
        &mut self,
        id: u32,
        filename: String,
        pflags: OpenFlags,
        _attrs: FileAttributes,
    ) -> Result<Handle, Self::Error> {
        let mut fs = lock(&self.fs);
        if pflags.contains(OpenFlags::CREATE) {
            fs.insert(filename.clone(), Node::File(Vec::new()));
        }
        match fs.get(&filename) {
            Some(Node::File(_)) => Ok(Handle {
                id,
                handle: filename,
            }),
            _ => Err(StatusCode::NoSuchFile),
        }
    }

    async fn read(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        len: u32,
    ) -> Result<Data, Self::Error> {
        let fs = lock(&self.fs);
        let Some(Node::File(content)) = fs.get(&handle) else {
            return Err(StatusCode::NoSuchFile);
        };
        let start = usize::try_from(offset).unwrap_or(usize::MAX);
        if start >= content.len() {
            return Err(StatusCode::Eof);
        }
        let end = content.len().min(start + len as usize);
        Ok(Data {
            id,
            data: content[start..end].to_vec(),
        })
    }

    async fn write(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<Status, Self::Error> {
        let mut fs = lock(&self.fs);
        let Some(Node::File(content)) = fs.get_mut(&handle) else {
            return Err(StatusCode::NoSuchFile);
        };
        let start = offset as usize;
        if content.len() < start + data.len() {
            content.resize(start + data.len(), 0);
        }
        content[start..start + data.len()].copy_from_slice(&data);
        Ok(ok(id))
    }

    async fn fstat(&mut self, id: u32, handle: String) -> Result<Attrs, Self::Error> {
        self.stat(id, handle).await
    }

    async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        let fs = lock(&self.fs);
        let node = fs.get(&path).ok_or(StatusCode::NoSuchFile)?;
        Ok(Attrs {
            id,
            attrs: attributes(node),
        })
    }
}

#[derive(Clone)]
struct SftpServer {
    fs: MemoryFs,
    channels: Arc<Mutex<HashMap<ChannelId, Channel<Msg>>>>,
}

impl server::Handler for SftpServer {
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
        channel: Channel<Msg>,
        reply: server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        lock(&self.channels).insert(channel.id(), channel);
        reply.accept().await;
        Ok(())
    }

    async fn subsystem_request(
        &mut self,
        channel_id: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let channel = lock(&self.channels).remove(&channel_id);
        match (name, channel) {
            ("sftp", Some(channel)) => {
                session.channel_success(channel_id)?;
                let handler = MemorySftp {
                    fs: self.fs.clone(),
                    listed_handles: HashMap::new(),
                };
                tokio::spawn(russh_sftp::server::run(channel.into_stream(), handler));
                Ok(())
            }
            _ => session.channel_failure(channel_id),
        }
    }
}

/// Démarre le serveur et renvoie son port avec le système de fichiers servi,
/// qui contient déjà le dossier d'accueil.
fn start_server(runtime: &Runtime) -> (u16, MemoryFs) {
    let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519)
        .expect("génération de la clé d'hôte");
    let config = Arc::new(server::Config {
        keys: vec![key],
        ..Default::default()
    });
    let fs: MemoryFs = Arc::new(Mutex::new(BTreeMap::from([
        ("/".to_owned(), Node::Directory),
        ("/home".to_owned(), Node::Directory),
        (HOME.to_owned(), Node::Directory),
    ])));
    let listener = runtime
        .block_on(TcpListener::bind("127.0.0.1:0"))
        .expect("port local libre");
    let port = listener.local_addr().expect("adresse locale").port();
    let handler = SftpServer {
        fs: fs.clone(),
        channels: Arc::new(Mutex::new(HashMap::new())),
    };
    runtime.spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            if let Ok(session) = server::run_stream(config.clone(), stream, handler.clone()).await {
                tokio::spawn(session);
            }
        }
    });
    (port, fs)
}

fn connect(runtime: &Runtime, port: u16) -> Backend {
    let connection = Connection {
        name: "test".into(),
        kind: ConnectionKind::Sftp,
        host: "127.0.0.1".into(),
        port,
        username: USER.into(),
        password: Some(PASSWORD.into()),
        ..Connection::new(0)
    };
    let status = SharedStatus::new(egui::Context::default());
    runtime
        .block_on(Backend::connect(&connection, None, &status))
        .expect("connexion SFTP")
}

fn put(fs: &MemoryFs, path: &str, node: Node) {
    lock(fs).insert(path.to_owned(), node);
}

fn names(runtime: &Runtime, backend: &mut Backend, path: &str) -> Vec<String> {
    runtime
        .block_on(backend.list(path))
        .expect("listage du dossier")
        .into_iter()
        .map(|entry| entry.name)
        .collect()
}

#[test]
fn should_resolve_the_home_folder() {
    let runtime = Runtime::new().unwrap();
    let (port, _fs) = start_server(&runtime);
    let mut backend = connect(&runtime, port);
    assert_eq!(runtime.block_on(backend.home()).unwrap(), HOME);
}

#[test]
fn should_list_folders_first_with_sizes_and_permissions() {
    let runtime = Runtime::new().unwrap();
    let (port, fs) = start_server(&runtime);
    put(&fs, "/home/tester/notes.txt", Node::File(b"hello".to_vec()));
    put(&fs, "/home/tester/www", Node::Directory);
    let mut backend = connect(&runtime, port);

    let entries = runtime.block_on(backend.list(HOME)).unwrap();

    assert_eq!(entries.len(), 2);
    assert_eq!((entries[0].name.as_str(), entries[0].is_dir), ("www", true));
    assert_eq!(
        (entries[1].name.as_str(), entries[1].size),
        ("notes.txt", 5)
    );
    assert_eq!(entries[1].permissions.as_deref(), Some("rw-r--r--"));
    assert_eq!(entries[1].modified, Some(1_700_000_000));
}

#[test]
fn should_upload_then_download_identical_bytes() {
    let runtime = Runtime::new().unwrap();
    let (port, fs) = start_server(&runtime);
    let mut backend = connect(&runtime, port);
    let local_dir = std::env::temp_dir().join(format!("ssh-manager-sftp-{}", uuid_like()));
    std::fs::create_dir_all(&local_dir).unwrap();
    // Plus gros qu'un paquet SFTP, pour exercer le découpage du transfert.
    let content: Vec<u8> = (0..200_000u32).map(|value| (value % 251) as u8).collect();
    let source = local_dir.join("source.bin");
    std::fs::write(&source, &content).unwrap();

    let uploaded = runtime
        .block_on(backend.upload(&source, "/home/tester/copy.bin"))
        .unwrap();
    let target = local_dir.join("downloaded.bin");
    let downloaded = runtime
        .block_on(backend.download("/home/tester/copy.bin", &target))
        .unwrap();

    assert_eq!(uploaded, content.len() as u64);
    assert_eq!(downloaded, content.len() as u64);
    assert!(
        matches!(lock(&fs).get("/home/tester/copy.bin"), Some(Node::File(stored)) if *stored == content)
    );
    assert_eq!(std::fs::read(&target).unwrap(), content);
    std::fs::remove_dir_all(&local_dir).unwrap();
}

#[test]
fn should_create_a_folder() {
    let runtime = Runtime::new().unwrap();
    let (port, _fs) = start_server(&runtime);
    let mut backend = connect(&runtime, port);
    runtime
        .block_on(backend.create_dir("/home/tester/backups"))
        .unwrap();
    assert_eq!(names(&runtime, &mut backend, HOME), ["backups"]);
}

#[test]
fn should_remove_a_whole_tree_deepest_first() {
    let runtime = Runtime::new().unwrap();
    let (port, fs) = start_server(&runtime);
    put(&fs, "/home/tester/site", Node::Directory);
    put(
        &fs,
        "/home/tester/site/index.html",
        Node::File(b"<html>".to_vec()),
    );
    put(&fs, "/home/tester/site/assets", Node::Directory);
    put(
        &fs,
        "/home/tester/site/assets/app.js",
        Node::File(b"run()".to_vec()),
    );
    put(&fs, "/home/tester/keep.txt", Node::File(b"keep".to_vec()));
    let mut backend = connect(&runtime, port);

    runtime
        .block_on(backend.remove_tree("/home/tester/site"))
        .unwrap();

    assert_eq!(names(&runtime, &mut backend, HOME), ["keep.txt"]);
    assert!(
        !lock(&fs)
            .keys()
            .any(|path| path.starts_with("/home/tester/site"))
    );
}

#[test]
fn should_report_a_missing_folder_as_an_operation_error() {
    let runtime = Runtime::new().unwrap();
    let (port, _fs) = start_server(&runtime);
    let mut backend = connect(&runtime, port);
    let error = runtime.block_on(backend.list("/nowhere")).unwrap_err();
    assert!(
        matches!(error, super::FsError::Operation(_)),
        "obtenu : {error:?}"
    );
}

/// Suffixe unique pour isoler les dossiers temporaires de tests parallèles.
fn uuid_like() -> String {
    format!("{}-{:x}", std::process::id(), rand::random::<u64>())
}
