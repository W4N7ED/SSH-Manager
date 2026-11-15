//! Coffre chiffré de SSH Manager.
//!
//! # Modèle de sécurité
//!
//! Tout le contenu du coffre (connexions, mots de passe, empreintes d'hôtes)
//! est chiffré en AES-256-GCM sous une clé de 32 octets.
//!
//! Cette clé est dérivée de la **phrase de récupération** de 12 mots
//! (PBKDF2-SHA512, 210 000 itérations) — la même dérivation que l'application
//! Electron, si bien qu'une phrase existante reste valable après migration.
//!
//! Pour éviter de saisir la phrase à chaque lancement, une copie de la clé est
//! protégée par DPAPI : elle n'est déchiffrable que par le même utilisateur
//! Windows, sur la même machine. Sur une autre machine (version portable,
//! profil restauré), DPAPI échoue et la phrase est demandée.

pub mod backup;
pub mod crypto;
pub mod error;
pub mod legacy;
pub mod paths;
pub mod recovery;
pub mod sealed_log;
pub mod vault;

#[cfg(windows)]
pub mod dpapi;

pub use error::VaultError;
pub use vault::{Vault, VaultData};
