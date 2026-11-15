//! Accès direct au presse-papiers Windows.
//!
//! - Lecture : egui ne sait lire le presse-papiers que sur Ctrl+V ; le collage
//!   au clic droit passe donc par l'API Win32.
//! - Écriture « privée » : vider le presse-papiers ne retire rien de
//!   l'historique Windows (Win+V). Pour qu'un secret n'y entre jamais, il faut
//!   déposer, dans la même ouverture du presse-papiers, le texte et les formats
//!   qui demandent à l'historique et au presse-papiers cloud de l'ignorer.

use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, RegisterClipboardFormatW,
    SetClipboardData,
};
use windows::Win32::System::Memory::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows::core::PCWSTR;

/// Format « texte Unicode » (UTF-16) du presse-papiers.
const CF_UNICODETEXT: u32 = 13;

/// Formats reconnus par l'historique Windows et le presse-papiers cloud : leur
/// seule présence suffit, la valeur transportée est ignorée.
const PRIVACY_MARKERS: [&str; 3] = [
    "ExcludeClipboardContentFromMonitorProcessing",
    "CanIncludeInClipboardHistory",
    "CanUploadToCloudClipboard",
];

#[derive(Debug, thiserror::Error)]
#[error("presse-papiers inaccessible ({step}) : {source}")]
pub struct ClipboardError {
    step: &'static str,
    #[source]
    source: windows::core::Error,
}

fn failed(step: &'static str) -> impl FnOnce(windows::core::Error) -> ClipboardError {
    move |source| ClipboardError { step, source }
}

/// Ferme le presse-papiers quand on sort de la portée, quoi qu'il arrive.
struct OpenedClipboard;

impl OpenedClipboard {
    fn open() -> Result<Self, ClipboardError> {
        // SAFETY : aucun pointeur en jeu ; l'ouverture est refermée par Drop.
        unsafe { OpenClipboard(None) }
            .map_err(failed("ouverture, verrouillé par une autre application"))?;
        Ok(Self)
    }
}

impl Drop for OpenedClipboard {
    fn drop(&mut self) {
        // SAFETY : construit uniquement après un OpenClipboard réussi.
        if let Err(error) = unsafe { CloseClipboard() } {
            tracing::warn!(%error, "failed to close the clipboard");
        }
    }
}

/// Texte du presse-papiers, ou `None` s'il est vide, non textuel ou verrouillé
/// par une autre application.
pub fn read_text() -> Option<String> {
    let _clipboard = OpenedClipboard::open().ok()?;
    // SAFETY : presse-papiers ouvert ; le bloc mémoire est déverrouillé avant
    // de rendre la main et n'est lu que dans la limite de sa taille.
    unsafe {
        let handle = GetClipboardData(CF_UNICODETEXT).ok()?;
        let memory = HGLOBAL(handle.0);
        let pointer = GlobalLock(memory) as *const u16;
        if pointer.is_null() {
            return None;
        }
        let capacity = GlobalSize(memory) / std::mem::size_of::<u16>();
        let units = std::slice::from_raw_parts(pointer, capacity);
        let length = units.iter().position(|&unit| unit == 0).unwrap_or(capacity);
        let text = String::from_utf16_lossy(&units[..length]);
        // Le déverrouillage « échoue » quand le compteur retombe à zéro : c'est l'issue normale.
        let _ = GlobalUnlock(memory);
        Some(text)
    }
}

/// Copie dans un bloc mémoire global, cédé au système par `SetClipboardData`.
fn global_block(bytes: &[u8]) -> Result<HGLOBAL, ClipboardError> {
    // SAFETY : bloc alloué à la bonne taille, copié pendant qu'il est verrouillé.
    unsafe {
        let memory = GlobalAlloc(GMEM_MOVEABLE, bytes.len()).map_err(failed("allocation"))?;
        let pointer = GlobalLock(memory) as *mut u8;
        if pointer.is_null() {
            let _ = GlobalFree(Some(memory));
            return Err(ClipboardError {
                step: "verrouillage mémoire",
                source: windows::core::Error::from_thread(),
            });
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer, bytes.len());
        let _ = GlobalUnlock(memory);
        Ok(memory)
    }
}

