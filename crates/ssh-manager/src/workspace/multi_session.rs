//! Plusieurs sessions à la fois : mosaïque et saisie synchronisée.

use super::*;

impl Workspace {
    /// Rejoue dans les autres sessions synchronisées ce qui a été tapé dans
    /// l'une d'elles. Seules les sessions connectées reçoivent la saisie.
    pub(super) fn replay_synced_input(&self, typed: Vec<TypedInput>) {
        for input in typed {
            for target in mosaic::replicas(&self.synced, input.source) {
                let Some(session) = self.sessions.iter().find(|session| session.id == target)
                else {
                    continue;
                };
                if let SessionKind::Terminal { ssh, .. } = &session.kind
                    && session.state().is_live()
                {
                    ssh.send(input.bytes.clone());
                }
            }
        }
    }

    pub(super) fn toggle_mosaic(&mut self) {
        if self.mosaic.is_empty() {
            self.mosaic
                .fill(self.sessions.iter().map(|session| session.id));
            if !self.mosaic.is_empty() {
                self.active = ActiveTab::Mosaic;
            } else {
                self.notify("Ouvrez au moins une session pour la mosaïque.", false);
            }
        } else {
            self.mosaic.clear();
            if self.active == ActiveTab::Mosaic {
                self.active = self
                    .sessions
                    .first()
                    .map_or(ActiveTab::Home, |session| ActiveTab::Session(session.id));
            }
        }
    }

    pub(super) fn toggle_sync(&mut self, id: u64) {
        let is_terminal = self
            .sessions
            .iter()
            .any(|session| session.id == id && session.is_terminal());
        if !is_terminal {
            return;
        }
        if !self.synced.remove(&id) {
            self.synced.insert(id);
        }
        tracing::info!(
            session = id,
            synced = self.synced.len(),
            "synchronized input changed"
        );
        if self.synced.len() == 1 {
            self.notify(
                "Synchronisez au moins une autre session pour répliquer la saisie.",
                true,
            );
        }
    }
}
