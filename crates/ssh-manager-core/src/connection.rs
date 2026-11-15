//! Une connexion enregistrée : où se connecter, avec quel protocole, comment
//! s'authentifier, et les métadonnées d'organisation (groupe, couleur, favori).

use serde::{Deserialize, Serialize};

/// Protocole d'une connexion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionKind {
    Ssh,
    Sftp,
    Ftp,
}

impl ConnectionKind {
    pub const ALL: [ConnectionKind; 3] = [Self::Ssh, Self::Sftp, Self::Ftp];

    /// Port conventionnel du protocole.
    pub fn default_port(self) -> u16 {
        match self {
            Self::Ssh | Self::Sftp => 22,
            Self::Ftp => 21,
        }
    }

    /// Libellé court affiché dans l'interface.
    pub fn label(self) -> &'static str {
        match self {
            Self::Ssh => "SSH",
            Self::Sftp => "SFTP",
            Self::Ftp => "FTP",
        }
    }

    /// Vrai si la connexion ouvre un navigateur de fichiers plutôt qu'un terminal.
    pub fn is_file_transfer(self) -> bool {
        matches!(self, Self::Sftp | Self::Ftp)
    }

    /// Vrai si le protocole repose sur SSH, donc sur une clé d'hôte à épingler.
    pub fn uses_host_key(self) -> bool {
        matches!(self, Self::Ssh | Self::Sftp)
    }
}

/// Une connexion enregistrée dans le coffre.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub id: String,
    pub name: String,
    pub kind: ConnectionKind,
    pub host: String,
    pub port: u16,
    pub username: String,
    /// Mot de passe mémorisé. Absent : il sera demandé à chaque ouverture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    /// Chemin d'une clé privée. Prioritaire sur le mot de passe.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_key_path: Option<String>,
    /// Phrase de passe de la clé privée, si elle est chiffrée et mémorisée.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_passphrase: Option<String>,
    /// FTP uniquement : FTPS (TLS explicite).
    #[serde(default)]
    pub ftp_secure: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default)]
    pub favorite: bool,
    /// Couleur propre, prioritaire sur celle du groupe (`#rrggbb`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Horodatage de création, en millisecondes depuis l'époque Unix.
    pub created_at: i64,
    /// Dernière ouverture d'une session, en millisecondes depuis l'époque Unix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<i64>,
}

impl Connection {
    /// Crée une connexion SSH vierge avec un identifiant unique.
    pub fn new(created_at: i64) -> Self {
        Self {
            id: format!("conn-{}", uuid::Uuid::new_v4()),
            name: String::new(),
            kind: ConnectionKind::Ssh,
            host: String::new(),
            port: ConnectionKind::Ssh.default_port(),
            username: String::new(),
            password: None,
            private_key_path: None,
            key_passphrase: None,
            ftp_secure: false,
            group: None,
            favorite: false,
            color: None,
            created_at,
            last_used_at: None,
        }
    }

    /// `user@host:port`, la cible telle qu'on la saisirait en ligne de commande.
    pub fn target(&self) -> String {
        format!("{}@{}:{}", self.username, self.host, self.port)
    }

    /// Vrai si l'ouverture d'une session devra demander un mot de passe.
    pub fn needs_password_prompt(&self) -> bool {
        self.private_key_path.is_none() && self.password.is_none()
    }

    /// Identifiant d'épinglage de la clé d'hôte, identique à celui de l'app Electron.
    pub fn host_key_id(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    /// Connexion ponctuelle à partir d'une saisie `user@hôte[:port]`, comme
    /// dans la barre de recherche. `None` si la saisie n'a pas cette forme.
    pub fn from_quick_target(text: &str, kind: ConnectionKind, created_at: i64) -> Option<Self> {
        let text = text.trim();
        if text.chars().any(char::is_whitespace) {
            return None;
        }
        let (username, address) = text.rsplit_once('@')?;
        let (host, port) = match address.rsplit_once(':') {
            // Une adresse IPv6 contient des « : » : on n'y cherche pas de port.
            Some((host, port)) if !host.contains(':') => {
                (host, port.parse::<u16>().ok().filter(|port| *port > 0)?)
            }
            _ => (address, kind.default_port()),
        };
        let valid_host = !host.is_empty()
            && host
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || ".-_:".contains(character));
        if username.is_empty() || !valid_host {
            return None;
        }
        let mut connection = Self::new(created_at);
        connection.kind = kind;
        connection.name = format!("{username}@{host}");
        connection.host = host.to_owned();
        connection.port = port;
        connection.username = username.to_owned();
        Some(connection)
    }

