use std::path::PathBuf;

/// Erreurs du coffre. Chaque variante dit ce qui a échoué et où, sans jamais
/// inclure de secret dans son message.
#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("impossible de lire ou d'écrire {path} : {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("fichier {path} illisible : {source}")]
    Format {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("phrase de récupération incorrecte")]
    WrongRecoveryPhrase,

    #[error("phrase de récupération invalide : {0}")]
    InvalidRecoveryPhrase(String),

    #[error(
        "la clé protégée par Windows n'est pas lisible sur ce poste — la phrase de récupération est nécessaire"
    )]
    RecoveryPhraseRequired,

    #[error("données chiffrées corrompues ou clé incorrecte ({context})")]
    Decryption { context: &'static str },

    #[error("protection Windows (DPAPI) indisponible : {0}")]
    Dpapi(String),

    #[error("version de coffre non prise en charge : {0}")]
    UnsupportedVersion(u32),

    #[error("coffre de l'ancienne application incomplet : {0}")]
    IncompleteLegacyVault(&'static str),
}

impl VaultError {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    pub(crate) fn format(path: impl Into<PathBuf>, source: serde_json::Error) -> Self {
        Self::Format {
            path: path.into(),
            source,
        }
    }
}
