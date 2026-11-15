//! SSH Manager — gestionnaire de connexions SSH, SFTP et FTP pour Windows.

// Pas de console derrière la fenêtre en version publiée.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod background;
mod clipboard;
mod external_terminal;
mod files;
mod recordings;
mod screens;
mod secure_copy;
mod session;
mod settings;
mod ssh_keys;
mod startup;
mod theme;
mod tls;
mod uninstall_export;
mod update_check;
mod widgets;
mod window_chrome;
mod workspace;

use std::fs::File;
use std::sync::Mutex;

use ssh_manager_vault::paths;

/// Filtre par défaut : nos évènements dès `info`, le moteur graphique
/// seulement à partir des avertissements — il détaille ses rouages internes.
const DEFAULT_LOG_FILTER: &str = "info,egui_glow=warn,glutin=warn";

/// Journal structuré (JSON, une ligne par évènement) dans le dossier de l'app.
/// Niveau réglable par la variable `SSH_MANAGER_LOG`.
fn init_logging() {
    let log_dir = paths::data_dir().join("logs");
    let writer = std::fs::create_dir_all(&log_dir)
        .and_then(|()| File::create(log_dir.join("ssh-manager.log")));
    let filter = tracing_subscriber::EnvFilter::try_from_env("SSH_MANAGER_LOG")
        .unwrap_or_else(|_| DEFAULT_LOG_FILTER.into());
    match writer {
        Ok(file) => tracing_subscriber::fmt()
            .json()
            .with_env_filter(filter)
            .with_writer(Mutex::new(file))
            .init(),
        // Sans fichier de journal, on journalise au moins sur la sortie d'erreur.
        Err(error) => {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_writer(std::io::stderr)
                .init();
            tracing::warn!(dir = %log_dir.display(), %error, "log file unavailable — logging to stderr");
        }
    }
}

/// Icône de la fenêtre et de la barre des tâches (l'exécutable a la sienne,
/// intégrée par `build.rs`).
fn window_icon() -> Option<egui::IconData> {
    eframe::icon_data::from_png_bytes(include_bytes!("../../../assets/icons/icon.png"))
        .inspect_err(|error| tracing::warn!(%error, "window icon could not be decoded"))
        .ok()
}

fn main() -> eframe::Result {
    init_logging();
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        portable = paths::is_portable(),
        data_dir = %paths::data_dir().display(),
        "application started"
    );

    if std::env::args().any(|argument| argument == uninstall_export::FLAG) {
        uninstall_export::run();
        return Ok(());
    }

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("SSH Manager")
        .with_inner_size([1400.0, 900.0])
        .with_min_inner_size([960.0, 600.0])
        // Barre de titre native retirée : l'app dessine la sienne (window_chrome).
        .with_decorations(false);
    if let Some(icon) = window_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    eframe::run_native(
        "SSH Manager",
        options,
        Box::new(|creation| Ok(Box::new(app::SshManagerApp::new(creation)))),
    )
}
