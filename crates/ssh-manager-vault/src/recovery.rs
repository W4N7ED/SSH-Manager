//! Phrase de récupération BIP39 (12 mots, liste anglaise).
//!
//! Même format que l'application Electron (`bip39.generateMnemonic()` :
//! 128 bits d'entropie, liste anglaise), pour que les phrases existantes soient
//! reconnues.

use bip39::{Language, Mnemonic};

use crate::crypto::random_bytes;
use crate::error::VaultError;

const ENTROPY_BYTES: usize = 16; // 128 bits → 12 mots

/// Tire une nouvelle phrase de 12 mots.
pub fn generate_phrase() -> String {
    let entropy = random_bytes::<ENTROPY_BYTES>();
    Mnemonic::from_entropy_in(Language::English, &entropy)
        .expect("16 octets d'entropie forment toujours une phrase valide")
        .to_string()
}

/// Vérifie qu'une phrase saisie est une phrase BIP39 bien formée (mots connus,
/// somme de contrôle correcte). Ne dit rien de sa correspondance avec un coffre.
pub fn validate_phrase(phrase: &str) -> Result<(), VaultError> {
    Mnemonic::parse_in_normalized(Language::English, &crate::crypto::normalize_phrase(phrase))
        .map(|_| ())
        .map_err(|error| VaultError::InvalidRecoveryPhrase(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_generate_twelve_words() {
        assert_eq!(generate_phrase().split_whitespace().count(), 12);
    }

    #[test]
    fn should_accept_its_own_phrases() {
        assert!(validate_phrase(&generate_phrase()).is_ok());
    }

    #[test]
    fn should_reject_a_bad_checksum() {
        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon";
        assert!(validate_phrase(phrase).is_err());
    }

    #[test]
    fn should_accept_mixed_case_input() {
        let phrase = "ABANDON abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon About";
        assert!(validate_phrase(phrase).is_ok());
    }
}
