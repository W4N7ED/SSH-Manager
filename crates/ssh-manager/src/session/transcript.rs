//! Extraction du texte lisible d'un flux de terminal, pour l'enregistrement.
//!
//! On ne reproduit pas l'écran : on suit la ligne en cours d'écriture, comme le
//! ferait une imprimante. Retour chariot, retour arrière, effacement de fin de
//! ligne et déplacements horizontaux suffisent pour qu'un prompt édité par
//! readline donne la commande finale. Les applications plein écran (vim, htop…)
//! passent par l'écran alternatif : leur contenu est remplacé par une mention.

use alacritty_terminal::vte::{Params, Parser, Perform};

use super::utf8_boundary::Utf8Boundary;

const TAB_WIDTH: usize = 8;
/// Modes privés DEC qui basculent vers l'écran alternatif.
const ALTERNATE_SCREEN_MODES: [u16; 3] = [47, 1047, 1049];
pub const FULL_SCREEN_NOTE: &str = "[application plein écran]";

/// Transforme des octets de terminal en lignes de texte brut.
pub struct TextExtractor {
    parser: Parser,
    boundary: Utf8Boundary,
    lines: LineBuilder,
}

impl Default for TextExtractor {
    fn default() -> Self {
        Self {
            parser: Parser::new(),
            boundary: Utf8Boundary::default(),
            lines: LineBuilder::default(),
        }
    }
}

impl TextExtractor {
    /// Interprète des octets ; renvoie les lignes terminées par ce passage.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<String> {
        let complete = self.boundary.complete(bytes);
        self.parser.advance(&mut self.lines, &complete);
        std::mem::take(&mut self.lines.completed)
    }

    /// Ligne en cours (souvent le prompt), sans fin de ligne.
    pub fn pending_line(&self) -> String {
        self.lines.current_text()
    }

    /// Termine la ligne en cours, s'il y en a une, et la renvoie.
    pub fn flush_pending(&mut self) -> Option<String> {
        let text = self.lines.current_text();
        self.lines.clear_line();
        (!text.is_empty()).then_some(text)
    }
}

#[derive(Default)]
struct LineBuilder {
    line: Vec<char>,
    cursor: usize,
    completed: Vec<String>,
    alternate_screen: bool,
}

impl LineBuilder {
    fn current_text(&self) -> String {
        self.line.iter().collect::<String>().trim_end().to_owned()
    }

    fn clear_line(&mut self) {
        self.line.clear();
        self.cursor = 0;
    }

    fn finish_line(&mut self) {
        let text = self.current_text();
        self.completed.push(text);
        self.clear_line();
    }

    fn move_to(&mut self, column: usize) {
        self.cursor = column;
        if self.line.len() < column {
            self.line.resize(column, ' ');
        }
    }

    fn put(&mut self, character: char) {
        if self.cursor < self.line.len() {
            self.line[self.cursor] = character;
        } else {
            self.move_to(self.cursor);
            self.line.push(character);
        }
        self.cursor += 1;
    }

    fn set_alternate_screen(&mut self, enabled: bool) {
        if enabled == self.alternate_screen {
            return;
        }
        if enabled {
            if !self.current_text().is_empty() {
                self.finish_line();
            }
            self.completed.push(FULL_SCREEN_NOTE.to_owned());
        }
        self.clear_line();
        self.alternate_screen = enabled;
    }
}

/// Premier paramètre d'une séquence CSI, avec sa valeur par défaut.
fn first_param(params: &Params, default: u16) -> usize {
    let value = params
        .iter()
        .next()
        .and_then(|values| values.first().copied())
        .unwrap_or(0);
    usize::from(if value == 0 { default } else { value })
}

impl Perform for LineBuilder {
    fn print(&mut self, character: char) {
        if !self.alternate_screen {
            self.put(character);
        }
    }

