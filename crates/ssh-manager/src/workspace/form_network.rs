//! Section « Réseau » du formulaire : serveur de rebond et tunnels locaux.

use egui::{RichText, Ui};
use ssh_manager_core::{Connection, ConnectionKind, LocalForward};

use crate::theme::{self, color};
use crate::widgets;

/// Une ligne de tunnel en cours de saisie (texte libre, vérifié à l'enregistrement).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TunnelDraft {
    pub local_port: String,
    pub remote_host: String,
    pub remote_port: String,
}

impl TunnelDraft {
    pub fn from_forward(forward: &LocalForward) -> Self {
        Self {
            local_port: forward.local_port.to_string(),
            remote_host: forward.remote_host.clone(),
            remote_port: forward.remote_port.to_string(),
        }
    }

    fn is_blank(&self) -> bool {
        self.local_port.trim().is_empty()
            && self.remote_host.trim().is_empty()
            && self.remote_port.trim().is_empty()
    }

    /// Le tunnel saisi, ou `None` si un champ est vide ou invalide.
    pub fn parse(&self) -> Option<LocalForward> {
        let host = self.remote_host.trim();
        let valid_host = !host.is_empty()
            && host
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || ".-_:".contains(character));
        Some(LocalForward {
            local_port: self
                .local_port
                .trim()
                .parse()
                .ok()
                .filter(|port| *port > 0)?,
            remote_host: valid_host.then(|| host.to_owned())?,
            remote_port: self
                .remote_port
                .trim()
                .parse()
                .ok()
                .filter(|port| *port > 0)?,
        })
    }
}

/// Tunnels saisis ; les lignes entièrement vides sont ignorées.
/// `Err` si une ligne remplie est invalide.
pub fn parse_tunnels(drafts: &[TunnelDraft]) -> Result<Vec<LocalForward>, &'static str> {
    drafts
        .iter()
        .filter(|draft| !draft.is_blank())
        .map(|draft| {
            draft
                .parse()
                .ok_or("Tunnel invalide : port local, hôte et port distant requis (1–65535)")
        })
        .collect()
}

/// Rebond possible : une connexion SSH enregistrée, autre que celle-ci.
fn jump_candidates<'a>(connection: &Connection, all: &'a [Connection]) -> Vec<&'a Connection> {
    all.iter()
        .filter(|candidate| candidate.id != connection.id && candidate.kind == ConnectionKind::Ssh)
        .collect()
}

pub fn show(
    ui: &mut Ui,
    connection: &mut Connection,
    all: &[Connection],
    tunnels: &mut Vec<TunnelDraft>,
) {
    if connection.kind == ConnectionKind::Ftp {
        return;
    }
    widgets::micro_label(ui, "Réseau");
    jump_picker(ui, connection, all);
    if connection.kind == ConnectionKind::Ssh {
        tunnel_editor(ui, tunnels);
    }
    ui.add_space(6.0);
}

fn jump_picker(ui: &mut Ui, connection: &mut Connection, all: &[Connection]) {
    let candidates = jump_candidates(connection, all);
    let current = connection
        .jump_connection_id
        .as_deref()
        .and_then(|id| candidates.iter().find(|candidate| candidate.id == id))
        .map_or_else(
            || "Aucun — connexion directe".to_owned(),
            |jump| jump.name.clone(),
        );
    ui.horizontal(|ui| {
        ui.label(RichText::new("Rebond").font(theme::sans(12.0)).color(color::TEXT_MUTED));
        egui::ComboBox::from_id_salt("form-jump")
            .selected_text(current)
            .width(260.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut connection.jump_connection_id, None, "Aucun — connexion directe");
                for candidate in &candidates {
                    let label = format!("{}  ({})", candidate.name, candidate.target());
                    ui.selectable_value(&mut connection.jump_connection_id, Some(candidate.id.clone()), label);
                }
            })
            .response
            .on_hover_text("Passer par ce serveur SSH pour joindre celui-ci (ProxyJump). Ses identifiants doivent être mémorisés.");
    });
}

fn tunnel_editor(ui: &mut Ui, tunnels: &mut Vec<TunnelDraft>) {
    ui.label(
        RichText::new("Tunnels locaux : 127.0.0.1:port local → hôte:port vu depuis le serveur, ouverts tant que le terminal est connecté.")
            .font(theme::sans(12.0))
            .color(color::TEXT_DIM),
    );
    let mut removed = None;
    for (index, draft) in tunnels.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.add(widgets::text_field(&mut draft.local_port, "8080").desired_width(60.0));
            ui.label(RichText::new("→").color(color::TEXT_DIM));
            ui.add(widgets::text_field(&mut draft.remote_host, "localhost").desired_width(170.0));
            ui.label(RichText::new(":").color(color::TEXT_DIM));
            ui.add(widgets::text_field(&mut draft.remote_port, "80").desired_width(60.0));
            if ui
                .small_button("✕")
                .on_hover_text("Retirer ce tunnel")
                .clicked()
            {
                removed = Some(index);
            }
        });
    }
    if let Some(index) = removed {
        tunnels.remove(index);
    }
    if ui.small_button("+ Ajouter un tunnel").clicked() {
        tunnels.push(TunnelDraft::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(local: &str, host: &str, port: &str) -> TunnelDraft {
        TunnelDraft {
            local_port: local.into(),
            remote_host: host.into(),
            remote_port: port.into(),
        }
    }

    #[test]
    fn should_parse_a_complete_tunnel() {
        assert_eq!(
            draft(" 15432 ", "db.internal", "5432").parse(),
            Some(LocalForward {
                local_port: 15432,
                remote_host: "db.internal".into(),
                remote_port: 5432
            })
        );
    }

    #[test]
    fn should_ignore_blank_rows_but_reject_incomplete_ones() {
        assert_eq!(parse_tunnels(&[draft("", "", "")]), Ok(vec![]));
        assert!(parse_tunnels(&[draft("8080", "", "80")]).is_err());
        assert!(parse_tunnels(&[draft("0", "web", "80")]).is_err());
        assert!(parse_tunnels(&[draft("8080", "web;rm", "80")]).is_err());
    }

    #[test]
    fn should_offer_only_other_ssh_connections_as_jump() {
        let mut target = Connection::new(0);
        target.id = "target".into();
        let mut sftp = Connection::new(0);
        sftp.kind = ConnectionKind::Sftp;
        let bastion = Connection::new(0);
        let all = vec![target.clone(), sftp, bastion.clone()];
        let candidates = jump_candidates(&target, &all);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].id, bastion.id);
    }
}
