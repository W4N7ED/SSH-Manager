//! L'application : un automate d'écrans, du premier lancement à l'espace de travail.

use egui::{RichText, Ui};

use crate::screens::setup::{SetupOutcome, SetupScreen};
use crate::screens::unlock::{UnlockOutcome, UnlockScreen};
use crate::startup::{self, Start};
use crate::theme::{self, color};
use crate::workspace::Workspace;

enum Screen {
    Setup(SetupScreen),
    Unlock(UnlockScreen),
    Workspace(Box<Workspace>),
    Failed(String),
}

pub struct SshManagerApp {
    screen: Screen,
}

impl SshManagerApp {
    pub fn new(creation: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&creation.egui_ctx);
        let screen = match startup::determine() {
            Start::Ready {
                vault,
                data,
                notice,
            } => Screen::Workspace(Box::new(Workspace::new(vault, data, notice))),
            Start::NeedsPhrase(purpose) => Screen::Unlock(UnlockScreen::new(purpose)),
            Start::NeedsSetup { imported } => Screen::Setup(SetupScreen::new(imported)),
            Start::Failed(error) => Screen::Failed(error),
        };
        Self { screen }
    }
}

impl eframe::App for SshManagerApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let next = match &mut self.screen {
            Screen::Setup(setup) => match setup.ui(ui) {
                SetupOutcome::Created { vault, data } => Some(Screen::Workspace(Box::new(
                    Workspace::new(vault, data, Some("Coffre créé.".into())),
                ))),
                SetupOutcome::Pending => None,
            },
            Screen::Unlock(unlock) => match unlock.ui(ui) {
                UnlockOutcome::Unlocked {
                    vault,
                    data,
                    notice,
                } => Some(Screen::Workspace(Box::new(Workspace::new(
                    vault, data, notice,
                )))),
                UnlockOutcome::Pending => None,
            },
            Screen::Workspace(workspace) => {
                workspace.ui(ui);
                None
            }
            Screen::Failed(error) => {
                failure(ui, error);
                None
            }
        };
        if let Some(next) = next {
            self.screen = next;
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        color::BASE.to_normalized_gamma_f32()
    }
}

fn failure(ui: &mut Ui, error: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(120.0);
        ui.label(RichText::new("Le coffre ne peut pas être ouvert").font(theme::sans_bold(22.0)));
        ui.add_space(10.0);
        ui.label(RichText::new(error).font(theme::mono(12.0)).color(color::TEXT_MUTED));
        ui.add_space(10.0);
        ui.label(RichText::new("Aucune donnée n'a été modifiée. Le détail figure dans le journal de l'application.").color(color::TEXT_DIM));
    });
}
