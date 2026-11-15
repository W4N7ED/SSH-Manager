//! Enregistrements de session : export d'une session ouverte, fenêtre
//! « Historique ».

use super::*;

impl Workspace {
    pub(super) fn export_session(&mut self, id: u64) {
        let Some(session) = self.sessions.iter().find(|session| session.id == id) else {
            return;
        };
        let SessionKind::Terminal { buffers, .. } = &session.kind else {
            return;
        };
        let suggested = recordings::export_file_name(
            &session.connection.name,
            chrono::Local::now().timestamp(),
        );
        let text = {
            let recorder = lock(&buffers.recorder);
            match recorder.path() {
                Some(path) => recordings::read_text(
                    path,
                    &self.vault.recording_key(),
                    &recorder.pending_line(),
                ),
                None => Ok(recordings::screen_document(
                    &session.connection,
                    &lock(&buffers.emulator).history_text(),
                )),
            }
        };
        self.save_export(&suggested, text);
    }

    /// Demande où enregistrer, puis écrit le texte exporté.
    pub(super) fn save_export(&mut self, suggested: &str, text: Result<String, RecordingError>) {
        let Some(target) = rfd::FileDialog::new()
            .set_title("Exporter la session")
            .set_file_name(suggested)
            .add_filter("Texte", &["txt"])
            .save_file()
        else {
            return;
        };
        match text.and_then(|text| recordings::write_export(&target, &text)) {
            Ok(()) => {
                tracing::info!(path = %target.display(), "session exported");
                let name = target
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.notify(format!("Session exportée : {name}"), true);
            }
            Err(error) => {
                tracing::error!(path = %target.display(), %error, "failed to export the session");
                self.notify(format!("Export impossible : {error}"), false);
            }
        }
    }

    pub(super) fn open_recordings(&mut self) {
        let key = self.vault.recording_key();
        let list = recordings::list(&ssh_manager_vault::paths::recordings_dir(), &key);
        self.recordings_dialog = Some(RecordingsDialog::new(list));
    }

    /// Enregistrements des sessions encore ouvertes.
    pub(super) fn live_recordings(&self) -> Vec<PathBuf> {
        self.sessions
            .iter()
            .filter_map(|session| match &session.kind {
                SessionKind::Terminal { buffers, .. } => {
                    lock(&buffers.recorder).path().map(Path::to_path_buf)
                }
                SessionKind::Files { .. } => None,
            })
            .collect()
    }

    pub(super) fn recordings_modal(&mut self, ctx: &egui::Context) {
        let live = self.live_recordings();
        let enabled = self.settings.record_sessions;
        let Some(dialog) = &mut self.recordings_dialog else {
            return;
        };
        match dialog.show(ctx, &live, enabled) {
            None => {}
            Some(RecordingsChoice::Close) => self.recordings_dialog = None,
            Some(RecordingsChoice::Export(recording)) => {
                let suggested = recordings::export_file_name(
                    &recording.header.connection_name,
                    recording.header.started_at,
                );
                let text = recordings::read_text(&recording.path, &self.vault.recording_key(), "");
                self.save_export(&suggested, text);
            }
            Some(RecordingsChoice::Delete(path)) => match std::fs::remove_file(&path) {
                Ok(()) => {
                    tracing::info!(path = %path.display(), "session recording deleted");
                    dialog.forget(&path);
                }
                Err(error) => {
                    tracing::error!(path = %path.display(), %error, "failed to delete a session recording");
                    self.notify(format!("Suppression impossible : {error}"), false);
                }
            },
        }
    }
}
