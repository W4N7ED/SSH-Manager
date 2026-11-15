//! Configuration TLS commune (FTPS, vérification des mises à jour) : rustls
//! avec les autorités du magasin de certificats Windows, y compris celles
//! qu'une entreprise y déploie.

use std::sync::Arc;

use rustls::ClientConfig;
use rustls_platform_verifier::BuilderVerifierExt;

pub fn client_config() -> Result<Arc<ClientConfig>, rustls::Error> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_platform_verifier()?
        .with_no_client_auth();
    Ok(Arc::new(config))
}
