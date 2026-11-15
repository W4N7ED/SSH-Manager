//! Premier lancement : création de la phrase de récupération et du coffre.
//!
//! Trois étapes : présentation, affichage des 12 mots, puis vérification de
//! trois mots tirés au hasard — assez pour s'assurer qu'ils ont été notés,
//! sans imposer de retaper la phrase entière.

use egui::{Align, CornerRadius, Frame, Layout, Margin, RichText, Ui, Vec2};
use rand::seq::IndexedRandom;
use ssh_manager_vault::{Vault, VaultData, paths, recovery};

use crate::background::BackgroundTask;
use crate::theme::{self, color, radius};
use crate::widgets;

const CHECKED_WORD_COUNT: usize = 3;

enum Step {
    Welcome,
    ShowPhrase {
        noted: bool,
    },
    Verify {
        positions: Vec<usize>,
        answers: Vec<String>,
        error: Option<String>,
    },
    Creating(BackgroundTask<Result<Vault, String>>),
}

pub struct SetupScreen {
    phrase: String,
    imported: Option<VaultData>,
    step: Step,
}

#[expect(
    clippy::large_enum_variant,
    reason = "valeur de retour éphémère, jamais stockée"
)]
pub enum SetupOutcome {
    Pending,
    Created { vault: Vault, data: VaultData },
}

impl SetupScreen {
    pub fn new(imported: Option<VaultData>) -> Self {
        Self {
            phrase: recovery::generate_phrase(),
            imported,
            step: Step::Welcome,
        }
    }

    fn words(&self) -> Vec<&str> {
        self.phrase.split_whitespace().collect()
    }

    pub fn ui(&mut self, ui: &mut Ui) -> SetupOutcome {
        let mut outcome = SetupOutcome::Pending;
        ui.with_layout(Layout::top_down(Align::Center), |ui| {
            ui.add_space((ui.available_height() - 560.0).max(24.0) / 2.0);
            Frame::new()
                .fill(color::SURFACE)
                .corner_radius(CornerRadius::same(radius::LARGE))
                .inner_margin(Margin::same(32))
                .show(ui, |ui| {
                    ui.set_width(460.0);
                    ui.with_layout(Layout::top_down(Align::Min), |ui| {
                        self.header(ui);
                        outcome = self.step_ui(ui);
                    });
                });
        });
        outcome
    }

