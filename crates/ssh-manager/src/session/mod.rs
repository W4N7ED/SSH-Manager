//! Sessions ouvertes sur les serveurs : émulation de terminal, connexion SSH et
//! rendu dans l'interface.

pub mod emulator;
pub mod keys;
pub mod palette;
pub mod private_key;
pub mod ssh;
pub mod terminal_theme;
pub mod transcript;
pub mod transport;
pub mod utf8_boundary;
pub mod view;

#[cfg(test)]
mod ssh_tests;
