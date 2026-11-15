//! Enregistrement des sessions terminal : le texte de chaque session est
//! conservé, chiffré avec une clé dérivée du coffre, et peut être exporté en
//! texte brut à la demande.
//!
//! Un enregistrement est un journal scellé ([`sealed_log`]) dont le premier
//! élément est un en-tête JSON, suivi de blocs de texte UTF-8.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ssh_manager_core::Connection;
use ssh_manager_vault::VaultError;
use ssh_manager_vault::crypto::VaultKey;
use ssh_manager_vault::sealed_log::{self, SealedLogWriter};

use crate::session::transcript::TextExtractor;

const EXTENSION: &str = "smlog";
const HEADER_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum RecordingError {
    #[error(transparent)]
    Vault(#[from] VaultError),
    #[error("en-tête d'enregistrement illisible : {0}")]
    Header(String),
    #[error("impossible d'écrire {path} : {source}")]
    Export {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingHeader {
    pub version: u32,
    pub connection_name: String,
    pub target: String,
    /// Début de la session, en secondes depuis l'époque Unix.
    pub started_at: i64,
}

/// Un enregistrement présent sur le disque.
#[derive(Debug, Clone)]
pub struct RecordingSummary {
    pub path: PathBuf,
    pub header: RecordingHeader,
    pub size_bytes: u64,
}

/// Enregistre une session au fil de l'eau. Une erreur d'écriture est
/// journalisée puis arrête l'enregistrement, sans interrompre la session.
pub struct Recorder {
    writer: Option<SealedLogWriter>,
    extractor: TextExtractor,
}

fn now_seconds() -> i64 {
    chrono::Local::now().timestamp()
}

fn file_name(started_at: i64) -> String {
    let date = chrono::DateTime::from_timestamp(started_at, 0)
        .map(|date| {
            date.with_timezone(&chrono::Local)
                .format("%Y-%m-%d_%H-%M-%S")
                .to_string()
        })
        .unwrap_or_else(|| started_at.to_string());
    // Le suffixe aléatoire évite toute collision entre sessions ouvertes la même seconde.
    format!(
        "{date}_{:06x}.{EXTENSION}",
        rand::random::<u32>() & 0xff_ffff
    )
}

impl Recorder {
    /// Enregistreur inactif : la session n'est pas enregistrée.
    pub fn disabled() -> Self {
        Self {
            writer: None,
            extractor: TextExtractor::default(),
        }
    }

    /// Démarre un enregistrement dans `directory`. En cas d'échec, la session
    /// continue sans enregistrement et l'erreur est journalisée.
    pub fn start(directory: &Path, key: VaultKey, connection: &Connection) -> Self {
        let header = RecordingHeader {
            version: HEADER_VERSION,
            connection_name: connection.name.clone(),
            target: connection.target(),
            started_at: now_seconds(),
        };
        let path = directory.join(file_name(header.started_at));
        let opened = SealedLogWriter::create(&path, key).and_then(|mut writer| {
            let header = serde_json::to_vec(&header).expect("en-tête sérialisable");
            writer.append(&header).map(|()| writer)
        });
        match opened {
            Ok(writer) => {
                tracing::info!(path = %path.display(), "session recording started");
                Self {
                    writer: Some(writer),
                    extractor: TextExtractor::default(),
                }
            }
            Err(error) => {
                tracing::error!(path = %path.display(), %error, "failed to start the session recording");
                Self::disabled()
            }
        }
    }

    pub fn path(&self) -> Option<&Path> {
        self.writer.as_ref().map(SealedLogWriter::path)
    }

    /// Ligne en cours, pas encore écrite (souvent le prompt).
    pub fn pending_line(&self) -> String {
        self.extractor.pending_line()
    }

    /// Octets reçus du serveur.
    pub fn feed(&mut self, bytes: &[u8]) {
        if self.writer.is_some() {
            let lines = self.extractor.feed(bytes);
            self.append_lines(&lines);
        }
    }

    /// Mention horodatée (reconnexion, fin de session), sur sa propre ligne.
    pub fn note(&mut self, text: &str) {
        let time = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let mut lines: Vec<String> = self.extractor.flush_pending().into_iter().collect();
        lines.push(format!("── {text} · {time} ──"));
        self.append_lines(&lines);
    }

    fn append_lines(&mut self, lines: &[String]) {
        let Some(writer) = &mut self.writer else {
            return;
        };
        if lines.is_empty() {
            return;
        }
        let mut text = lines.join("\n");
        text.push('\n');
        if let Err(error) = writer.append(text.as_bytes()) {
            tracing::error!(path = %writer.path().display(), %error, "session recording stopped after a write failure");
            self.writer = None;
        }
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        if self.writer.is_some() {
            self.note("fin de l'enregistrement");
        }
    }
}

fn read_header(path: &Path, key: &[u8; 32]) -> Result<RecordingHeader, RecordingError> {
    let log = sealed_log::read(path, key, Some(1))?;
    let first = log
        .records
        .first()
        .ok_or_else(|| RecordingError::Header("enregistrement vide".into()))?;
    serde_json::from_slice(first).map_err(|error| RecordingError::Header(error.to_string()))
}

/// Enregistrements lisibles avec cette clé, du plus récent au plus ancien.
/// Ceux d'un autre coffre ou corrompus sont ignorés (et journalisés).
pub fn list(directory: &Path, key: &[u8; 32]) -> Vec<RecordingSummary> {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => {
            tracing::error!(path = %directory.display(), %error, "failed to list session recordings");
            return Vec::new();
        }
    };
    let mut recordings: Vec<RecordingSummary> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == EXTENSION))
        .filter_map(|path| match read_header(&path, key) {
            Ok(header) => Some(RecordingSummary {
                size_bytes: std::fs::metadata(&path).map(|metadata| metadata.len()).unwrap_or(0),
                path,
                header,
            }),
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "session recording skipped: unreadable");
                None
            }
        })
        .collect();
    recordings.sort_by_key(|recording| std::cmp::Reverse(recording.header.started_at));
    recordings
}

