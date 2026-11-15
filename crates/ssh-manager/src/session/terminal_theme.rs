//! Thèmes du terminal : fond, texte et curseur, comme dans l'application
//! Electron. Les 16 couleurs ANSI restent celles de la palette Minuit.

use egui::Color32;
use serde::{Deserialize, Serialize};

pub const CUSTOM_ID: &str = "custom";

/// Couleurs effectivement utilisées pour peindre un terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalColors {
    pub background: Color32,
    pub foreground: Color32,
    pub cursor: Color32,
}

pub struct Preset {
    pub id: &'static str,
    pub label: &'static str,
    pub colors: TerminalColors,
}

const fn colors(background: u32, foreground: u32, cursor: u32) -> TerminalColors {
    const fn rgb(value: u32) -> Color32 {
        Color32::from_rgb((value >> 16) as u8, (value >> 8) as u8, value as u8)
    }
    TerminalColors {
        background: rgb(background),
        foreground: rgb(foreground),
        cursor: rgb(cursor),
    }
}

/// Mêmes thèmes et mêmes valeurs que l'ancienne application.
pub const PRESETS: [Preset; 8] = [
    Preset {
        id: "minuit",
        label: "Minuit",
        colors: colors(0x0e0b16, 0xd9d4e8, 0x9a6bff),
    },
    Preset {
        id: "github-dark",
        label: "GitHub Dark",
        colors: colors(0x0d1117, 0xe6edf3, 0x58a6ff),
    },
    Preset {
        id: "dracula",
        label: "Dracula",
        colors: colors(0x282a36, 0xf8f8f2, 0xff79c6),
    },
    Preset {
        id: "monokai",
        label: "Monokai",
        colors: colors(0x272822, 0xf8f8f2, 0xf8f8f0),
    },
    Preset {
        id: "solarized-dark",
        label: "Solarized Dark",
        colors: colors(0x002b36, 0x839496, 0x268bd2),
    },
    Preset {
        id: "nord",
        label: "Nord",
        colors: colors(0x2e3440, 0xd8dee9, 0x88c0d0),
    },
    Preset {
        id: "one-dark",
        label: "One Dark",
        colors: colors(0x282c34, 0xabb2bf, 0x528bff),
    },
    Preset {
        id: "light",
        label: "Clair",
        colors: colors(0xffffff, 0x24292f, 0x0969da),
    },
];

pub const MINUIT: TerminalColors = PRESETS[0].colors;

fn to_hex(color: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r(), color.g(), color.b())
}

fn parse_hex(text: &str) -> Option<Color32> {
    let hex = text.strip_prefix('#')?;
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let channel = |index: usize| u8::from_str_radix(&hex[index..index + 2], 16).ok();
    Some(Color32::from_rgb(channel(0)?, channel(2)?, channel(4)?))
}

/// Thème choisi, tel qu'enregistré dans les réglages (couleurs `#rrggbb`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalTheme {
    pub id: String,
    pub background: String,
    pub foreground: String,
    pub cursor: String,
}

impl Default for TerminalTheme {
    fn default() -> Self {
        Self::from_preset(&PRESETS[0])
    }
}

impl TerminalTheme {
    pub fn from_preset(preset: &Preset) -> Self {
        Self::with_colors(preset.id, preset.colors)
    }

    pub fn custom(colors: TerminalColors) -> Self {
        Self::with_colors(CUSTOM_ID, colors)
    }

    fn with_colors(id: &str, colors: TerminalColors) -> Self {
        Self {
            id: id.to_owned(),
            background: to_hex(colors.background),
            foreground: to_hex(colors.foreground),
            cursor: to_hex(colors.cursor),
        }
    }

    pub fn preset(id: &str) -> Option<&'static Preset> {
        PRESETS.iter().find(|preset| preset.id == id)
    }

    /// Couleurs à peindre ; une valeur illisible retombe sur celle de Minuit.
    pub fn colors(&self) -> TerminalColors {
        TerminalColors {
            background: parse_hex(&self.background).unwrap_or(MINUIT.background),
            foreground: parse_hex(&self.foreground).unwrap_or(MINUIT.foreground),
            cursor: parse_hex(&self.cursor).unwrap_or(MINUIT.cursor),
        }
    }

    pub fn label(&self) -> &str {
        Self::preset(&self.id).map_or("Personnalisé", |preset| preset.label)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_default_to_minuit() {
        assert_eq!(TerminalTheme::default().id, "minuit");
        assert_eq!(TerminalTheme::default().colors(), MINUIT);
    }

    #[test]
    fn should_store_colors_as_hex_like_the_previous_app() {
        let theme = TerminalTheme::from_preset(TerminalTheme::preset("dracula").unwrap());
        assert_eq!(theme.background, "#282a36");
        assert_eq!(theme.cursor, "#ff79c6");
    }

    #[test]
    fn should_round_trip_custom_colors() {
        let chosen = colors(0x101010, 0xfafafa, 0xff0000);
        let theme = TerminalTheme::custom(chosen);
        assert_eq!(theme.label(), "Personnalisé");
        assert_eq!(theme.colors(), chosen);
    }

    #[test]
    fn should_fall_back_to_minuit_for_an_unreadable_color() {
        let theme = TerminalTheme {
            background: "rouge".into(),
            ..TerminalTheme::default()
        };
        assert_eq!(theme.colors().background, MINUIT.background);
    }
}