/// Dépose un bloc ; en cas de refus, le bloc nous appartient encore et est libéré.
fn set_data(format: u32, bytes: &[u8], step: &'static str) -> Result<(), ClipboardError> {
    let memory = global_block(bytes)?;
    // SAFETY : presse-papiers ouvert par l'appelant ; après succès, le système
    // possède le bloc, il ne doit plus être libéré ici.
    unsafe {
        if let Err(source) = SetClipboardData(format, Some(HANDLE(memory.0))) {
            let _ = GlobalFree(Some(memory));
            return Err(ClipboardError { step, source });
        }
    }
    Ok(())
}

/// Écrit `text` en le tenant hors de l'historique Windows et du cloud.
pub fn write_private(text: &str) -> Result<(), ClipboardError> {
    let _clipboard = OpenedClipboard::open()?;
    // SAFETY : presse-papiers ouvert juste au-dessus.
    unsafe { EmptyClipboard() }.map_err(failed("vidage"))?;

    let utf16: Vec<u8> = text
        .encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect();
    set_data(CF_UNICODETEXT, &utf16, "écriture du texte")?;

    for marker in PRIVACY_MARKERS {
        let name: Vec<u16> = marker.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY : nom terminé par un zéro, vivant pendant l'appel.
        let format = unsafe { RegisterClipboardFormatW(PCWSTR(name.as_ptr())) };
        if format == 0 {
            return Err(ClipboardError {
                step: "enregistrement d'un marqueur de confidentialité",
                source: windows::core::Error::from_thread(),
            });
        }
        set_data(
            format,
            &[0, 0, 0, 0],
            "écriture d'un marqueur de confidentialité",
        )?;
    }
    Ok(())
}

/// Vide le presse-papiers s'il contient encore `expected` : une copie faite
/// ensuite par l'utilisateur n'est jamais effacée. Renvoie vrai si vidé.
pub fn clear_if_unchanged(expected: &str) -> Result<bool, ClipboardError> {
    if read_text().as_deref() != Some(expected) {
        return Ok(false);
    }
    let _clipboard = OpenedClipboard::open()?;
    // SAFETY : presse-papiers ouvert juste au-dessus.
    unsafe { EmptyClipboard() }.map_err(failed("vidage"))?;
    Ok(true)
}

/// Touche au vrai presse-papiers de la session : lancé à la demande seulement
/// (`cargo test -- --ignored`), le contenu d'origine est restauré à la fin.
#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::DataExchange::IsClipboardFormatAvailable;

    fn has_format(name: &str) -> bool {
        let name: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY : nom terminé par un zéro ; la vérification n'ouvre pas le presse-papiers.
        unsafe {
            let format = RegisterClipboardFormatW(PCWSTR(name.as_ptr()));
            IsClipboardFormatAvailable(format).is_ok()
        }
    }

    #[test]
    #[ignore = "modifie le presse-papiers de la session Windows"]
    fn should_write_privately_then_clear_only_the_same_text() {
        let original = read_text();

        write_private("secret-de-test-é").unwrap();
        assert_eq!(read_text().as_deref(), Some("secret-de-test-é"));
        assert!(
            PRIVACY_MARKERS.iter().all(|marker| has_format(marker)),
            "marqueurs Win+V présents"
        );

        assert!(
            !clear_if_unchanged("autre chose").unwrap(),
            "une autre valeur n'est pas effacée"
        );
        assert!(clear_if_unchanged("secret-de-test-é").unwrap());
        assert_eq!(read_text(), None);

        if let Some(text) = original {
            write_private(&text).unwrap();
        }
    }
}
