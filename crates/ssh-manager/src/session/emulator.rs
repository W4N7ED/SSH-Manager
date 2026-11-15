//! Émulation de terminal : transforme le flux d'octets du serveur en grille de
//! cellules. Aucune dépendance au réseau ni à l'interface.

use alacritty_terminal::event::{Event, EventListener};
use std::sync::{Arc, Mutex};

use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::Processor;
use tokio::sync::mpsc::UnboundedSender;

use super::transport::lock;
use super::utf8_boundary::Utf8Boundary;

type ResponseTarget = Arc<Mutex<Option<UnboundedSender<Vec<u8>>>>>;

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
/// La destination change à chaque reconnexion, d'où le partage.
pub struct ResponseSink {
    responses: ResponseTarget,
}

impl EventListener for ResponseSink {
    fn send_event(&self, event: Event) {
        if let (Event::PtyWrite(text), Some(responses)) = (event, lock(&self.responses).as_ref()) {
            // Fermé seulement si la session s'est déjà arrêtée : rien à renvoyer alors.
            let _ = responses.send(text.into_bytes());
        }
    }
}

pub struct Emulator {
    term: Term<ResponseSink>,
    parser: Processor,
    boundary: Utf8Boundary,
    size: GridSize,
    responses: ResponseTarget,
}

impl Emulator {
    /// `responses` reçoit les octets que le terminal doit renvoyer au serveur.
    pub fn new(size: GridSize, responses: Option<UnboundedSender<Vec<u8>>>) -> Self {
        let config = Config {
            scrolling_history: SCROLLBACK_LINES,
            ..Config::default()
        };
        let responses = Arc::new(Mutex::new(responses));
        Self {
            term: Term::new(
                config,
                &size,
                ResponseSink {
                    responses: responses.clone(),
                },
            ),
            parser: Processor::new(),
            boundary: Utf8Boundary::default(),
            size,
            responses,
        }
    }

    /// Redirige les réponses du terminal vers une nouvelle connexion : l'écran
    /// et l'historique sont conservés d'une reconnexion à l'autre.
    pub fn connect_responses(&self, responses: UnboundedSender<Vec<u8>>) {
        *lock(&self.responses) = Some(responses);
    }

    /// Tout le texte conservé, historique compris, sans les lignes vides de fin.
    pub fn history_text(&self) -> String {
        let grid = self.term.grid();
        let mut lines: Vec<String> = (grid.topmost_line().0..=grid.bottommost_line().0)
            .map(|line| {
                let row = &grid[Line(line)];
                (0..grid.columns())
                    .map(|column| &row[Column(column)])
                    .filter(|cell| !cell.flags.contains(Flags::WIDE_CHAR_SPACER))
                    .map(|cell| cell.c)
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect();
        while lines.last().is_some_and(String::is_empty) {
            lines.pop();
        }
        lines.join("\n")
    }

    /// Interprète des octets reçus du serveur.
    pub fn advance(&mut self, bytes: &[u8]) {
        let complete = self.boundary.complete(bytes);
        self.parser.advance(&mut self.term, &complete);
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
    fn should_keep_the_letter_that_follows_a_split_character() {
        let mut emulator = emulator();
        let bytes = "été".as_bytes();
        emulator.advance(&bytes[..1]);
        emulator.advance(&bytes[1..]);
        assert_eq!(emulator.screen_text()[0], "été");
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
    fn should_export_history_that_scrolled_off_the_screen() {
        let mut emulator = emulator();
        emulator.advance(b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\nsix");
        assert_eq!(emulator.history_text(), "one\ntwo\nthree\nfour\nfive\nsix");
    }

    #[test]
    fn should_send_responses_to_the_latest_connection() {
        let mut emulator = emulator();
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        emulator.connect_responses(sender);
        emulator.advance(b"\x1b[c");
        assert!(receiver.try_recv().is_ok());
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