    /// Vérifie les champs obligatoires. Renvoie la liste des problèmes, vide si valide.
    pub fn validate(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        if self.name.trim().is_empty() {
            errors.push(ValidationError::MissingName);
        }
        if self.host.trim().is_empty() {
            errors.push(ValidationError::MissingHost);
        }
        if self.username.trim().is_empty() {
            errors.push(ValidationError::MissingUsername);
        }
        if self.port == 0 {
            errors.push(ValidationError::InvalidPort);
        }
        errors
    }
}

/// Problème de saisie détecté dans une connexion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationError {
    MissingName,
    MissingHost,
    MissingUsername,
    InvalidPort,
}

impl ValidationError {
    pub fn message(self) -> &'static str {
        match self {
            Self::MissingName => "Le nom est requis",
            Self::MissingHost => "L'hôte est requis",
            Self::MissingUsername => "Le nom d'utilisateur est requis",
            Self::InvalidPort => "Port invalide (1–65535)",
        }
    }
}

#[cfg(test)]
mod quick_target_tests {
    use super::*;

    fn parse(text: &str) -> Option<Connection> {
        Connection::from_quick_target(text, ConnectionKind::Ssh, 0)
    }

    #[test]
    fn should_parse_user_host_and_port() {
        let connection = parse("deploy@10.0.0.5:2222").unwrap();
        assert_eq!(
            (
                connection.username.as_str(),
                connection.host.as_str(),
                connection.port
            ),
            ("deploy", "10.0.0.5", 2222)
        );
        assert_eq!(connection.name, "deploy@10.0.0.5");
    }

    #[test]
    fn should_use_the_protocol_default_port_when_omitted() {
        let connection =
            Connection::from_quick_target("anon@ftp.example.org", ConnectionKind::Ftp, 0).unwrap();
        assert_eq!(connection.port, 21);
        assert_eq!(connection.kind, ConnectionKind::Ftp);
    }

    #[test]
    fn should_keep_an_ipv6_address_whole() {
        assert_eq!(parse("root@fe80::1").unwrap().host, "fe80::1");
    }

    #[test]
    fn should_ignore_plain_search_text() {
        assert!(parse("prod web").is_none());
        assert!(parse("web-01").is_none());
        assert!(parse("@host").is_none());
        assert!(parse("user@").is_none());
        assert!(parse("user@host:99999").is_none());
        assert!(parse("user@host/../x").is_none());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Connection {
        Connection {
            name: "prod".into(),
            host: "10.0.0.1".into(),
            username: "root".into(),
            ..Connection::new(0)
        }
    }

    #[test]
    fn should_use_protocol_default_ports() {
        assert_eq!(ConnectionKind::Ssh.default_port(), 22);
        assert_eq!(ConnectionKind::Sftp.default_port(), 22);
        assert_eq!(ConnectionKind::Ftp.default_port(), 21);
    }

    #[test]
    fn should_format_target_as_user_at_host_colon_port() {
        assert_eq!(sample().target(), "root@10.0.0.1:22");
    }

    #[test]
    fn should_require_password_prompt_when_no_credential_is_stored() {
        assert!(sample().needs_password_prompt());
    }

    #[test]
    fn should_not_prompt_when_a_private_key_is_set() {
        let connection = Connection {
            private_key_path: Some("C:/id_ed25519".into()),
            ..sample()
        };
        assert!(!connection.needs_password_prompt());
    }

    #[test]
    fn should_report_every_missing_field() {
        let errors = Connection::new(0).validate();
        assert_eq!(
            errors,
            vec![
                ValidationError::MissingName,
                ValidationError::MissingHost,
                ValidationError::MissingUsername
            ]
        );
    }

    #[test]
    fn should_generate_unique_identifiers() {
        assert_ne!(Connection::new(0).id, Connection::new(0).id);
    }
}
