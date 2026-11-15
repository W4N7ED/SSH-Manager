//! Préférences de l'application. Rien de secret ici : le fichier est en clair,
//! à côté du coffre.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use ssh_manager_vault::paths;

use crate::session::terminal_theme::TerminalTheme;

/// Geste qui colle le presse-papiers dans un terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PasteMode {
    /// Clic droit colle ; Ctrl+V est transmis au shell (insertion littérale).
    #[default]
    RightClick,
    /// Ctrl+V colle ; le clic droit n'a pas d'effet.
    CtrlV,
}

impl PasteMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::RightClick => "Clic droit",
            Self::CtrlV => "Ctrl+V",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub paste_mode: PasteMode,
    /// Enregistre le texte de chaque session terminal (chiffré).
    pub record_sessions: bool,
    /// Efface le presse-papiers après la copie d'un secret.
    pub clipboard_auto_clear: bool,
    pub clipboard_clear_seconds: u32,
    pub terminal_theme: TerminalTheme,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            paste_mode: PasteMode::default(),
            record_sessions: true,
            clipboard_auto_clear: true,
            clipboard_clear_seconds: 30,
            terminal_theme: TerminalTheme::default(),
        }
    }
}

fn file() -> PathBuf {
    paths::data_dir().join("settings.json")
}

impl Settings {
    /// Préférences enregistrées, ou valeurs par défaut si le fichier manque ou
    /// est illisible (dans ce cas, l'anomalie est journalisée).
    pub fn load() -> Self {
        let path = file();
        match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|error| {
                tracing::warn!(path = %path.display(), %error, "settings file unreadable — using defaults");
                Self::default()
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "settings file unreadable — using defaults");
                Self::default()
            }
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = file();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_default_to_right_click_paste_like_the_previous_app() {
        assert_eq!(Settings::default().paste_mode, PasteMode::RightClick);
    }

    #[test]
    fn should_record_sessions_by_default_even_for_older_settings_files() {
        assert!(Settings::default().record_sessions);
        assert!(
            serde_json::from_str::<Settings>(r#"{"pasteMode":"ctrlV"}"#)
                .unwrap()
                .record_sessions
        );
    }

    #[test]
    fn should_round_trip_through_json() {
        let settings = Settings {
            paste_mode: PasteMode::CtrlV,
            record_sessions: false,
            clipboard_auto_clear: false,
            clipboard_clear_seconds: 120,
            terminal_theme: TerminalTheme::from_preset(
                TerminalTheme::preset("nord").expect("thème Nord présent"),
            ),
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&json).unwrap(), settings);
    }

    #[test]
    fn should_tolerate_unknown_and_missing_fields() {
        assert_eq!(
            serde_json::from_str::<Settings>(r#"{"futureOption":1}"#).unwrap(),
            Settings::default()
        );
    }
}