/// Durées de conservation proposées, en jours (`None` : toujours).
pub const RETENTION_OPTIONS: [Option<u32>; 4] = [None, Some(30), Some(90), Some(365)];

pub fn retention_label(days: Option<u32>) -> String {
    match days {
        None => "Toujours".into(),
        Some(365) => "1 an".into(),
        Some(days) => format!("{days} jours"),
    }
}

/// Supprime les enregistrements écrits pour la dernière fois il y a plus de
/// `max_age`, sauf ceux des sessions encore ouvertes (`keep`). L'âge est pris
/// sur la date de modification : inutile de déchiffrer pour trier.
/// Renvoie le nombre de fichiers supprimés.
pub fn purge_older_than(
    directory: &Path,
    max_age: std::time::Duration,
    now: std::time::SystemTime,
    keep: &[PathBuf],
) -> usize {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return 0;
    };
    let mut removed = 0;
    for path in entries.filter_map(Result::ok).map(|entry| entry.path()) {
        let is_recording = path
            .extension()
            .is_some_and(|extension| extension == EXTENSION);
        let modified = std::fs::metadata(&path).and_then(|metadata| metadata.modified());
        let expired = modified
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > max_age);
        if !is_recording || !expired || keep.contains(&path) {
            continue;
        }
        match std::fs::remove_file(&path) {
            Ok(()) => removed += 1,
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "expired session recording could not be removed")
            }
        }
    }
    if removed > 0 {
        tracing::info!(
            removed,
            max_age_days = max_age.as_secs() / 86_400,
            "expired session recordings removed"
        );
    }
    removed
}

