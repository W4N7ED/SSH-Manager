//! Travail long hors du fil de l'interface (dérivation de clé, génération de
//! clé RSA…), dont le résultat est relevé à l'image suivante.

use std::sync::mpsc::{Receiver, channel};

pub struct BackgroundTask<T> {
    receiver: Receiver<T>,
}

impl<T: Send + 'static> BackgroundTask<T> {
    /// Lance `work` sur un fil dédié et redemande un affichage à la fin.
    pub fn spawn(ctx: &egui::Context, work: impl FnOnce() -> T + Send + 'static) -> Self {
        let (sender, receiver) = channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            // L'envoi n'échoue que si l'écran a été abandonné entre-temps.
            let _ = sender.send(work());
            ctx.request_repaint();
        });
        Self { receiver }
    }

    /// Résultat s'il est prêt. Une tâche dont le fil a paniqué ne renvoie jamais rien.
    pub fn poll(&self) -> Option<T> {
        self.receiver.try_recv().ok()
    }
}
