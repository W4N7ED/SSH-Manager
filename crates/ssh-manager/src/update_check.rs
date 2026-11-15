//! Vérification des mises à jour : la dernière Release publiée sur GitHub
//! est comparée à la version en cours. Rien n'est téléchargé ni installé
//! automatiquement : l'utilisateur ouvre la page de la version s'il le veut.
//!
//! Une seule requête HTTPS au démarrage, sans aucune donnée de l'utilisateur.
//! HTTP/1.0 suffit (réponse d'un seul tenant, sans découpage) et évite une
//! dépendance de client HTTP complète.

use std::time::Duration;

use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const API_HOST: &str = "api.github.com";
const LATEST_RELEASE_PATH: &str = "/repos/W4N7ED/SSH-Manager/releases/latest";
const TIMEOUT: Duration = Duration::from_secs(10);
/// Une réponse plus grosse n'est pas celle attendue.
const MAX_RESPONSE_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub page_url: String,
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
}

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("réseau : {0}")]
    Network(String),
    #[error("réponse inattendue : {0}")]
    Response(String),
}

/// « v2.1.0 » ou « 2.1.0 » → (2, 1, 0) ; `None` si ce n'est pas une version.
fn parse_version(text: &str) -> Option<(u32, u32, u32)> {
    let mut parts = text.trim().trim_start_matches('v').split('.');
    let mut next = || {
        parts
            .next()?
            .split(|character: char| !character.is_ascii_digit())
            .next()?
            .parse()
            .ok()
    };
    Some((next()?, next()?, next().unwrap_or(0)))
}

/// Vrai si `latest` est strictement plus récente que `current`.
pub fn is_newer(latest: &str, current: &str) -> bool {
    matches!((parse_version(latest), parse_version(current)), (Some(latest), Some(current)) if latest > current)
}

/// Sépare l'en-tête de la réponse HTTP : code d'état et corps.
fn split_response(raw: &[u8]) -> Result<(u16, &[u8]), UpdateError> {
    let end = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| UpdateError::Response("en-tête incomplet".into()))?;
    let head = String::from_utf8_lossy(&raw[..end]);
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| UpdateError::Response("code d'état illisible".into()))?;
    Ok((status, &raw[end + 4..]))
}

/// Interprète la réponse de l'API : `None` s'il n'y a encore aucune Release.
fn parse_latest_release(raw: &[u8]) -> Result<Option<Release>, UpdateError> {
    let (status, body) = split_response(raw)?;
    match status {
        200 => {
            let release: GithubRelease = serde_json::from_slice(body)
                .map_err(|error| UpdateError::Response(error.to_string()))?;
            Ok(Some(Release {
                version: release.tag_name.trim_start_matches('v').to_owned(),
                page_url: release.html_url,
            }))
        }
        404 => Ok(None),
        other => Err(UpdateError::Response(format!("code HTTP {other}"))),
    }
}

async fn fetch_latest_release() -> Result<Vec<u8>, UpdateError> {
    let network = |error: &dyn std::fmt::Display| UpdateError::Network(error.to_string());
    let config = crate::tls::client_config().map_err(|error| network(&error))?;
    let connector = tokio_rustls::TlsConnector::from(config);
    let tcp = TcpStream::connect((API_HOST, 443))
        .await
        .map_err(|error| network(&error))?;
    let server_name =
        rustls::pki_types::ServerName::try_from(API_HOST).map_err(|error| network(&error))?;
    let mut stream = connector
        .connect(server_name, tcp)
        .await
        .map_err(|error| network(&error))?;
    let request = format!(
        "GET {LATEST_RELEASE_PATH} HTTP/1.0\r\nHost: {API_HOST}\r\nUser-Agent: ssh-manager\r\nAccept: application/vnd.github+json\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|error| network(&error))?;
    let mut raw = Vec::new();
    (&mut stream)
        .take(MAX_RESPONSE_BYTES)
        .read_to_end(&mut raw)
        .await
        .map_err(|error| network(&error))?;
    Ok(raw)
}

/// Dernière Release plus récente que la version en cours, s'il y en a une.
pub async fn newer_release() -> Result<Option<Release>, UpdateError> {
    let raw = tokio::time::timeout(TIMEOUT, fetch_latest_release())
        .await
        .map_err(|_| {
            UpdateError::Network(format!("aucune réponse après {} s", TIMEOUT.as_secs()))
        })??;
    Ok(parse_latest_release(&raw)?
        .filter(|release| is_newer(&release.version, env!("CARGO_PKG_VERSION"))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_compare_versions_numerically() {
        assert!(is_newer("v2.10.0", "2.9.3"));
        assert!(is_newer("3.0", "2.99.99"));
        assert!(!is_newer("v2.0.0", "2.0.0"));
        assert!(!is_newer("1.9.0", "2.0.0"));
        assert!(!is_newer("nightly", "2.0.0"));
    }

    #[test]
    fn should_read_the_latest_release() {
        let raw = b"HTTP/1.0 200 OK\r\nContent-Type: application/json\r\n\r\n{\"tag_name\":\"v2.1.0\",\"html_url\":\"https://github.com/W4N7ED/SSH-Manager/releases/tag/v2.1.0\",\"draft\":false}";
        assert_eq!(
            parse_latest_release(raw).unwrap(),
            Some(Release {
                version: "2.1.0".into(),
                page_url: "https://github.com/W4N7ED/SSH-Manager/releases/tag/v2.1.0".into()
            })
        );
    }

    #[test]
    fn should_treat_a_repository_without_release_as_up_to_date() {
        assert_eq!(
            parse_latest_release(b"HTTP/1.0 404 Not Found\r\n\r\n{}").unwrap(),
            None
        );
    }

    #[test]
    fn should_reject_an_unexpected_answer() {
        assert!(parse_latest_release(b"HTTP/1.0 403 Forbidden\r\n\r\nrate limited").is_err());
        assert!(parse_latest_release(b"garbage").is_err());
    }
}
