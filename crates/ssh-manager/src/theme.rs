//! Direction graphique « Minuit » : fond violet nuit, surfaces empilées en
//! cartes arrondies, accent violet réservé à trois rôles — onglet actif,
//! action principale, ligne sélectionnée.

use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Stroke, Vec2,
};

pub mod color {
    use egui::Color32;

    pub const BASE: Color32 = Color32::from_rgb(0x15, 0x10, 0x20);
    pub const SURFACE: Color32 = Color32::from_rgb(0x1d, 0x17, 0x30);
    pub const ELEVATED: Color32 = Color32::from_rgb(0x26, 0x20, 0x3a);
    pub const HOVER: Color32 = Color32::from_rgb(0x30, 0x28, 0x48);
    pub const TERMINAL: Color32 = Color32::from_rgb(0x0e, 0x0b, 0x16);

    pub const BORDER: Color32 = Color32::from_rgb(0x33, 0x2b, 0x4a);
    pub const BORDER_SUBTLE: Color32 = Color32::from_rgb(0x28, 0x21, 0x37);

    pub const TEXT: Color32 = Color32::from_rgb(0xf0, 0xed, 0xf7);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(0xa9, 0xa1, 0xc0);
    pub const TEXT_DIM: Color32 = Color32::from_rgb(0x73, 0x6c, 0x8d);

    pub const ACCENT: Color32 = Color32::from_rgb(0x9a, 0x6b, 0xff);
    pub const ACCENT_LIGHT: Color32 = Color32::from_rgb(0xc4, 0xb5, 0xfd);
    /// Texte posé sur un aplat d'accent : le blanc y manque de contraste.
    pub const ON_ACCENT: Color32 = Color32::from_rgb(0x15, 0x0e, 0x26);

    pub const SUCCESS: Color32 = Color32::from_rgb(0x4a, 0xde, 0x80);
    pub const WARNING: Color32 = Color32::from_rgb(0xfb, 0xbf, 0x24);
    pub const DANGER: Color32 = Color32::from_rgb(0xfb, 0x71, 0x85);

    /// Accent translucide, pour un fond teinté au-dessus d'une surface.
    pub fn tint(color: Color32, alpha: u8) -> Color32 {
        Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
    }

    /// `#rrggbb` → couleur ; `None` si la chaîne est mal formée.
    pub fn parse_hex(text: &str) -> Option<Color32> {
        let hex = text.strip_prefix('#')?;
        if hex.len() != 6 {
            return None;
        }
        let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        Some(Color32::from_rgb(channel(0)?, channel(2)?, channel(4)?))
    }
}

pub mod radius {
    pub const LARGE: u8 = 20;
    pub const MEDIUM: u8 = 14;
    pub const SMALL: u8 = 10;
    pub const PILL: u8 = 255;
}

/// Familles nommées en plus des familles par défaut.
pub mod family {
    pub const SANS_SEMIBOLD: &str = "archivo-600";
    pub const SANS_BOLD: &str = "archivo-800";
    pub const MONO_BOLD: &str = "jetbrains-mono-700";
}

pub fn sans(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

pub fn sans_semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(family::SANS_SEMIBOLD.into()))
}

pub fn sans_bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(family::SANS_BOLD.into()))
}

pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

/// Polices empaquetées dans l'exécutable : aucune dépendance au système.
fn fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let mut add = |name: &str, bytes: &'static [u8]| {
        fonts
            .font_data
            .insert(name.to_owned(), FontData::from_static(bytes).into());
    };
    add(
        "archivo-400",
        include_bytes!("../../../assets/fonts/Archivo-400.ttf"),
    );
    add(
        family::SANS_SEMIBOLD,
        include_bytes!("../../../assets/fonts/Archivo-600.ttf"),
    );
    add(
        family::SANS_BOLD,
        include_bytes!("../../../assets/fonts/Archivo-800.ttf"),
    );
    add(
        "jetbrains-mono-400",
        include_bytes!("../../../assets/fonts/JetBrainsMono-400.ttf"),
    );
    add(
        family::MONO_BOLD,
        include_bytes!("../../../assets/fonts/JetBrainsMono-700.ttf"),
    );

    // Les polices d'egui restent en repli : elles couvrent symboles et emoji
    // absents du sous-ensemble latin d'Archivo.
    let insert_first = |fonts: &mut FontDefinitions, family: FontFamily, font: &str| {
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, font.to_owned());
    };
    insert_first(&mut fonts, FontFamily::Proportional, "archivo-400");
    insert_first(&mut fonts, FontFamily::Monospace, "jetbrains-mono-400");
    for (name, primary, fallback) in [
        (
            family::SANS_SEMIBOLD,
            family::SANS_SEMIBOLD,
            FontFamily::Proportional,
        ),
        (
            family::SANS_BOLD,
            family::SANS_BOLD,
            FontFamily::Proportional,
        ),
        (family::MONO_BOLD, family::MONO_BOLD, FontFamily::Monospace),
    ] {
        let mut chain = vec![primary.to_owned()];
        chain.extend(fonts.families.get(&fallback).cloned().unwrap_or_default());
        fonts.families.insert(FontFamily::Name(name.into()), chain);
    }
    fonts
}

pub fn apply(ctx: &egui::Context) {
    ctx.set_fonts(fonts());

    let mut style = (*ctx.global_style()).clone();
    style.spacing.item_spacing = Vec2::new(8.0, 8.0);
    style.spacing.button_padding = Vec2::new(14.0, 7.0);
    style.spacing.interact_size.y = 32.0;
    style.spacing.window_margin = Margin::same(20);
    style.spacing.menu_margin = Margin::same(6);
    style.spacing.scroll.bar_width = 6.0;

    let visuals = &mut style.visuals;
    visuals.dark_mode = true;
    visuals.override_text_color = Some(color::TEXT);
    visuals.panel_fill = color::BASE;
    visuals.window_fill = color::BASE;
    visuals.extreme_bg_color = color::SURFACE;
    visuals.faint_bg_color = color::SURFACE;
    visuals.code_bg_color = color::TERMINAL;
    visuals.window_stroke = Stroke::new(1.0, color::BORDER);
    visuals.window_corner_radius = CornerRadius::same(radius::LARGE);
    visuals.menu_corner_radius = CornerRadius::same(radius::MEDIUM);
    visuals.hyperlink_color = color::ACCENT_LIGHT;
    visuals.warn_fg_color = color::WARNING;
    visuals.error_fg_color = color::DANGER;
    visuals.selection.bg_fill = color::tint(color::ACCENT, 90);
    visuals.selection.stroke = Stroke::new(1.0, color::ACCENT_LIGHT);

    let widget = |fill: Color32, text: Color32| egui::style::WidgetVisuals {
        bg_fill: fill,
        weak_bg_fill: fill,
        bg_stroke: Stroke::NONE,
        corner_radius: CornerRadius::same(radius::SMALL),
        fg_stroke: Stroke::new(1.0, text),
        expansion: 0.0,
    };
    visuals.widgets.noninteractive = widget(color::BASE, color::TEXT_MUTED);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, color::BORDER_SUBTLE);
    visuals.widgets.inactive = widget(color::SURFACE, color::TEXT_MUTED);
    visuals.widgets.hovered = widget(color::ELEVATED, color::TEXT);
    visuals.widgets.active = widget(color::HOVER, color::TEXT);
    visuals.widgets.open = widget(color::ELEVATED, color::TEXT);

    // L'app n'a qu'une apparence : le même style vaut pour les thèmes clair et sombre.
    ctx.all_styles_mut(|target| *target = style.clone());
}
