//! Recherche dans l'écran et l'historique du terminal (Ctrl+Maj+F).
//!
//! La recherche est littérale et insensible à la casse. Chaque occurrence
//! trouvée est sélectionnée et amenée à l'écran ; la suivante part de là.

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Boundary, Column, Direction, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::search::{Match, RegexSearch};

use super::emulator::Emulator;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchDirection {
    /// Vers le haut : les sorties plus anciennes.
    Older,
    /// Vers le bas : les sorties plus récentes.
    Newer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchOutcome {
    Found,
    NotFound,
}

/// Échappe les caractères spéciaux des expressions régulières : la saisie de
/// l'utilisateur est cherchée telle quelle.
fn escape_regex(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        if "\\.+*?()|[]{}^$#&-~".contains(character) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

#[derive(Default)]
pub struct TerminalSearch {
    pub query: String,
    /// Requête de la dernière occurrence : une nouvelle requête repart du bas.
    searched: String,
    last_match: Option<Match>,
    pub outcome: Option<SearchOutcome>,
}

impl TerminalSearch {
    pub fn find(&mut self, emulator: &mut Emulator, direction: SearchDirection) -> SearchOutcome {
        if self.query != self.searched {
            self.last_match = None;
            self.searched = self.query.clone();
        }
        let outcome = self.search(emulator, direction);
        self.outcome = (!self.query.is_empty()).then_some(outcome);
        outcome
    }

    fn search(&mut self, emulator: &mut Emulator, direction: SearchDirection) -> SearchOutcome {
        if self.query.is_empty() {
            return SearchOutcome::NotFound;
        }
        let Ok(mut regex) = RegexSearch::new(&format!("(?i){}", escape_regex(&self.query))) else {
            return SearchOutcome::NotFound;
        };
        let term = emulator.term_mut();
        let origin = match (&self.last_match, direction) {
            (Some(found), SearchDirection::Older) => found.start().sub(term, Boundary::None, 1),
            (Some(found), SearchDirection::Newer) => found.end().add(term, Boundary::None, 1),
            (None, SearchDirection::Older) => {
                Point::new(term.bottommost_line(), term.last_column())
            }
            (None, SearchDirection::Newer) => Point::new(term.topmost_line(), Column(0)),
        };
        let (side, regex_direction) = match direction {
            SearchDirection::Older => (Side::Right, Direction::Left),
            SearchDirection::Newer => (Side::Left, Direction::Right),
        };
        let Some(found) = term.search_next(&mut regex, origin, regex_direction, side, None) else {
            return SearchOutcome::NotFound;
        };
        let mut selection = Selection::new(SelectionType::Simple, *found.start(), Side::Left);
        selection.update(*found.end(), Side::Right);
        term.selection = Some(selection);
        term.scroll_to_point(*found.start());
        self.last_match = Some(found);
        SearchOutcome::Found
    }

    /// Ferme la recherche : la sélection de l'occurrence disparaît.
    pub fn clear(&mut self, emulator: &mut Emulator) {
        if self.last_match.take().is_some() {
            emulator.term_mut().selection = None;
        }
        self.outcome = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::emulator::GridSize;

    fn emulator_with(text: &[u8]) -> Emulator {
        let mut emulator = Emulator::new(
            GridSize {
                columns: 30,
                rows: 4,
            },
            None,
        );
        emulator.advance(text);
        emulator
    }

    fn selected(emulator: &Emulator) -> String {
        emulator.term().selection_to_string().unwrap_or_default()
    }

    #[test]
    fn should_find_the_most_recent_match_first_then_older_ones() {
        // Six lignes pour quatre à l'écran : la première est dans l'historique.
        let mut emulator = emulator_with(b"Error one\r\nok\r\nok\r\nok\r\nok\r\nerror TWO");
        let mut search = TerminalSearch {
            query: "error".into(),
            ..Default::default()
        };
        assert_eq!(
            search.find(&mut emulator, SearchDirection::Older),
            SearchOutcome::Found
        );
        assert_eq!(selected(&emulator), "error");
        let first_line = emulator
            .term()
            .selection
            .as_ref()
            .unwrap()
            .to_range(emulator.term())
            .unwrap()
            .start
            .line;
        assert_eq!(
            search.find(&mut emulator, SearchDirection::Older),
            SearchOutcome::Found
        );
        let second_line = emulator
            .term()
            .selection
            .as_ref()
            .unwrap()
            .to_range(emulator.term())
            .unwrap()
            .start
            .line;
        assert!(
            second_line < first_line,
            "la recherche remonte dans l'historique"
        );
        assert_eq!(selected(&emulator), "Error", "insensible à la casse");
    }

    #[test]
    fn should_report_a_missing_text() {
        let mut emulator = emulator_with(b"hello");
        let mut search = TerminalSearch {
            query: "absent".into(),
            ..Default::default()
        };
        assert_eq!(
            search.find(&mut emulator, SearchDirection::Older),
            SearchOutcome::NotFound
        );
    }

    #[test]
    fn should_search_regex_characters_literally() {
        let mut emulator = emulator_with(b"cost: $5 (approx.)");
        let mut search = TerminalSearch {
            query: "$5 (approx.)".into(),
            ..Default::default()
        };
        assert_eq!(
            search.find(&mut emulator, SearchDirection::Older),
            SearchOutcome::Found
        );
        assert_eq!(selected(&emulator), "$5 (approx.)");
    }

    #[test]
    fn should_drop_the_selection_when_closed() {
        let mut emulator = emulator_with(b"needle");
        let mut search = TerminalSearch {
            query: "needle".into(),
            ..Default::default()
        };
        search.find(&mut emulator, SearchDirection::Older);
        search.clear(&mut emulator);
        assert!(emulator.term().selection.is_none());
    }
}