    fn header(&self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            widgets::brand_mark(ui, 16.0);
            ui.label(RichText::new("SSH Manager").font(theme::sans_semibold(13.0)));
        });
        ui.add_space(18.0);
        let reached = match self.step {
            Step::Welcome => 1,
            Step::ShowPhrase { .. } => 2,
            Step::Verify { .. } | Step::Creating(_) => 3,
        };
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let width = (ui.available_width() - 12.0) / 3.0;
            for index in 1..=3 {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 4.0), egui::Sense::hover());
                let fill = if index <= reached {
                    color::ACCENT
                } else {
                    color::ELEVATED
                };
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(radius::PILL), fill);
            }
        });
        ui.add_space(22.0);
    }

    fn step_ui(&mut self, ui: &mut Ui) -> SetupOutcome {
        match &mut self.step {
            Step::Welcome => {
                title(ui, "Bienvenue dans SSH Manager");
                paragraph(
                    ui,
                    "Vos connexions — hôtes, mots de passe, chemins de clés — sont chiffrées en AES-256 dans un coffre local.",
                );
                paragraph(
                    ui,
                    "Le coffre est protégé par une phrase de récupération de 12 mots. Elle est la seule façon de le rouvrir sur un autre poste.",
                );
                if let Some(folder) = ssh_manager_vault::paths::portable_data_dir() {
                    paragraph(
                        ui,
                        &format!(
                            "Version portable : les données restent à côté de l'application, dans « {} ». Sur un autre PC, la phrase est demandée une seule fois, puis l'ouverture y devient automatique.",
                            folder.display()
                        ),
                    );
                }
                if self
                    .imported
                    .as_ref()
                    .is_some_and(|data| !data.library.connections.is_empty())
                {
                    paragraph(
                        ui,
                        "Les connexions de l'ancienne application, qui n'étaient pas chiffrées, seront reprises dans ce coffre.",
                    );
                }
                ui.add_space(16.0);
                if widgets::primary_button(ui, "Générer ma phrase de récupération").clicked() {
                    self.step = Step::ShowPhrase { noted: false };
                }
            }
            Step::ShowPhrase { noted } => {
                title(ui, "Notez votre phrase de récupération");
                paragraph(
                    ui,
                    "Ces 12 mots chiffrent votre coffre. Sans eux, aucune restauration n'est possible.",
                );
                ui.add_space(8.0);
                word_grid(ui, &self.phrase);
                ui.add_space(14.0);
                ui.checkbox(noted, "J'ai noté ces 12 mots en lieu sûr");
                ui.add_space(14.0);
                if widgets::primary_button_enabled(ui, *noted, "Continuer").clicked() {
                    let mut rng = rand::rng();
                    let mut positions: Vec<usize> = (0..12)
                        .collect::<Vec<_>>()
                        .sample(&mut rng, CHECKED_WORD_COUNT)
                        .copied()
                        .collect();
                    positions.sort_unstable();
                    self.step = Step::Verify {
                        positions,
                        answers: vec![String::new(); CHECKED_WORD_COUNT],
                        error: None,
                    };
                }
            }
            Step::Verify { .. } => return self.verify_ui(ui),
            Step::Creating(task) => {
                title(ui, "Création du coffre…");
                ui.spinner();
                if let Some(result) = task.poll() {
                    match result {
                        Ok(vault) => {
                            return SetupOutcome::Created {
                                vault,
                                data: self.imported.take().unwrap_or_default(),
                            };
                        }
                        Err(error) => {
                            self.step = Step::Verify {
                                positions: vec![],
                                answers: vec![],
                                error: Some(error),
                            };
                        }
                    }
                }
            }
        }
        SetupOutcome::Pending
    }

    fn verify_ui(&mut self, ui: &mut Ui) -> SetupOutcome {
        let words: Vec<String> = self.words().iter().map(|word| word.to_string()).collect();
        let Step::Verify {
            positions,
            answers,
            error,
        } = &mut self.step
        else {
            return SetupOutcome::Pending;
        };

        title(ui, "Vérification");
        paragraph(
            ui,
            "Saisissez les mots demandés pour confirmer que la phrase est bien notée.",
        );
        ui.add_space(8.0);
        for (index, position) in positions.iter().enumerate() {
            widgets::micro_label(ui, &format!("Mot n° {}", position + 1));
            ui.add(
                widgets::text_field(&mut answers[index], "")
                    .font(theme::mono(13.0))
                    .desired_width(f32::INFINITY),
            );
            ui.add_space(4.0);
        }
        if let Some(error) = error {
            ui.label(RichText::new(error.as_str()).color(color::DANGER));
        }
        ui.add_space(12.0);

        let mut go_back = false;
        let mut confirmed = false;
        ui.horizontal(|ui| {
            go_back = widgets::secondary_button(ui, "Revoir les mots").clicked();
            confirmed = widgets::primary_button(ui, "Créer le coffre").clicked();
        });

        if go_back {
            self.step = Step::ShowPhrase { noted: true };
        } else if confirmed {
            let all_match = positions
                .iter()
                .zip(answers.iter())
                .all(|(position, answer)| answer.trim().eq_ignore_ascii_case(&words[*position]));
            if all_match {
                let phrase = self.phrase.clone();
                let imported = self.imported.clone().unwrap_or_default();
                self.step = Step::Creating(BackgroundTask::spawn(ui.ctx(), move || {
                    let vault = Vault::create(paths::vault_file(), &phrase);
                    vault
                        .save(&imported)
                        .map(|()| vault)
                        .map_err(|error| error.to_string())
                }));
            } else {
                *error = Some("Un ou plusieurs mots ne correspondent pas.".into());
            }
        }
        SetupOutcome::Pending
    }
}

pub fn title(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).font(theme::sans_bold(26.0)));
    ui.add_space(6.0);
}

pub fn paragraph(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text)
            .font(theme::sans(13.0))
            .color(color::TEXT_MUTED),
    );
    ui.add_space(4.0);
}

fn word_grid(ui: &mut Ui, phrase: &str) {
    egui::Grid::new("recovery-words")
        .num_columns(3)
        .spacing(Vec2::new(8.0, 8.0))
        .show(ui, |ui| {
            for (index, word) in phrase.split_whitespace().enumerate() {
                Frame::new()
                    .fill(color::BASE)
                    .corner_radius(CornerRadius::same(radius::SMALL))
                    .inner_margin(Margin::symmetric(12, 10))
                    .show(ui, |ui| {
                        ui.set_width(118.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{}", index + 1))
                                    .font(theme::mono(11.0))
                                    .color(color::TEXT_DIM),
                            );
                            ui.label(RichText::new(word).font(theme::mono(13.0)));
                        });
                    });
                if index % 3 == 2 {
                    ui.end_row();
                }
            }
        });
}
