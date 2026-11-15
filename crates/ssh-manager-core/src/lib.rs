//! Domaine de SSH Manager.
//!
//! Ce crate ne fait aucune entrée/sortie : ni fichier, ni réseau, ni interface.
//! Il décrit ce qu'est une connexion, comment on les organise et les filtre, et
//! fournit les utilitaires purs (générateur de mots de passe, formatage).

pub mod connection;
pub mod format;
pub mod library;
pub mod password;
pub mod ssh_config;

pub use connection::{Connection, ConnectionKind, LocalForward};
pub use library::{ConnectionLibrary, GroupFilter, KindFilter};
