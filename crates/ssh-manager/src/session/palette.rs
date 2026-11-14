//! Couleurs du terminal : thème « Minuit » et résolution des couleurs ANSI.

use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::{Color, NamedColor};
use egui::Color32;

pub const BACKGROUND: Color32 = Color32::from_rgb(0x0e, 0x0b, 0x16);
pub const FOREGROUND: Color32 = Color32::from_rgb(0xd9, 0xd4, 0xe8);
pub const CURSOR: Color32 = Color32::from_rgb(0x9a, 0x6b, 0xff);
pub const SELECTION: Color32 = Color32::from_rgba_premultiplied(55, 38, 92, 140);

/// Les 16 couleurs ANSI, accordées à la direction Minuit.
const ANSI: [Color32; 16] = [
    Color32::from_rgb(0x26, 0x20, 0x3a), // noir
    Color32::from_rgb(0xfb, 0x71, 0x85), // rouge
    Color32::from_rgb(0x4a, 0xde, 0x80), // vert
    Color32::from_rgb(0xfb, 0xbf, 0x24), // jaune
    Color32::from_rgb(0x60, 0xa5, 0xfa), // bleu
    Color32::from_rgb(0xa7, 0x8b, 0xfa), // magenta
    Color32::from_rgb(0x7d, 0xd3, 0xfc), // cyan
    Color32::from_rgb(0xd9, 0xd4, 0xe8), // blanc
    Color32::from_rgb(0x73, 0x6c, 0x8d), // noir vif
    Color32::from_rgb(0xfd, 0xa4, 0xaf), // rouge vif
    Color32::from_rgb(0x86, 0xef, 0xac), // vert vif
    Color32::from_rgb(0xfd, 0xe0, 0x47), // jaune vif
    Color32::from_rgb(0x93, 0xc5, 0xfd), // bleu vif
    Color32::from_rgb(0xc4, 0xb5, 0xfd), // magenta vif
    Color32::from_rgb(0xa5, 0xf3, 0xfc), // cyan vif
    Color32::from_rgb(0xf0, 0xed, 0xf7), // blanc vif
];

/// Couleur d'index 16 à 255 de la palette xterm : cube 6×6×6 puis 24 gris.
fn xterm_256(index: u8) -> Color32 {
    match index {
        0..16 => ANSI[usize::from(index)],
        16..232 => {
            let cube = index - 16;
            let level = |value: u8| if value == 0 { 0 } else { 55 + value * 40 };
            Color32::from_rgb(level(cube / 36), level((cube / 6) % 6), level(cube % 6))
        }
        232.. => {
            let gray = 8 + (index - 232) * 10;
            Color32::from_rgb(gray, gray, gray)
        }
    }
}

fn dim(color: Color32) -> Color32 {
    Color32::from_rgb(
        (color.r() as f32 * 0.66) as u8,
        (color.g() as f32 * 0.66) as u8,
        (color.b() as f32 * 0.66) as u8,
    )
}

fn named(color: NamedColor) -> Color32 {
    match color {
        NamedColor::Foreground | NamedColor::BrightForeground => FOREGROUND,
        NamedColor::Background => BACKGROUND,
        NamedColor::Cursor => CURSOR,
        NamedColor::DimForeground => dim(FOREGROUND),
        other => {
            let index = other as usize;
            if index < 16 {
                ANSI[index]
            } else {
                dim(ANSI[(index - NamedColor::DimBlack as usize) % 8])
            }
        }
    }
}

fn resolve(color: Color) -> Color32 {
    match color {
        Color::Named(name) => named(name),
        Color::Spec(rgb) => Color32::from_rgb(rgb.r, rgb.g, rgb.b),
        Color::Indexed(index) => xterm_256(index),
    }
}

/// Couleurs effectives d'une cellule, attributs (gras, atténué, inversé) appliqués.
pub fn cell_colors(fg: Color, bg: Color, flags: Flags) -> (Color32, Color32) {
    // Comme la plupart des terminaux : le gras éclaircit les 8 couleurs de base.
    let fg = match fg {
        Color::Named(name) if flags.contains(Flags::BOLD) && (name as usize) < 8 => {
            Color::Named(name.to_bright())
        }
        other => other,
    };
    let mut foreground = resolve(fg);
    let mut background = resolve(bg);
    if flags.contains(Flags::DIM) {
        foreground = dim(foreground);
    }
    if flags.contains(Flags::INVERSE) {
        std::mem::swap(&mut foreground, &mut background);
    }
    if flags.contains(Flags::HIDDEN) {
        foreground = background;
    }
    (foreground, background)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_map_the_cube_corners() {
        assert_eq!(xterm_256(16), Color32::from_rgb(0, 0, 0));
        assert_eq!(xterm_256(231), Color32::from_rgb(255, 255, 255));
    }

    #[test]
    fn should_map_the_grayscale_ramp() {
        assert_eq!(xterm_256(232), Color32::from_rgb(8, 8, 8));
        assert_eq!(xterm_256(255), Color32::from_rgb(238, 238, 238));
    }

    #[test]
    fn should_swap_colors_when_inverse() {
        let (fg, bg) = cell_colors(
            Color::Named(NamedColor::Foreground),
            Color::Named(NamedColor::Background),
            Flags::INVERSE,
        );
        assert_eq!((fg, bg), (BACKGROUND, FOREGROUND));
    }

    #[test]
    fn should_brighten_bold_base_colors() {
        let (fg, _) = cell_colors(
            Color::Named(NamedColor::Red),
            Color::Named(NamedColor::Background),
            Flags::BOLD,
        );
        assert_eq!(fg, ANSI[9]);
    }

    #[test]
    fn should_use_true_colors_as_is() {
        let rgb = alacritty_terminal::vte::ansi::Rgb { r: 1, g: 2, b: 3 };
        let (fg, _) = cell_colors(
            Color::Spec(rgb),
            Color::Named(NamedColor::Background),
            Flags::empty(),
        );
        assert_eq!(fg, Color32::from_rgb(1, 2, 3));
    }
}
