//! Saisie de la phrase de récupération : coffre déplacé sur un autre poste, ou
//! migration de l'ancienne application impossible sans elle.

use egui::{Align, CornerRadius, Frame, Layout, Margin, RichText, Ui};
use ssh_manager_vault::{Vault, VaultData, VaultError, paths};

use crate::background::BackgroundTask;
use crate::screens::setup::{paragraph, title};
use crate::startup::{self, PhrasePurpose, Start};
use crate::theme::{self, color, radius};
use crate::widgets;

type UnlockResult = Result<(Vault, VaultData, Option<String>), String>;

pub struct UnlockScreen {
    purpose: PhrasePurpose,
    phrase: String,
    error: Option<String>,
    task: Option<BackgroundTask<UnlockResult>>,
}

#[expect(
    clippy::large_enum_variant,
    reason = "valeur de retour éphémère, jamais stockée"
)]
pub enum UnlockOutcome {
    Pending,
    Unlocked {
        vault: Vault,
        data: VaultData,
        notice: Option<String>,
    },
}

impl UnlockScreen {
    pub fn new(purpose: PhrasePurpose) -> Self {
        Self {
            purpose,
            phrase: String::new(),
            error: None,
            task: None,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui) -> UnlockOutcome {
        if let Some(result) = self.task.as_ref().and_then(BackgroundTask::poll) {
            self.task = None;
            match result {
                Ok((vault, data, notice)) => {
                    return UnlockOutcome::Unlocked {
                        vault,
                        data,
                        notice,
                    };
                }
                Err(error) => self.error = Some(error),
            }
        }

        ui.with_layout(Layout::top_down(Align::Center), |ui| {
            ui.add_space((ui.available_height() - 360.0).max(24.0) / 2.0);
            Frame::new()
                .fill(color::SURFACE)
                .corner_radius(CornerRadius::same(radius::LARGE))
                .inner_margin(Margin::same(32))
                .show(ui, |ui| {
                    ui.set_width(460.0);
                    ui.with_layout(Layout::top_down(Align::Min), |ui| self.form(ui));
                });
        });
        UnlockOutcome::Pending
    }

    fn form(&mut self, ui: &mut Ui) {
        let (heading, explanation) = match self.purpose {
            PhrasePurpose::UnlockVault => (
                "Déverrouiller le coffre",
                "Ce coffre n'a encore jamais été ouvert sur ce poste (ou ce compte Windows). Saisissez votre phrase de récupération : les lancements suivants y seront automatiques.",
            ),
            PhrasePurpose::ImportLegacy(_) => (
                "Importer vos connexions",
                "L'ancien coffre n'a pas pu être ouvert automatiquement. Saisissez la phrase de récupération de l'ancienne application : elle restera valable pour ce coffre.",
            ),
        };
        title(ui, heading);
        paragraph(ui, explanation);
        ui.add_space(10.0);

        widgets::micro_label(ui, "Phrase de récupération");
        let busy = self.task.is_some();
        let field = ui.add_enabled(
            !busy,
            widgets::text_field(&mut self.phrase, "mot1 mot2 mot3 … mot12")
                .font(theme::mono(13.0))
                .desired_width(f32::INFINITY),
        );
        let submitted = field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));

        if let Some(error) = &self.error {
            ui.add_space(6.0);
            ui.label(RichText::new(error.as_str()).color(color::DANGER));
        }
        ui.add_space(14.0);

        ui.horizontal(|ui| {
            let label = if busy {
                "Vérification…"
            } else {
                "Continuer"
            };
            let clicked =
                widgets::primary_button_enabled(ui, !busy && !self.phrase.trim().is_empty(), label)
                    .clicked();
            if busy {
                ui.spinner();
            }
            if (clicked || submitted) && !busy && !self.phrase.trim().is_empty() {
                self.start(ui.ctx());
            }
        });
    }

    fn start(&mut self, ctx: &egui::Context) {
        self.error = None;
        let phrase = self.phrase.trim().to_owned();
        let task = match &self.purpose {
            PhrasePurpose::UnlockVault => BackgroundTask::spawn(ctx, move || {
                Vault::open_with_phrase(paths::vault_file(), &phrase)
                    .map(|(vault, data)| (vault, data, None))
                    .map_err(describe)
            }),
            PhrasePurpose::ImportLegacy(legacy) => {
                // Le coffre est relocalisé sur le fil dédié à partir de son
                // dossier : la dérivation de clé y est coûteuse (PBKDF2).
                let dir = legacy.dir().to_path_buf();
                BackgroundTask::spawn(ctx, move || {
                    let legacy = ssh_manager_vault::legacy::LegacyVault::locate(dir)
                        .ok_or_else(|| "L'ancien coffre n'est plus lisible.".to_owned())?;
                    let key = legacy.recover_key_with_phrase(&phrase).map_err(describe)?;
                    let import = legacy.read(Some(key)).map_err(describe)?;
                    match startup::migrate(import).map_err(describe)? {
                        Start::Ready {
                            vault,
                            data,
                            notice,
                        } => Ok((vault, data, notice)),
                        _ => Err("La migration n'a pas abouti.".to_owned()),
                    }
                })
            }
        };
        self.task = Some(task);
    }
}

fn describe(error: VaultError) -> String {
    match error {
        VaultError::WrongRecoveryPhrase => "Phrase de récupération incorrecte.".to_owned(),
        other => other.to_string(),
    }
}
