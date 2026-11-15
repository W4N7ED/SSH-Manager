//! Formulaire de création et de modification d'une connexion.

use egui::{CornerRadius, Frame, Margin, RichText, Ui};
use ssh_manager_core::{Connection, ConnectionKind};

use super::form_network::{self, TunnelDraft};
use super::key_generator::{KeyGenerator, KeyGeneratorEvent};
use crate::secure_copy::SecureClipboard;
use crate::settings::Settings;
use crate::theme::{self, color, radius};
use crate::widgets;

const NO_GROUP: &str = "Sans groupe";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Authentication {
    PrivateKey,
    Password,
}

pub enum FormResult {
    Saved(Box<Connection>),
    Cancelled,
}

pub struct ConnectionForm {
    connection: Connection,
    editing: bool,
    port_text: String,
    password_text: String,
    passphrase_text: String,
    authentication: Authentication,
    new_group: String,
    errors: Vec<&'static str>,
    key_generator: Option<KeyGenerator>,
    tunnels: Vec<TunnelDraft>,
}

impl ConnectionForm {
    pub fn new(connection: Connection, editing: bool) -> Self {
        let authentication = if connection.private_key_path.is_some() {
            Authentication::PrivateKey
        } else {
            Authentication::Password
        };
        Self {
            port_text: connection.port.to_string(),
            password_text: connection.password.clone().unwrap_or_default(),
            passphrase_text: connection.key_passphrase.clone().unwrap_or_default(),
            authentication,
            new_group: String::new(),
            errors: Vec::new(),
            key_generator: None,
            tunnels: connection
                .local_forwards
                .iter()
                .map(TunnelDraft::from_forward)
                .collect(),
            connection,
            editing,
        }
    }

