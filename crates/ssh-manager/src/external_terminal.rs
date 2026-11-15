//! Ouverture d'une connexion SSH dans PowerShell ou l'invite de commandes,
//! avec le client OpenSSH de Windows.
//!
//! La ligne de commande est interprétée par le shell : hôte et utilisateur sont
//! réduits à un jeu de caractères sûr, et un chemin de clé contenant un
//! caractère spécial pour le shell est refusé plutôt qu'échappé.

use std::os::windows::process::CommandExt;
use std::process::Command;

use ssh_manager_core::Connection;

/// Ouvre une nouvelle fenêtre de console au lieu d'hériter de celle de l'app.
const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalShell {
    PowerShell,
    Cmd,
}

impl ExternalShell {
    pub fn label(self) -> &'static str {
        match self {
            Self::PowerShell => "PowerShell",
            Self::Cmd => "Invite de commandes",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExternalTerminalError {
    #[error("le chemin de la clé contient un caractère non pris en charge par le shell")]
    UnsafeKeyPath,
    #[error("l'hôte ou l'utilisateur est vide après nettoyage")]
    EmptyTarget,
    #[error("lancement de {shell} impossible : {source}")]
    Launch {
        shell: &'static str,
        #[source]
        source: std::io::Error,
    },
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || "._@:-".contains(*character))
        .take(128)
        .collect()
}

/// Commande `ssh …` à exécuter dans le shell choisi.
pub fn ssh_command(
    connection: &Connection,
    shell: ExternalShell,
) -> Result<String, ExternalTerminalError> {
    let host = sanitize(&connection.host);
    let user = sanitize(&connection.username);
    if host.is_empty() || user.is_empty() {
        return Err(ExternalTerminalError::EmptyTarget);
    }
    let mut command = String::from("ssh");
    if connection.port != 22 {
        command.push_str(&format!(" -p {}", connection.port));
    }
    if let Some(key) = &connection.private_key_path {
        let forbidden: &[char] = match shell {
            ExternalShell::PowerShell => &['\'', '`', '$', '"', '\n', '\r'],
            ExternalShell::Cmd => &['"', '%', '^', '&', '|', '<', '>', '!', '\n', '\r'],
        };
        if key.contains(forbidden) {
            return Err(ExternalTerminalError::UnsafeKeyPath);
        }
        match shell {
            ExternalShell::PowerShell => command.push_str(&format!(" -i '{key}'")),
            ExternalShell::Cmd => command.push_str(&format!(" -i \"{key}\"")),
        }
    }
    command.push_str(&format!(" {user}@{host}"));
    Ok(command)
}

/// Ouvre une fenêtre du shell choisi, connectée au serveur.
pub fn open(connection: &Connection, shell: ExternalShell) -> Result<(), ExternalTerminalError> {
    let command = ssh_command(connection, shell)?;
    let mut process = match shell {
        ExternalShell::PowerShell => {
            let mut process = Command::new("powershell.exe");
            process.args(["-NoExit", "-Command", &command]);
            process
        }
        ExternalShell::Cmd => {
            let mut process = Command::new("cmd.exe");
            // Argument brut : cmd.exe ne suit pas les règles de guillemets du C.
            process.raw_arg(format!("/K {command}"));
            process
        }
    };
    process
        .creation_flags(CREATE_NEW_CONSOLE)
        .spawn()
        .map_err(|source| ExternalTerminalError::Launch {
            shell: shell.label(),
            source,
        })?;
    tracing::info!(shell = shell.label(), target = %connection.target(), "connection opened in an external terminal");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connection() -> Connection {
        Connection {
            host: "10.0.0.5".into(),
            username: "deploy".into(),
            ..Connection::new(0)
        }
    }

    #[test]
    fn should_omit_the_default_port() {
        assert_eq!(
            ssh_command(&connection(), ExternalShell::Cmd).unwrap(),
            "ssh deploy@10.0.0.5"
        );
    }

    #[test]
    fn should_pass_a_custom_port_and_the_private_key() {
        let connection = Connection {
            port: 2222,
            private_key_path: Some(r"C:\Users\me\.ssh\id ed25519".into()),
            ..connection()
        };
        assert_eq!(
            ssh_command(&connection, ExternalShell::PowerShell).unwrap(),
            r"ssh -p 2222 -i 'C:\Users\me\.ssh\id ed25519' deploy@10.0.0.5"
        );
        assert_eq!(
            ssh_command(&connection, ExternalShell::Cmd).unwrap(),
            r#"ssh -p 2222 -i "C:\Users\me\.ssh\id ed25519" deploy@10.0.0.5"#
        );
    }

    #[test]
    fn should_strip_shell_metacharacters_from_host_and_user() {
        let connection = Connection {
            host: "srv; rm -rf /".into(),
            username: "a&b".into(),
            ..connection()
        };
        assert_eq!(
            ssh_command(&connection, ExternalShell::Cmd).unwrap(),
            "ssh ab@srvrm-rf"
        );
    }

    #[test]
    fn should_refuse_a_key_path_that_the_shell_would_interpret() {
        let connection = Connection {
            private_key_path: Some(r"C:\keys\%USERNAME%\id".into()),
            ..connection()
        };
        assert!(matches!(
            ssh_command(&connection, ExternalShell::Cmd),
            Err(ExternalTerminalError::UnsafeKeyPath)
        ));
    }
}
