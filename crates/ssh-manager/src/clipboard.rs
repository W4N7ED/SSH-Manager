//! Accès direct au presse-papiers Windows.
//!
//! egui sait écrire dans le presse-papiers mais pas le lire en dehors du
//! raccourci Ctrl+V : le collage au clic droit passe donc par l'API Win32.

use windows::Win32::Foundation::HGLOBAL;
use windows::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard};
use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

/// Format « texte Unicode » (UTF-16) du presse-papiers.
const CF_UNICODETEXT: u32 = 13;

/// Ferme le presse-papiers quand on sort de la portée, quoi qu'il arrive.
struct OpenedClipboard;

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
    // SAFETY : appels Win32 séquentiels ; le presse-papiers ouvert est refermé
    // par `OpenedClipboard`, le bloc mémoire déverrouillé avant de rendre la main.
    unsafe {
        OpenClipboard(None).ok()?;
        let _guard = OpenedClipboard;
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
