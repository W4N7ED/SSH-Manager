//! Liens web affichés dans le terminal : repérage sous le pointeur, et
//! ouverture dans le navigateur par défaut (Ctrl+clic).

use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HSTRING, w};

const SCHEMES: [&str; 2] = ["https://", "http://"];
/// Caractères qui terminent une adresse dans un texte de terminal.
const DELIMITERS: &[char] = &[' ', '\t', '"', '\'', '<', '>', '`', '|', '{', '}', '[', ']'];
/// Ponctuation finale qui appartient à la phrase, pas à l'adresse.
const TRAILING_PUNCTUATION: &[char] = &['.', ',', ';', ':', '!', '?', ')'];

/// Adresse web qui couvre la colonne `column` (en caractères) de `line`.
pub fn url_at(line: &str, column: usize) -> Option<String> {
    let characters: Vec<char> = line.chars().collect();
    let text: String = characters.iter().collect();
    for scheme in SCHEMES {
        let mut search_from = 0;
        while let Some(found) = text[search_from..].find(scheme) {
            let start_byte = search_from + found;
            let start = text[..start_byte].chars().count();
            let mut end = start;
            while end < characters.len() && !DELIMITERS.contains(&characters[end]) {
                end += 1;
            }
            let mut url: String = characters[start..end].iter().collect();
            // Une parenthèse fermante finale n'est gardée que si elle referme
            // une parenthèse ouverte dans l'adresse (liens Wikipédia…).
            while url.ends_with(TRAILING_PUNCTUATION) {
                let balanced = url.matches('(').count() >= url.matches(')').count();
                if url.ends_with(')') && balanced {
                    break;
                }
                url.pop();
            }
            let url_end = start + url.chars().count();
            if (start..url_end).contains(&column) && url.len() > scheme.len() {
                return Some(url);
            }
            search_from = start_byte + scheme.len();
        }
    }
    None
}

/// Ouvre une adresse http(s) dans le navigateur par défaut. Passer par
/// ShellExecute plutôt que par un shell évite qu'un caractère de l'adresse
/// (« & », « ^ »…) soit interprété comme une commande.
pub fn open(url: &str) {
    if !SCHEMES.iter().any(|scheme| url.starts_with(scheme)) {
        tracing::warn!(%url, "refused to open a non-web link");
        return;
    }
    // SAFETY : chaînes terminées par un zéro, valides pendant l'appel.
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            &HSTRING::from(url),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    // ShellExecute signale un échec par une valeur ≤ 32.
    if result.0 as isize <= 32 {
        tracing::error!(%url, code = result.0 as isize, "failed to open a link in the browser");
    } else {
        tracing::info!(%url, "link opened in the browser");
    }
}

#[cfg(test)]
mod tests {
    use super::url_at;

    #[test]
    fn should_find_the_url_under_the_pointer() {
        let line = "voir https://example.com/docs?a=1&b=2 pour la suite";
        assert_eq!(
            url_at(line, 10).as_deref(),
            Some("https://example.com/docs?a=1&b=2")
        );
        assert_eq!(url_at(line, 2), None);
    }

    #[test]
    fn should_leave_trailing_punctuation_out() {
        assert_eq!(
            url_at("Docs: http://a.io/x.", 8).as_deref(),
            Some("http://a.io/x")
        );
        assert_eq!(
            url_at("(https://w.org/wiki/Rust_(langage))", 3).as_deref(),
            Some("https://w.org/wiki/Rust_(langage)")
        );
    }

    #[test]
    fn should_count_columns_in_characters() {
        let line = "éàù https://é.fr";
        assert_eq!(url_at(line, 5).as_deref(), Some("https://é.fr"));
    }

    #[test]
    fn should_ignore_a_bare_scheme() {
        assert_eq!(url_at("https:// alone", 2), None);
    }
}
