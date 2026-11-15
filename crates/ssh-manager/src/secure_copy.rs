//! Copie des secrets (mots de passe générés, clés publiques) : écriture privée
//! hors de l'historique Windows, puis effacement différé.
//!
//! Le minuteur est vérifié à chaque image et l'interface est réveillée à
//! l'échéance : l'effacement a lieu même si la fenêtre est en arrière-plan,
//! c'est justement le moment où l'utilisateur est parti coller ailleurs.

use std::time::{Duration, Instant};

use zeroize::Zeroizing;

use crate::clipboard;
use crate::settings::Settings;

/// Délais proposés dans la boîte à outils, en secondes.
pub const CLEAR_DELAY_OPTIONS: [u32; 6] = [10, 20, 30, 60, 120, 300];

/// « 30 s », « 2 min ».
pub fn format_delay(seconds: u32) -> String {
    if seconds < 60 {
        format!("{seconds} s")
    } else {
        format!("{} min", seconds / 60)
    }
}

/// Ce qui a été fait, pour l'annoncer à l'utilisateur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopyOutcome {
    /// Délai avant effacement, si l'effacement est activé.
    pub clear_after_seconds: Option<u32>,
    /// Faux si l'écriture privée a échoué et qu'une copie ordinaire l'a remplacée.
    pub kept_out_of_history: bool,
}

impl CopyOutcome {
    /// « ✓ Copié · effacé dans 30 s », complété si Win+V n'a pas pu être évité.
    /// `feminine` accorde le message (« la clé publique » : copiée, effacée).
    pub fn describe(self, feminine: bool) -> String {
        let ending = if feminine { "e" } else { "" };
        let mut text = format!("✓ Copié{ending}");
        if let Some(seconds) = self.clear_after_seconds {
            text.push_str(&format!(" · effacé{ending} dans {}", format_delay(seconds)));
        }
        if !self.kept_out_of_history {
            text.push_str(" · historique Windows non exclu");
        }
        text
    }
}

struct PendingClear {
    text: Zeroizing<String>,
    deadline: Instant,
}

#[derive(Default)]
pub struct SecureClipboard {
    pending: Option<PendingClear>,
}

impl SecureClipboard {
    /// Copie `text` selon les réglages de presse-papiers.
    pub fn copy(&mut self, ctx: &egui::Context, text: &str, settings: &Settings) -> CopyOutcome {
        self.pending = None;
        let kept_out_of_history = match clipboard::write_private(text) {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!(%error, "private clipboard write failed — falling back to a plain copy");
                ctx.copy_text(text.to_owned());
                false
            }
        };
        let clear_after_seconds = settings
            .clipboard_auto_clear
            .then_some(settings.clipboard_clear_seconds);
        if let Some(seconds) = clear_after_seconds {
            let delay = Duration::from_secs(u64::from(seconds));
            self.pending = Some(PendingClear {
                text: Zeroizing::new(text.to_owned()),
                deadline: Instant::now() + delay,
            });
            ctx.request_repaint_after(delay);
        }
        CopyOutcome {
            clear_after_seconds,
            kept_out_of_history,
        }
    }

    /// À appeler à chaque image : efface à l'échéance.
    pub fn tick(&mut self, ctx: &egui::Context) {
        let Some(pending) = &self.pending else { return };
        let now = Instant::now();
        if now < pending.deadline {
            ctx.request_repaint_after(pending.deadline - now);
            return;
        }
        self.clear_now();
    }

    /// Efface tout de suite le secret en attente, s'il est encore là.
    fn clear_now(&mut self) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        match clipboard::clear_if_unchanged(&pending.text) {
            Ok(true) => tracing::info!("clipboard cleared after the configured delay"),
            Ok(false) => {
                tracing::info!("clipboard left untouched — its content changed since the copy")
            }
            Err(error) => tracing::error!(%error, "failed to clear the clipboard"),
        }
    }
}

impl Drop for SecureClipboard {
    /// Quitter avant l'échéance laisserait le secret dans le presse-papiers.
    fn drop(&mut self) {
        self.clear_now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_format_delays_in_seconds_then_minutes() {
        assert_eq!(format_delay(30), "30 s");
        assert_eq!(format_delay(120), "2 min");
    }

    #[test]
    fn should_describe_a_private_copy_with_its_clearing_delay() {
        let outcome = CopyOutcome {
            clear_after_seconds: Some(20),
            kept_out_of_history: true,
        };
        assert_eq!(outcome.describe(false), "✓ Copié · effacé dans 20 s");
        assert_eq!(outcome.describe(true), "✓ Copiée · effacée dans 20 s");
    }

    #[test]
    fn should_warn_when_the_copy_could_not_be_kept_out_of_history() {
        let outcome = CopyOutcome {
            clear_after_seconds: None,
            kept_out_of_history: false,
        };
        assert_eq!(
            outcome.describe(false),
            "✓ Copié · historique Windows non exclu"
        );
    }
}
