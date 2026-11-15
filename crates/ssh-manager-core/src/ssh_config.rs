//! Lecture d'un fichier `~/.ssh/config` (format OpenSSH) pour en créer des
//! connexions.
//!
//! Sont repris, pour chaque bloc `Host` nommé : `HostName`, `User`, `Port`,
//! `IdentityFile`, `ProxyJump` (premier rebond) et `LocalForward`. Les motifs
//! génériques (`*`, `?`, `!`), les blocs `Match` et `Include` sont ignorés :
//! ils décrivent des règles, pas des serveurs.

use std::collections::HashMap;

use crate::connection::{Connection, ConnectionKind, LocalForward};

/// Un bloc `Host` tel qu'écrit dans le fichier.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct HostEntry {
    pub alias: String,
    pub host_name: Option<String>,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub identity_file: Option<String>,
    pub proxy_jump: Option<String>,
    pub local_forwards: Vec<LocalForward>,
}

fn is_pattern(alias: &str) -> bool {
    alias.contains(['*', '?', '!'])
}

/// « mot-clé valeur » ou « mot-clé=valeur », guillemets retirés.
fn split_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let separator = line.find(|character: char| character.is_whitespace() || character == '=')?;
    let keyword = line[..separator].to_ascii_lowercase();
    let value = line[separator..]
        .trim_start_matches(|character: char| character.is_whitespace() || character == '=')
        .trim()
        .trim_matches('"')
        .to_owned();
    Some((keyword, value))
}

/// `LocalForward [adresse:]port hôte:port`.
fn parse_local_forward(value: &str) -> Option<LocalForward> {
    let mut parts = value.split_whitespace();
    let local = parts.next()?;
    let (remote_host, remote_port) = parts.next()?.rsplit_once(':')?;
    let local_port = local.rsplit(':').next()?.parse().ok()?;
    Some(LocalForward {
        local_port,
        remote_host: remote_host.trim_matches(['[', ']']).to_owned(),
        remote_port: remote_port.parse().ok()?,
    })
}

/// Lit les blocs `Host` nommés, dans l'ordre du fichier.
pub fn parse(text: &str) -> Vec<HostEntry> {
    let mut entries: Vec<HostEntry> = Vec::new();
    // Un bloc « Host a b » s'applique à plusieurs alias : on garde le premier nommé.
    let mut current: Option<usize> = None;
    for (keyword, value) in text.lines().filter_map(split_line) {
        match keyword.as_str() {
            "host" => {
                current = value
                    .split_whitespace()
                    .find(|alias| !is_pattern(alias))
                    .map(|alias| {
                        entries.push(HostEntry {
                            alias: alias.to_owned(),
                            ..HostEntry::default()
                        });
                        entries.len() - 1
                    });
            }
            "match" => current = None,
            _ => {
                let Some(entry) = current.map(|index| &mut entries[index]) else {
                    continue;
                };
                // Comme OpenSSH : la première valeur d'un mot-clé l'emporte.
                match keyword.as_str() {
                    "hostname" => {
                        entry.host_name.get_or_insert(value);
                    }
                    "user" => {
                        entry.user.get_or_insert(value);
                    }
                    "port" => {
                        if entry.port.is_none() {
                            entry.port = value.parse().ok();
                        }
                    }
                    "identityfile" => {
                        entry.identity_file.get_or_insert(value);
                    }
                    "proxyjump" => {
                        let first_hop = value
                            .split(',')
                            .next()
                            .unwrap_or_default()
                            .trim()
                            .to_owned();
                        if first_hop != "none" {
                            entry.proxy_jump.get_or_insert(first_hop);
                        }
                    }
                    "localforward" => entry.local_forwards.extend(parse_local_forward(&value)),
                    _ => {}
                }
            }
        }
    }
    entries
}

/// `~/…` → chemin absolu dans `home`.
fn expand_home(path: &str, home: &str) -> String {
    match path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        Some(rest) => format!(
            "{}\\{}",
            home.trim_end_matches(['/', '\\']),
            rest.replace('/', "\\")
        ),
        None => path.to_owned(),
    }
}

/// Paramètres communs à la conversion des blocs en connexions.
pub struct ImportContext<'a> {
    /// Dossier personnel, pour développer `~` dans les chemins de clés.
    pub home: &'a str,
    /// Utilisateur par défaut d'OpenSSH quand `User` est absent.
    pub default_user: &'a str,
    pub group: &'a str,
    pub now: i64,
}

