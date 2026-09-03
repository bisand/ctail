//! ctail's palettes on DeniseUI's semantic theme: the same 21 themes (and
//! custom ones) the other front ends use, expressed as nine seed colours.

use ctail_core::ThemeColors;
use denise::theme::{ColorScheme, Theme};
use denise::Color;

/// Parses "#rrggbb" (or "#rgb"); anything else is mid grey.
pub fn hex(s: &str) -> Color {
    let h = s.trim().trim_start_matches('#');
    let h = if h.len() == 3 {
        h.chars().flat_map(|c| [c, c]).collect::<String>()
    } else {
        h.to_string()
    };
    u32::from_str_radix(&h, 16)
        .map(Color::from_rgb888)
        .unwrap_or(Color::from_rgb888(0x808080))
}

/// A ctail palette as a Denise theme. The name is leaked once per theme change,
/// which is what a `&'static str` name costs for a runtime-chosen theme.
pub fn from_palette(name: &str, mode: &str, p: &ThemeColors) -> Theme {
    let scheme = if mode == "light" {
        ColorScheme::Light
    } else {
        ColorScheme::Dark
    };
    let name: &'static str = Box::leak(format!("{name}-{mode}").into_boxed_str());
    Theme::from_seeds(
        name,
        scheme,
        hex(&p.bg_primary),
        hex(&p.accent),
        hex(&p.accent_hover),
        hex(&p.accent),
        hex(&p.bg_surface),
        hex(&p.accent),
        hex(&p.success),
        hex(&p.warning),
        hex(&p.danger),
    )
}
