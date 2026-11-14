//! Sessions ouvertes sur les serveurs : émulation de terminal, connexion SSH et
//! rendu dans l'interface.

pub mod emulator;
pub mod keys;
pub mod palette;
pub mod ssh;
pub mod transport;
pub mod view;

#[cfg(test)]
mod ssh_tests;