/// Date locale lisible, « 2026-11-15 14:03 ».
pub fn format_date(seconds: i64) -> String {
    chrono::DateTime::from_timestamp(seconds, 0)
        .map(|date| {
            date.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_default()
}

fn document(header: &RecordingHeader, body: &str) -> String {
    format!(
        "SSH Manager — enregistrement de session\nConnexion : {} ({})\nDébut : {}\n{}\n\n{}",
        header.connection_name,
        header.target,
        format_date(header.started_at),
        "─".repeat(60),
        body
    )
}

/// Export d'une session non enregistrée : on n'a que l'historique de l'écran.
pub fn screen_document(connection: &Connection, screen_text: &str) -> String {
    format!(
        "SSH Manager — historique du terminal (session non enregistrée)\nConnexion : {} ({})\nExporté le : {}\n{}\n\n{}\n",
        connection.name,
        connection.target(),
        format_date(now_seconds()),
        "─".repeat(60),
        screen_text
    )
}

/// Texte complet d'un enregistrement, prêt à exporter. `pending` est la ligne
/// en cours d'une session encore ouverte.
pub fn read_text(path: &Path, key: &[u8; 32], pending: &str) -> Result<String, RecordingError> {
    let log = sealed_log::read(path, key, None)?;
    let mut records = log.records.iter();
    let header: RecordingHeader = records
        .next()
        .ok_or_else(|| RecordingError::Header("enregistrement vide".into()))
        .and_then(|first| {
            serde_json::from_slice(first).map_err(|error| RecordingError::Header(error.to_string()))
        })?;
    let mut body: String = records
        .map(|record| String::from_utf8_lossy(record).into_owned())
        .collect();
    if !pending.is_empty() {
        body.push_str(pending);
        body.push('\n');
    }
    if log.truncated {
        body.push_str(
            "[fin de l'enregistrement tronquée : l'application a été arrêtée brutalement]\n",
        );
    }
    Ok(document(&header, &body))
}

/// Écrit un texte exporté à l'emplacement choisi par l'utilisateur.
pub fn write_export(target: &Path, text: &str) -> Result<(), RecordingError> {
    // CRLF : le fichier s'ouvre proprement dans le Bloc-notes et les outils Windows.
    std::fs::write(target, text.replace('\n', "\r\n")).map_err(|source| RecordingError::Export {
        path: target.to_owned(),
        source,
    })
}

/// Nom de fichier proposé à l'export : « serveur-web_2026-11-15_14-03.txt ».
pub fn export_file_name(header_name: &str, started_at: i64) -> String {
    let safe: String = header_name
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || "-_.".contains(character) {
                character
            } else {
                '-'
            }
        })
        .collect();
    let date = format_date(started_at).replace([' ', ':'], "-");
    format!("{}_{date}.txt", safe.trim_matches('-'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ssh_manager_vault::sealed_log::derive_log_key;

    fn key() -> VaultKey {
        derive_log_key(&[3u8; 32])
    }

    fn temp_dir(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "ssh-manager-recordings-{name}-{:x}",
            rand::random::<u64>()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        directory
    }

    fn connection() -> Connection {
        Connection {
            name: "web".into(),
            host: "10.0.0.5".into(),
            username: "deploy".into(),
            ..Connection::new(0)
        }
    }

    #[test]
    fn should_record_the_text_of_a_session_and_read_it_back() {
        let directory = temp_dir("roundtrip");
        let mut recorder = Recorder::start(&directory, key(), &connection());
        recorder.feed(b"\x1b[32mdeploy@web\x1b[0m:~$ uptime\r\n 10:00 up 3 days\r\n");
        let path = recorder.path().unwrap().to_owned();
        drop(recorder);

        let text = read_text(&path, &key(), "").unwrap();
        assert!(text.contains("Connexion : web (deploy@10.0.0.5:22)"));
        assert!(text.contains("deploy@web:~$ uptime\n 10:00 up 3 days\n"));
        assert!(text.contains("fin de l'enregistrement"));
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn should_list_recordings_newest_first() {
        let directory = temp_dir("list");
        let first = Recorder::start(&directory, key(), &connection());
        drop(first);
        let recordings = list(&directory, &key());
        assert_eq!(recordings.len(), 1);
        assert_eq!(recordings[0].header.connection_name, "web");
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn should_skip_recordings_of_another_vault() {
        let directory = temp_dir("other-vault");
        drop(Recorder::start(&directory, key(), &connection()));
        let other = derive_log_key(&[4u8; 32]);
        assert!(list(&directory, &other).is_empty());
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn should_include_the_pending_prompt_of_a_live_session() {
        let directory = temp_dir("pending");
        let mut recorder = Recorder::start(&directory, key(), &connection());
        recorder.feed(b"ok\r\n$ ");
        let text = read_text(recorder.path().unwrap(), &key(), &recorder.pending_line()).unwrap();
        assert!(text.ends_with("ok\n$\n"));
        drop(recorder);
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn should_not_record_when_disabled() {
        let mut recorder = Recorder::disabled();
        recorder.feed(b"secret\r\n");
        assert!(recorder.path().is_none());
        assert_eq!(recorder.pending_line(), "", "rien n'est même analysé");
    }

    #[test]
    fn should_purge_only_expired_recordings_of_closed_sessions() {
        let directory = temp_dir("purge");
        let day = std::time::Duration::from_secs(86_400);
        let now = std::time::SystemTime::now();
        let write = |name: &str, age_days: u64| {
            let path = directory.join(name);
            std::fs::write(&path, b"x").unwrap();
            let file = std::fs::File::options().write(true).open(&path).unwrap();
            file.set_modified(now - day * age_days as u32).unwrap();
            path
        };
        let old = write("old.smlog", 40);
        let recent = write("recent.smlog", 5);
        let live = write("live.smlog", 40);
        let other = write("notes.txt", 400);

        let removed = purge_older_than(&directory, day * 30, now, std::slice::from_ref(&live));

        assert_eq!(removed, 1);
        assert!(!old.exists());
        assert!(recent.exists() && live.exists() && other.exists());
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn should_propose_a_safe_export_file_name() {
        let name = export_file_name("prod / web:01", 0);
        assert!(name.starts_with("prod---web-01_"), "{name}");
        assert!(name.ends_with(".txt"));
        assert!(!name.contains(['/', ':', ' ']));
    }
}
