//! Protection de données par Windows (DPAPI).
//!
//! Le résultat n'est déchiffrable que par le même compte utilisateur, sur la
//! même machine. C'est ce qui permet d'ouvrir le coffre sans redemander la
//! phrase de récupération, sans pour autant laisser la clé en clair sur disque.

use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
};
use zeroize::Zeroizing;

use crate::error::VaultError;

fn input_blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    }
}

/// Copie le tampon alloué par Windows puis le libère.
///
/// # Safety
/// `blob` doit provenir d'un appel DPAPI réussi : Windows l'a alloué avec
/// `LocalAlloc` et il n'a pas encore été libéré.
unsafe fn take_output(blob: CRYPT_INTEGER_BLOB) -> Zeroizing<Vec<u8>> {
    // SAFETY : garanti par l'appelant — pbData pointe sur cbData octets valides.
    let bytes = unsafe { std::slice::from_raw_parts(blob.pbData, blob.cbData as usize) }.to_vec();
    // SAFETY : tampon alloué par DPAPI avec LocalAlloc, libéré une seule fois ici.
    unsafe { LocalFree(Some(HLOCAL(blob.pbData.cast()))) };
    Zeroizing::new(bytes)
}

pub fn protect(data: &[u8]) -> Result<Vec<u8>, VaultError> {
    let input = input_blob(data);
    let mut output = CRYPT_INTEGER_BLOB::default();
    // SAFETY : `input` pointe sur `data`, vivant pendant l'appel ; `output` est
    // rempli par Windows et libéré par `take_output`.
    unsafe {
        CryptProtectData(
            &input,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    }
    .map_err(|error| VaultError::Dpapi(error.message()))?;
    // SAFETY : l'appel a réussi, `output` est un tampon DPAPI valide.
    Ok(unsafe { take_output(output) }.to_vec())
}

pub fn unprotect(data: &[u8]) -> Result<Zeroizing<Vec<u8>>, VaultError> {
    let input = input_blob(data);
    let mut output = CRYPT_INTEGER_BLOB::default();
    // SAFETY : mêmes garanties que dans `protect`.
    unsafe {
        CryptUnprotectData(
            &input,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    }
    .map_err(|error| VaultError::Dpapi(error.message()))?;
    // SAFETY : l'appel a réussi, `output` est un tampon DPAPI valide.
    Ok(unsafe { take_output(output) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_round_trip_through_dpapi() {
        let protected = protect(b"cle-de-test").unwrap();
        assert_ne!(protected, b"cle-de-test");
        assert_eq!(unprotect(&protected).unwrap().as_slice(), b"cle-de-test");
    }

    #[test]
    fn should_reject_corrupted_blobs() {
        let mut protected = protect(b"cle-de-test").unwrap();
        let last = protected.len() - 1;
        protected[last] ^= 0xff;
        assert!(unprotect(&protected).is_err());
    }
}
