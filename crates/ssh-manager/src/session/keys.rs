//! Traduction des touches en octets envoyés au serveur (conventions xterm).

use egui::{Key, Modifiers};

/// Octets d'une touche spéciale ou d'une combinaison Ctrl, ou `None` si la
/// touche produit du texte ordinaire (transmis par ailleurs) ou n'a pas d'effet.
///
/// `application_cursor` : mode « curseur applicatif » (DECCKM) activé par le
/// serveur — vim, less… attendent alors `ESC O A` au lieu de `ESC [ A`.
pub fn encode(key: Key, modifiers: Modifiers, application_cursor: bool) -> Option<Vec<u8>> {
    if modifiers.ctrl
        && !modifiers.alt
        && let Some(byte) = control_byte(key)
    {
        return Some(vec![byte]);
    }

    let cursor = |final_byte: u8| -> Vec<u8> {
        match (modifier_parameter(modifiers), application_cursor) {
            (Some(parameter), _) => {
                format!("\x1b[1;{parameter}{}", final_byte as char).into_bytes()
            }
            (None, true) => vec![0x1b, b'O', final_byte],
            (None, false) => vec![0x1b, b'[', final_byte],
        }
    };
    let tilde = |code: u8| -> Vec<u8> {
        match modifier_parameter(modifiers) {
            Some(parameter) => format!("\x1b[{code};{parameter}~").into_bytes(),
            None => format!("\x1b[{code}~").into_bytes(),
        }
    };

    let bytes = match key {
        Key::Enter => b"\r".to_vec(),
        Key::Backspace => {
            if modifiers.ctrl {
                vec![0x08]
            } else {
                vec![0x7f]
            }
        }
        Key::Tab => {
            if modifiers.shift {
                b"\x1b[Z".to_vec()
            } else {
                b"\t".to_vec()
            }
        }
        Key::Escape => vec![0x1b],
        Key::ArrowUp => cursor(b'A'),
        Key::ArrowDown => cursor(b'B'),
        Key::ArrowRight => cursor(b'C'),
        Key::ArrowLeft => cursor(b'D'),
        Key::Home => cursor(b'H'),
        Key::End => cursor(b'F'),
        Key::Insert => tilde(2),
        Key::Delete => tilde(3),
        Key::PageUp => tilde(5),
        Key::PageDown => tilde(6),
        Key::F1 => b"\x1bOP".to_vec(),
        Key::F2 => b"\x1bOQ".to_vec(),
        Key::F3 => b"\x1bOR".to_vec(),
        Key::F4 => b"\x1bOS".to_vec(),
        Key::F5 => tilde(15),
        Key::F6 => tilde(17),
        Key::F7 => tilde(18),
        Key::F8 => tilde(19),
        Key::F9 => tilde(20),
        Key::F10 => tilde(21),
        Key::F11 => tilde(23),
        Key::F12 => tilde(24),
        _ => return None,
    };
    Some(bytes)
}

/// Paramètre xterm des modificateurs : 1 + Maj(1) + Alt(2) + Ctrl(4), absent sans modificateur.
fn modifier_parameter(modifiers: Modifiers) -> Option<u8> {
    let value =
        u8::from(modifiers.shift) + 2 * u8::from(modifiers.alt) + 4 * u8::from(modifiers.ctrl);
    (value > 0).then_some(value + 1)
}

/// Ctrl+lettre → caractère de contrôle (Ctrl+A = 0x01 … Ctrl+Z = 0x1A).
fn control_byte(key: Key) -> Option<u8> {
    let name = key.name();
    if name.len() == 1 {
        let letter = name.as_bytes()[0].to_ascii_lowercase();
        if letter.is_ascii_lowercase() {
            return Some(letter - b'a' + 1);
        }
    }
    match key {
        Key::OpenBracket => Some(0x1b),
        Key::Backslash => Some(0x1c),
        Key::CloseBracket => Some(0x1d),
        Key::Space => Some(0x00),
        _ => None,
    }
}

/// Texte collé, encadré si le serveur a activé le collage délimité : un shell
/// moderne n'exécute alors rien avant la validation explicite.
pub fn paste(text: &str, bracketed: bool) -> Vec<u8> {
    // Un texte copié se termine souvent par un saut de ligne (ligne entière
    // sélectionnée) : collé tel quel, il validerait la commande et le curseur
    // passerait à la ligne suivante. On le retire pour coller sur la ligne du prompt.
    let text = text.trim_end_matches(['\r', '\n']);
    // Les fins de ligne Windows deviendraient des lignes vides dans le shell distant.
    let normalized = text.replace("\r\n", "\r").replace('\n', "\r");
    if bracketed {
        // Un texte collé ne doit pas pouvoir refermer le délimiteur lui-même.
        let safe = normalized.replace("\x1b[201~", "");
        format!("\x1b[200~{safe}\x1b[201~").into_bytes()
    } else {
        normalized.into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: Modifiers = Modifiers::NONE;

    #[test]
    fn should_send_carriage_return_for_enter() {
        assert_eq!(encode(Key::Enter, NONE, false), Some(b"\r".to_vec()));
    }

    #[test]
    fn should_send_delete_for_backspace() {
        assert_eq!(encode(Key::Backspace, NONE, false), Some(vec![0x7f]));
    }

    #[test]
    fn should_follow_the_application_cursor_mode() {
        assert_eq!(encode(Key::ArrowUp, NONE, false), Some(b"\x1b[A".to_vec()));
        assert_eq!(encode(Key::ArrowUp, NONE, true), Some(b"\x1bOA".to_vec()));
    }

    #[test]
    fn should_encode_ctrl_letters_as_control_characters() {
        assert_eq!(encode(Key::C, Modifiers::CTRL, false), Some(vec![0x03]));
        assert_eq!(encode(Key::D, Modifiers::CTRL, false), Some(vec![0x04]));
    }

    #[test]
    fn should_add_modifier_parameters_to_arrows() {
        assert_eq!(
            encode(Key::ArrowRight, Modifiers::CTRL, false),
            Some(b"\x1b[1;5C".to_vec())
        );
    }

    #[test]
    fn should_leave_plain_letters_to_text_input() {
        assert_eq!(encode(Key::A, NONE, false), None);
    }

    #[test]
    fn should_normalize_windows_line_endings_when_pasting() {
        assert_eq!(paste("a\r\nb\nc", false), b"a\rb\rc".to_vec());
    }

    #[test]
    fn should_drop_trailing_line_breaks_so_the_paste_stays_on_the_prompt() {
        assert_eq!(paste("ls -la\r\n", false), b"ls -la".to_vec());
        assert_eq!(
            paste("ls -la\n\n", true),
            b"\x1b[200~ls -la\x1b[201~".to_vec()
        );
    }

    #[test]
    fn should_bracket_pastes_and_strip_an_embedded_terminator() {
        assert_eq!(paste("x\x1b[201~y", true), b"\x1b[200~xy\x1b[201~".to_vec());
    }
}
