//! Émulation de terminal : transforme le flux d'octets du serveur en grille de
//! cellules. Aucune dépendance au réseau ni à l'interface.

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::Processor;
use tokio::sync::mpsc::UnboundedSender;

/// Historique conservé au-dessus de l'écran, en lignes.
const SCROLLBACK_LINES: usize = 10_000;

/// Taille de la grille en cellules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridSize {
    pub columns: u16,
    pub rows: u16,
}

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.screen_lines()
    }

    fn screen_lines(&self) -> usize {
        usize::from(self.rows)
    }

    fn columns(&self) -> usize {
        usize::from(self.columns)
    }
}

/// Reçoit les évènements du terminal. Seules les réponses que l'émulateur doit
/// renvoyer au serveur (identification, position du curseur…) nous intéressent.
pub struct ResponseSink {
    responses: Option<UnboundedSender<Vec<u8>>>,
}

impl EventListener for ResponseSink {
    fn send_event(&self, event: Event) {
        if let (Event::PtyWrite(text), Some(responses)) = (event, &self.responses) {
            // Fermé seulement si la session s'est déjà arrêtée : rien à renvoyer alors.
            let _ = responses.send(text.into_bytes());
        }
    }
}

pub struct Emulator {
    term: Term<ResponseSink>,
    parser: Processor,
    size: GridSize,
}

impl Emulator {
    /// `responses` reçoit les octets que le terminal doit renvoyer au serveur.
    pub fn new(size: GridSize, responses: Option<UnboundedSender<Vec<u8>>>) -> Self {
        let config = Config {
            scrolling_history: SCROLLBACK_LINES,
            ..Config::default()
        };
        Self {
            term: Term::new(config, &size, ResponseSink { responses }),
            parser: Processor::new(),
            size,
        }
    }

    /// Interprète des octets reçus du serveur.
    pub fn advance(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.term, bytes);
    }

    pub fn size(&self) -> GridSize {
        self.size
    }

    /// Redimensionne la grille ; renvoie vrai si la taille a changé.
    pub fn resize(&mut self, size: GridSize) -> bool {
        if size == self.size || size.columns == 0 || size.rows == 0 {
            return false;
        }
        self.size = size;
        self.term.resize(size);
        true
    }

    pub fn term(&self) -> &Term<ResponseSink> {
        &self.term
    }

    pub fn term_mut(&mut self) -> &mut Term<ResponseSink> {
        &mut self.term
    }

    pub fn mode(&self) -> TermMode {
        *self.term.mode()
    }

    pub fn scroll(&mut self, lines: i32) {
        self.term.scroll_display(Scroll::Delta(lines));
    }

    pub fn scroll_to_bottom(&mut self) {
        self.term.scroll_display(Scroll::Bottom);
    }

    /// Texte visible de l'écran, une ligne par rangée, espaces de fin retirés.
    #[cfg(test)]
    pub fn screen_text(&self) -> Vec<String> {
        let mut rows = vec![String::new(); usize::from(self.size.rows)];
        for indexed in self.term.grid().display_iter() {
            let row = indexed.point.line.0 + self.term.grid().display_offset() as i32;
            if let Some(text) = usize::try_from(row).ok().and_then(|row| rows.get_mut(row)) {
                text.push(indexed.c);
            }
        }
        rows.into_iter()
            .map(|row| row.trim_end().to_owned())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn emulator() -> Emulator {
        Emulator::new(
            GridSize {
                columns: 20,
                rows: 4,
            },
            None,
        )
    }

    #[test]
    fn should_render_plain_text() {
        let mut emulator = emulator();
        emulator.advance(b"hello\r\nworld");
        assert_eq!(&emulator.screen_text()[..2], ["hello", "world"]);
    }

    #[test]
    fn should_interpret_cursor_movement() {
        let mut emulator = emulator();
        emulator.advance(b"abc\x1b[1;2Hz");
        assert_eq!(emulator.screen_text()[0], "azc");
    }

    #[test]
    fn should_clear_the_screen() {
        let mut emulator = emulator();
        emulator.advance(b"junk\x1b[2J\x1b[H");
        assert!(emulator.screen_text().iter().all(String::is_empty));
    }

    #[test]
    fn should_decode_utf8_split_across_chunks() {
        let mut emulator = emulator();
        let bytes = "é".as_bytes();
        emulator.advance(&bytes[..1]);
        emulator.advance(&bytes[1..]);
        assert_eq!(emulator.screen_text()[0], "é");
    }

    #[test]
    fn should_reply_to_a_device_attributes_query() {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let mut emulator = Emulator::new(
            GridSize {
                columns: 20,
                rows: 4,
            },
            Some(sender),
        );
        emulator.advance(b"\x1b[c");
        assert!(
            receiver.try_recv().is_ok(),
            "le terminal doit répondre à l'interrogation DA"
        );
    }

    #[test]
    fn should_track_application_cursor_mode() {
        let mut emulator = emulator();
        emulator.advance(b"\x1b[?1h");
        assert!(emulator.mode().contains(TermMode::APP_CURSOR));
    }

    #[test]
    fn should_ignore_a_resize_to_the_same_size() {
        let mut emulator = emulator();
        assert!(!emulator.resize(GridSize {
            columns: 20,
            rows: 4
        }));
        assert!(emulator.resize(GridSize {
            columns: 30,
            rows: 4
        }));
    }
}