fn connection_for(entry: &HostEntry, context: &ImportContext<'_>) -> Connection {
    let mut connection = Connection::new(context.now);
    connection.kind = ConnectionKind::Ssh;
    connection.name = entry.alias.clone();
    connection.host = entry
        .host_name
        .clone()
        .unwrap_or_else(|| entry.alias.clone());
    connection.port = entry.port.unwrap_or(22);
    connection.username = entry
        .user
        .clone()
        .unwrap_or_else(|| context.default_user.to_owned());
    connection.private_key_path = entry
        .identity_file
        .as_deref()
        .map(|path| expand_home(path, context.home));
    connection.local_forwards = entry.local_forwards.clone();
    connection.group = Some(context.group.to_owned());
    connection
}

/// Crée une connexion par bloc. Un `ProxyJump` qui nomme un autre bloc
/// devient un lien vers sa connexion ; une adresse `user@hôte:port` donne
/// une connexion de rebond supplémentaire.
pub fn to_connections(entries: &[HostEntry], context: &ImportContext<'_>) -> Vec<Connection> {
    let mut connections: Vec<Connection> = entries
        .iter()
        .map(|entry| connection_for(entry, context))
        .collect();
    let mut by_alias: HashMap<String, String> = entries
        .iter()
        .zip(&connections)
        .map(|(entry, connection)| (entry.alias.clone(), connection.id.clone()))
        .collect();
    for (index, entry) in entries.iter().enumerate() {
        let Some(jump) = &entry.proxy_jump else {
            continue;
        };
        let jump_id = match by_alias.get(jump) {
            Some(id) => id.clone(),
            None => {
                let Some(mut hop) = Connection::from_quick_target(
                    &with_default_user(jump, context.default_user),
                    ConnectionKind::Ssh,
                    context.now,
                ) else {
                    continue;
                };
                hop.group = Some(context.group.to_owned());
                by_alias.insert(jump.clone(), hop.id.clone());
                let id = hop.id.clone();
                connections.push(hop);
                id
            }
        };
        connections[index].jump_connection_id = Some(jump_id);
    }
    connections
}

fn with_default_user(target: &str, default_user: &str) -> String {
    if target.contains('@') {
        target.to_owned()
    } else {
        format!("{default_user}@{target}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = r#"
# Serveurs de production
Host *
    ServerAliveInterval 30

Host web prod-web
    HostName 10.0.0.5
    User deploy
    Port 2222
    IdentityFile ~/.ssh/id_ed25519_web
    LocalForward 8080 localhost:80

Host db
    HostName=db.internal
    ProxyJump web
    LocalForward 127.0.0.1:15432 db.internal:5432

Host legacy
    ProxyJump admin@bastion.example.org:2200

Match host *.internal
    User nobody
"#;

    fn context() -> ImportContext<'static> {
        ImportContext {
            home: r"C:\Users\me",
            default_user: "me",
            group: "ssh config",
            now: 0,
        }
    }

    #[test]
    fn should_read_named_hosts_and_ignore_patterns() {
        let entries = parse(CONFIG);
        let aliases: Vec<&str> = entries.iter().map(|entry| entry.alias.as_str()).collect();
        assert_eq!(aliases, ["web", "db", "legacy"]);
        let web = &entries[0];
        assert_eq!(web.host_name.as_deref(), Some("10.0.0.5"));
        assert_eq!(
            (web.user.as_deref(), web.port),
            (Some("deploy"), Some(2222))
        );
        assert_eq!(
            entries[1].host_name.as_deref(),
            Some("db.internal"),
            "syntaxe « clé=valeur »"
        );
    }

    #[test]
    fn should_read_local_forwards() {
        let entries = parse(CONFIG);
        assert_eq!(
            entries[1].local_forwards,
            [LocalForward {
                local_port: 15432,
                remote_host: "db.internal".into(),
                remote_port: 5432
            }]
        );
    }

    #[test]
    fn should_build_connections_with_defaults_and_expanded_keys() {
        let connections = to_connections(&parse(CONFIG), &context());
        let web = &connections[0];
        assert_eq!(web.target(), "deploy@10.0.0.5:2222");
        assert_eq!(
            web.private_key_path.as_deref(),
            Some(r"C:\Users\me\.ssh\id_ed25519_web")
        );
        assert_eq!(web.group.as_deref(), Some("ssh config"));
        let db = &connections[1];
        assert_eq!(
            db.target(),
            "me@db.internal:22",
            "utilisateur Windows par défaut"
        );
    }

    #[test]
    fn should_link_a_jump_to_another_imported_host() {
        let connections = to_connections(&parse(CONFIG), &context());
        assert_eq!(
            connections[1].jump_connection_id.as_deref(),
            Some(connections[0].id.as_str())
        );
    }

    #[test]
    fn should_create_a_jump_connection_for_an_explicit_address() {
        let connections = to_connections(&parse(CONFIG), &context());
        let hop = connections.last().unwrap();
        assert_eq!(hop.target(), "admin@bastion.example.org:2200");
        assert_eq!(
            connections[2].jump_connection_id.as_deref(),
            Some(hop.id.as_str())
        );
    }
}
