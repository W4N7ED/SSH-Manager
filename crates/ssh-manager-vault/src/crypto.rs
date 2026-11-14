//! Primitives cryptographiques du coffre.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use hmac::{Hmac, Mac};
use rand::RngExt;
use sha2::{Sha256, Sha512};
use zeroize::Zeroizing;

use crate::error::VaultError;

/// Itérations PBKDF2 — identiques à l'application Electron, pour que la phrase
/// de récupération existante dérive la même clé.
pub const KEY_DERIVATION_ROUNDS: u32 = 210_000;
pub const KEY_LENGTH: usize = 32;
pub const SALT_LENGTH: usize = 32;
pub const NONCE_LENGTH: usize = 12;

pub type VaultKey = Zeroizing<[u8; KEY_LENGTH]>;

/// La phrase est normalisée comme dans l'app Electron : espaces de bord retirés,
/// minuscules. L'ordre et l'espacement interne des mots comptent.
pub fn normalize_phrase(phrase: &str) -> String {
    phrase.trim().to_lowercase()
}

/// Clé du coffre dérivée de la phrase de récupération.
pub fn derive_key(phrase: &str, salt: &[u8]) -> VaultKey {
    let mut key = Zeroizing::new([0u8; KEY_LENGTH]);
    pbkdf2::pbkdf2_hmac::<Sha512>(
        normalize_phrase(phrase).as_bytes(),
        salt,
        KEY_DERIVATION_ROUNDS,
        key.as_mut(),
    );
    key
}

/// Empreinte de contrôle de la phrase : permet de dire « phrase incorrecte »
/// sans tenter de déchiffrer, et sans rien révéler de la clé.
pub fn phrase_verification_hash(phrase: &str, salt: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(salt).expect("HMAC accepte toute longueur de clé");
    mac.update(normalize_phrase(phrase).as_bytes());
    to_hex(&mac.finalize().into_bytes())
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    rand::rng().fill(&mut bytes[..]);
    bytes
}

/// Chiffre et authentifie. Renvoie `(nonce, chiffré)`.
pub fn seal(key: &[u8; KEY_LENGTH], plaintext: &[u8]) -> ([u8; NONCE_LENGTH], Vec<u8>) {
    let cipher = Aes256Gcm::new_from_slice(key).expect("clé de 32 octets");
    let nonce = random_bytes::<NONCE_LENGTH>();
    let ciphertext = cipher
        .encrypt(&Nonce::from(nonce), plaintext)
        .expect("le chiffrement AES-GCM n'échoue que sur des tailles hors limites");
    (nonce, ciphertext)
}

/// Déchiffre et vérifie l'authenticité. Échoue si un seul bit a changé.
pub fn open(
    key: &[u8; KEY_LENGTH],
    nonce: &[u8],
    ciphertext: &[u8],
    context: &'static str,
) -> Result<Zeroizing<Vec<u8>>, VaultError> {
    let nonce: [u8; NONCE_LENGTH] = nonce
        .try_into()
        .map_err(|_| VaultError::Decryption { context })?;
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| VaultError::Decryption { context })?;
    cipher
        .decrypt(&Nonce::from(nonce), ciphertext)
        .map(Zeroizing::new)
        .map_err(|_| VaultError::Decryption { context })
}

pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn from_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok())
        .collect()
}

/// Comparaison en temps constant, pour ne pas laisser fuiter d'information
/// sur une empreinte par la durée de la comparaison.
pub fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_normalize_case_and_surrounding_whitespace() {
        assert_eq!(normalize_phrase("  Abandon ABILITY  "), "abandon ability");
    }

    #[test]
    fn should_round_trip_sealed_data() {
        let key = [7u8; KEY_LENGTH];
        let (nonce, ciphertext) = seal(&key, b"secret");
        assert_eq!(
            open(&key, &nonce, &ciphertext, "test").unwrap().as_slice(),
            b"secret"
        );
    }

    #[test]
    fn should_reject_tampered_ciphertext() {
        let key = [7u8; KEY_LENGTH];
        let (nonce, mut ciphertext) = seal(&key, b"secret");
        ciphertext[0] ^= 1;
        assert!(open(&key, &nonce, &ciphertext, "test").is_err());
    }

    #[test]
    fn should_reject_the_wrong_key() {
        let (nonce, ciphertext) = seal(&[7u8; KEY_LENGTH], b"secret");
        assert!(open(&[8u8; KEY_LENGTH], &nonce, &ciphertext, "test").is_err());
    }

    #[test]
    fn should_use_a_fresh_nonce_for_every_seal() {
        let key = [7u8; KEY_LENGTH];
        assert_ne!(seal(&key, b"a").0, seal(&key, b"a").0);
    }

    #[test]
    fn should_round_trip_hex() {
        assert_eq!(from_hex(&to_hex(&[0, 15, 255])).unwrap(), vec![0, 15, 255]);
        assert!(from_hex("abc").is_none());
    }
}