    /// Affiche le formulaire ; renvoie un résultat quand il se ferme.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        groups: &[String],
        connections: &[Connection],
        clipboard: &mut SecureClipboard,
        settings: &Settings,
    ) -> Option<FormResult> {
        let mut result = None;
        let modal = egui::Modal::new(egui::Id::unique("connection-form"))
            .backdrop_color(color::tint(egui::Color32::BLACK, 150))
            .frame(
                Frame::new()
                    .fill(color::BASE)
                    .stroke(egui::Stroke::new(1.0, color::BORDER))
                    .corner_radius(CornerRadius::same(radius::LARGE))
                    .inner_margin(Margin::same(22)),
            )
            .show(ctx, |ui| {
                ui.set_width(520.0);
                result = self.body(ui, groups, connections, clipboard, settings);
            });
        if modal.should_close() && result.is_none() {
            result = Some(FormResult::Cancelled);
        }
        result
    }

    fn body(
        &mut self,
        ui: &mut Ui,
        groups: &[String],
        connections: &[Connection],
        clipboard: &mut SecureClipboard,
        settings: &Settings,
    ) -> Option<FormResult> {
        let title = if self.editing {
            "Modifier la connexion"
        } else {
            "Nouvelle connexion"
        };
        ui.label(RichText::new(title).font(theme::sans_bold(21.0)));
        ui.add_space(14.0);

        widgets::micro_label(ui, "Type");
        let previous_kind = self.connection.kind;
        let kinds = ConnectionKind::ALL.map(|kind| (kind, kind.label()));
        if widgets::segmented(ui, &kinds, &mut self.connection.kind) {
            // Le port suit le protocole tant qu'il n'a pas été personnalisé.
            if self.port_text == previous_kind.default_port().to_string() {
                self.port_text = self.connection.kind.default_port().to_string();
            }
            if self.connection.kind == ConnectionKind::Ftp {
                self.authentication = Authentication::Password;
            }
        }
        ui.add_space(10.0);

        field(ui, "Nom de la connexion", |ui| {
            ui.add(
                widgets::text_field(&mut self.connection.name, "prod-web-01")
                    .desired_width(f32::INFINITY),
            )
        });
        ui.columns(2, |columns| {
            field(&mut columns[0], "Hôte", |ui| {
                ui.add(
                    widgets::text_field(&mut self.connection.host, "10.20.0.13")
                        .font(theme::mono(13.0))
                        .desired_width(f32::INFINITY),
                )
            });
            field(&mut columns[1], "Port", |ui| {
                ui.add(
                    widgets::text_field(&mut self.port_text, "22")
                        .font(theme::mono(13.0))
                        .desired_width(f32::INFINITY),
                )
            });
        });
        ui.columns(2, |columns| {
            field(&mut columns[0], "Utilisateur", |ui| {
                ui.add(
                    widgets::text_field(&mut self.connection.username, "root")
                        .font(theme::mono(13.0))
                        .desired_width(f32::INFINITY),
                )
            });
            field(&mut columns[1], "Groupe", |ui| {
                self.group_picker(ui, groups)
            });
        });

        self.authentication_fields(ui, clipboard, settings);
        ui.add_space(4.0);
        form_network::show(ui, &mut self.connection, connections, &mut self.tunnels);
        ui.checkbox(&mut self.connection.favorite, "Ajouter aux favoris");

        for error in &self.errors {
            ui.label(RichText::new(*error).color(color::DANGER));
        }
        ui.add_space(14.0);
        self.footer(ui)
    }

    fn group_picker(&mut self, ui: &mut Ui, groups: &[String]) -> egui::Response {
        let current = self
            .connection
            .group
            .clone()
            .unwrap_or_else(|| NO_GROUP.to_owned());
        let response = egui::ComboBox::from_id_salt("form-group")
            .selected_text(current)
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.connection.group, None, NO_GROUP);
                for group in groups {
                    ui.selectable_value(&mut self.connection.group, Some(group.clone()), group);
                }
            })
            .response;
        ui.horizontal(|ui| {
            ui.add(
                widgets::text_field(&mut self.new_group, "ou nouveau groupe…")
                    .desired_width(ui.available_width()),
            );
        });
        response
    }

    fn authentication_fields(
        &mut self,
        ui: &mut Ui,
        clipboard: &mut SecureClipboard,
        settings: &Settings,
    ) {
        if self.connection.kind == ConnectionKind::Ftp {
            field(ui, "Mot de passe", |ui| {
                ui.add(
                    widgets::text_field(&mut self.password_text, "laisser vide pour le demander")
                        .password(true)
                        .desired_width(f32::INFINITY),
                )
            });
            ui.checkbox(&mut self.connection.ftp_secure, "FTPS (connexion chiffrée)");
            if !self.connection.ftp_secure {
                ui.label(RichText::new("⚠ Le FTP simple transmet identifiant et mot de passe en clair sur le réseau.").font(theme::sans(12.0)).color(color::WARNING));
            }
            return;
        }

        widgets::micro_label(ui, "Authentification");
        widgets::segmented(
            ui,
            &[
                (Authentication::PrivateKey, "Clé privée"),
                (Authentication::Password, "Mot de passe"),
            ],
            &mut self.authentication,
        );
        ui.add_space(6.0);
        match self.authentication {
            Authentication::Password => {
                ui.add(
                    widgets::text_field(
                        &mut self.password_text,
                        "laisser vide pour le demander à chaque connexion",
                    )
                    .password(true)
                    .desired_width(f32::INFINITY),
                );
            }
            Authentication::PrivateKey => {
                ui.horizontal(|ui| {
                    let mut path = self.connection.private_key_path.clone().unwrap_or_default();
                    ui.add_enabled(
                        false,
                        widgets::text_field(&mut path, "C:\\Users\\vous\\.ssh\\id_ed25519")
                            .font(theme::mono(12.0))
                            .desired_width(ui.available_width() - 110.0),
                    );
                    if widgets::secondary_button(ui, "Parcourir…").clicked() {
                        let picked = rfd::FileDialog::new()
                            .set_title("Choisir une clé privée")
                            .set_directory(dirs::home_dir().unwrap_or_default().join(".ssh"))
                            .pick_file();
                        if let Some(path) = picked {
                            self.connection.private_key_path = Some(path.display().to_string());
                        }
                    }
                });
                ui.add_space(6.0);
                ui.add(
                    widgets::text_field(
                        &mut self.passphrase_text,
                        "phrase de passe de la clé (si chiffrée — vide : demandée à l'ouverture)",
                    )
                    .password(true)
                    .desired_width(f32::INFINITY),
                );
                self.key_generation(ui, clipboard, settings);
            }
        }
    }

    fn key_generation(
        &mut self,
        ui: &mut Ui,
        clipboard: &mut SecureClipboard,
        settings: &Settings,
    ) {
        ui.add_space(6.0);
        let Some(generator) = &mut self.key_generator else {
            if widgets::secondary_button(ui, "+ Créer une nouvelle clé").clicked() {
                let label = if self.connection.name.trim().is_empty() {
                    self.connection.host.trim()
                } else {
                    self.connection.name.trim()
                };
                self.key_generator = Some(KeyGenerator::new(label));
            }
            return;
        };
        match generator.show(ui, clipboard, settings) {
            Some(KeyGeneratorEvent::Generated(path)) => {
                self.connection.private_key_path = Some(path.display().to_string());
            }
            Some(KeyGeneratorEvent::Closed) => self.key_generator = None,
            None => {}
        }
    }

    fn footer(&mut self, ui: &mut Ui) -> Option<FormResult> {
        let mut result = None;
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Ctrl+Entrée pour enregistrer")
                    .font(theme::mono(11.0))
                    .color(color::TEXT_DIM),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = if self.editing {
                    "Enregistrer"
                } else {
                    "Créer"
                };
                let submit = widgets::primary_button(ui, label).clicked()
                    || ui
                        .input(|input| input.modifiers.ctrl && input.key_pressed(egui::Key::Enter));
                if widgets::secondary_button(ui, "Annuler").clicked() {
                    result = Some(FormResult::Cancelled);
                }
                if submit {
                    result = self
                        .submit()
                        .map(|connection| FormResult::Saved(Box::new(connection)));
                }
            });
        });
        result
    }

    /// Construit la connexion finale, ou affiche les erreurs et renvoie `None`.
    fn submit(&mut self) -> Option<Connection> {
        let mut connection = self.connection.clone();
        connection.name = connection.name.trim().to_owned();
        connection.host = connection.host.trim().to_owned();
        connection.username = connection.username.trim().to_owned();
        connection.port = self.port_text.trim().parse().unwrap_or(0);
        if !self.new_group.trim().is_empty() {
            connection.group = Some(self.new_group.trim().to_owned());
        }
        connection.password = match self.authentication {
            Authentication::Password => {
                Some(self.password_text.clone()).filter(|password| !password.is_empty())
            }
            Authentication::PrivateKey => None,
        };
        connection.key_passphrase = match self.authentication {
            Authentication::PrivateKey => {
                Some(self.passphrase_text.clone()).filter(|passphrase| !passphrase.is_empty())
            }
            Authentication::Password => None,
        };
        if self.authentication == Authentication::Password || connection.kind == ConnectionKind::Ftp
        {
            connection.private_key_path = None;
        }
        if connection.kind != ConnectionKind::Ftp {
            connection.ftp_secure = false;
        }
        let tunnels = form_network::parse_tunnels(&self.tunnels);
        connection.local_forwards = match (&tunnels, connection.kind) {
            (Ok(tunnels), ConnectionKind::Ssh) => tunnels.clone(),
            _ => Vec::new(),
        };
        if connection.kind == ConnectionKind::Ftp {
            connection.jump_connection_id = None;
        }

        self.errors = connection
            .validate()
            .into_iter()
            .map(|error| error.message())
            .collect();
        if let Err(error) = tunnels {
            self.errors.push(error);
        }
        if self.authentication == Authentication::PrivateKey
            && connection.kind != ConnectionKind::Ftp
            && connection.private_key_path.is_none()
        {
            self.errors.push("Sélectionnez une clé privée");
        }
        self.errors.is_empty().then_some(connection)
    }
}

fn field(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui) -> egui::Response) {
    widgets::micro_label(ui, label);
    add(ui);
    ui.add_space(8.0);
}
