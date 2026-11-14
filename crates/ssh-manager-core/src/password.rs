//! Génération de mots de passe.
//!
//! L'aléa vient de `rand::rng()`, un générateur cryptographiquement sûr
//! réensemencé depuis le système. Les tirages bornés de `rand` sont uniformes :
//! pas de biais de modulo.

use rand::RngExt;
use rand::seq::SliceRandom;

pub const LENGTH_MIN: usize = 8;
pub const LENGTH_MAX: usize = 64;

const LOWERCASE: &str = "abcdefghijklmnopqrstuvwxyz";
const UPPERCASE: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS: &str = "0123456789";
const SYMBOLS: &str = "!@#$%^&*()-_=+[]{}:;,.?/";
/// Caractères que l'on confond à la lecture (l/1/I, O/0/o).
const AMBIGUOUS: &str = "Il1O0o";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PasswordOptions {
    pub length: usize,
    pub lowercase: bool,
    pub uppercase: bool,
    pub digits: bool,
    pub symbols: bool,
    pub exclude_ambiguous: bool,
}

impl Default for PasswordOptions {
    fn default() -> Self {
        Self {
            length: 20,
            lowercase: true,
            uppercase: true,
            digits: true,
            symbols: true,
            exclude_ambiguous: false,
        }
    }
}

/// Robustesse qualitative, déduite de l'entropie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strength {
    Weak,
    Fair,
    Strong,
    Excellent,
}

impl Strength {
    pub fn from_entropy(bits: u32) -> Self {
        match bits {
            0..50 => Self::Weak,
            50..70 => Self::Fair,
            70..100 => Self::Strong,
            _ => Self::Excellent,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Weak => "faible",
            Self::Fair => "correcte",
            Self::Strong => "solide",
            Self::Excellent => "excellente",
        }
    }
}

/// Jeux de caractères activés, filtre d'ambiguïté appliqué.
pub fn character_sets(options: &PasswordOptions) -> Vec<Vec<char>> {
    [
        (options.lowercase, LOWERCASE),
        (options.uppercase, UPPERCASE),
        (options.digits, DIGITS),
        (options.symbols, SYMBOLS),
    ]
    .into_iter()
    .filter(|(enabled, _)| *enabled)
    .map(|(_, set)| {
        set.chars()
            .filter(|c| !(options.exclude_ambiguous && AMBIGUOUS.contains(*c)))
            .collect::<Vec<_>>()
    })
    .filter(|set| !set.is_empty())
    .collect()
}

/// Génère un mot de passe où chaque jeu activé apparaît au moins une fois.
/// Renvoie une chaîne vide si aucun jeu n'est activé.
pub fn generate(options: &PasswordOptions) -> String {
    let sets = character_sets(options);
    if sets.is_empty() {
        return String::new();
    }
    let length = options.length.clamp(LENGTH_MIN, LENGTH_MAX);
    let pool: Vec<char> = sets.iter().flatten().copied().collect();
    let mut rng = rand::rng();

    let mut characters: Vec<char> = sets
        .iter()
        .take(length)
        .map(|set| set[rng.random_range(0..set.len())])
        .collect();
    while characters.len() < length {
        characters.push(pool[rng.random_range(0..pool.len())]);
    }
    characters.shuffle(&mut rng);
    characters.into_iter().collect()
}

/// Entropie du procédé de génération, en bits.
pub fn entropy_bits(options: &PasswordOptions) -> u32 {
    let pool_size: usize = character_sets(options).iter().map(Vec::len).sum();
    if pool_size == 0 {
        return 0;
    }
    (options.length as f64 * (pool_size as f64).log2()).round() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contains_any(password: &str, set: &str) -> bool {
        password.chars().any(|c| set.contains(c))
    }

    #[test]
    fn should_honour_the_requested_length() {
        let password = generate(&PasswordOptions {
            length: 33,
            ..Default::default()
        });
        assert_eq!(password.chars().count(), 33);
    }

    #[test]
    fn should_clamp_length_to_the_allowed_range() {
        let password = generate(&PasswordOptions {
            length: 2,
            ..Default::default()
        });
        assert_eq!(password.chars().count(), LENGTH_MIN);
    }

    #[test]
    fn should_include_every_enabled_set() {
        for _ in 0..300 {
            let password = generate(&PasswordOptions {
                length: 12,
                ..Default::default()
            });
            assert!(contains_any(&password, LOWERCASE) && contains_any(&password, UPPERCASE));
            assert!(contains_any(&password, DIGITS) && contains_any(&password, SYMBOLS));
        }
    }

    #[test]
    fn should_never_use_a_disabled_set() {
        for _ in 0..300 {
            let password = generate(&PasswordOptions {
                digits: false,
                symbols: false,
                ..Default::default()
            });
            assert!(!contains_any(&password, DIGITS) && !contains_any(&password, SYMBOLS));
        }
    }

    #[test]
    fn should_exclude_ambiguous_characters_when_asked() {
        for _ in 0..300 {
            let password = generate(&PasswordOptions {
                length: 40,
                exclude_ambiguous: true,
                ..Default::default()
            });
            assert!(!contains_any(&password, AMBIGUOUS));
        }
    }

    #[test]
    fn should_return_empty_when_no_set_is_enabled() {
        let options = PasswordOptions {
            lowercase: false,
            uppercase: false,
            digits: false,
            symbols: false,
            ..Default::default()
        };
        assert_eq!(generate(&options), "");
    }

    #[test]
    fn should_compute_entropy_from_pool_size() {
        // 26 + 26 + 10 + 24 = 86 caractères
        assert_eq!(
            entropy_bits(&PasswordOptions::default()),
            (20.0 * 86f64.log2()).round() as u32
        );
    }

    #[test]
    fn should_rate_the_default_options_as_excellent() {
        assert_eq!(
            Strength::from_entropy(entropy_bits(&PasswordOptions::default())),
            Strength::Excellent
        );
    }

    #[test]
    fn should_draw_characters_uniformly_within_a_single_set() {
        let options = PasswordOptions {
            uppercase: false,
            digits: false,
            symbols: false,
            length: 40,
            ..Default::default()
        };
        let mut counts = std::collections::HashMap::new();
        for _ in 0..4000 {
            for c in generate(&options).chars() {
                *counts.entry(c).or_insert(0usize) += 1;
            }
        }
        let expected = 4000.0 * 40.0 / 26.0;
        let worst = counts
            .values()
            .map(|&n| (n as f64 - expected).abs() / expected)
            .fold(0.0, f64::max);
        assert_eq!(counts.len(), 26);
        assert!(
            worst < 0.06,
            "écart maximal de {:.1} % : biais suspect",
            worst * 100.0
        );
    }
}