    fn execute(&mut self, byte: u8) {
        if self.alternate_screen {
            return;
        }
        match byte {
            b'\n' => self.finish_line(),
            b'\r' => self.cursor = 0,
            0x08 => self.cursor = self.cursor.saturating_sub(1),
            b'\t' => self.move_to((self.cursor / TAB_WIDTH + 1) * TAB_WIDTH),
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        if intermediates == b"?" && matches!(action, 'h' | 'l') {
            let switches_screen = params.iter().any(|values| {
                values
                    .iter()
                    .any(|mode| ALTERNATE_SCREEN_MODES.contains(mode))
            });
            if switches_screen {
                self.set_alternate_screen(action == 'h');
            }
            return;
        }
        if self.alternate_screen || !intermediates.is_empty() {
            return;
        }
        match action {
            // Effacement : 0 = jusqu'à la fin, 1 = jusqu'au curseur, 2 = toute la ligne.
            'K' => match params
                .iter()
                .next()
                .and_then(|values| values.first().copied())
                .unwrap_or(0)
            {
                0 => self.line.truncate(self.cursor),
                1 => self
                    .line
                    .iter_mut()
                    .take(self.cursor + 1)
                    .for_each(|cell| *cell = ' '),
                _ => self.line.clear(),
            },
            'C' => self.move_to(self.cursor + first_param(params, 1)),
            'D' => self.cursor = self.cursor.saturating_sub(first_param(params, 1)),
            'G' => self.move_to(first_param(params, 1) - 1),
            'P' => {
                let end = self.line.len().min(self.cursor + first_param(params, 1));
                if self.cursor < end {
                    self.line.drain(self.cursor..end);
                }
            }
            '@' => {
                let count = first_param(params, 1);
                if self.cursor <= self.line.len() {
                    self.line
                        .splice(self.cursor..self.cursor, std::iter::repeat_n(' ', count));
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(bytes: &[u8]) -> Vec<String> {
        TextExtractor::default().feed(bytes)
    }

    #[test]
    fn should_split_plain_output_into_lines() {
        assert_eq!(lines(b"hello\r\nworld\r\n"), ["hello", "world"]);
    }

    #[test]
    fn should_drop_colour_sequences() {
        assert_eq!(
            lines(b"\x1b[1;32muser@host\x1b[0m:~$ ls\r\n"),
            ["user@host:~$ ls"]
        );
    }

    #[test]
    fn should_apply_backspaces_like_the_screen_does() {
        assert_eq!(lines(b"$ lss\x08 \x08\r\n"), ["$ ls"]);
    }

    #[test]
    fn should_keep_the_line_rewritten_after_a_carriage_return() {
        assert_eq!(lines(b"progress 10%\rprogress 100%\r\n"), ["progress 100%"]);
    }

    #[test]
    fn should_erase_to_the_end_of_line() {
        assert_eq!(lines(b"$ long command\r$ ls\x1b[K\r\n"), ["$ ls"]);
    }

    #[test]
    fn should_follow_cursor_moves_from_line_editing() {
        // readline : « gti », retour de deux colonnes, insertion corrigée.
        assert_eq!(lines(b"$ gti\x1b[2D\x1b[1@i\x1b[1C\x1b[1P\r\n"), ["$ git"]);
    }

    #[test]
    fn should_expand_tabs_to_the_next_stop() {
        assert_eq!(lines(b"a\tb\r\n"), ["a       b"]);
    }

    #[test]
    fn should_replace_full_screen_applications_with_a_note() {
        let output = lines(b"$ vim\r\n\x1b[?1049hHIDDEN\r\nCONTENT\x1b[?1049l$ exit\r\n");
        assert_eq!(output, ["$ vim", FULL_SCREEN_NOTE, "$ exit"]);
    }

    #[test]
    fn should_keep_the_prompt_as_a_pending_line() {
        let mut extractor = TextExtractor::default();
        assert!(extractor.feed(b"done\r\nuser@host:~$ ").len() == 1);
        assert_eq!(extractor.pending_line(), "user@host:~$");
        assert_eq!(extractor.flush_pending().as_deref(), Some("user@host:~$"));
        assert_eq!(extractor.flush_pending(), None);
    }

    #[test]
    fn should_decode_utf8_split_across_chunks() {
        let mut extractor = TextExtractor::default();
        let bytes = "été\r\n".as_bytes();
        assert!(extractor.feed(&bytes[..1]).is_empty());
        assert_eq!(extractor.feed(&bytes[1..]), ["été"]);
    }
}
