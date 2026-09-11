//! ctail's palettes on DeniseUI's semantic theme: the same 21 themes (and
//! custom ones) the other front ends use, expressed as nine seed colours.

use ctail_core::{effective_theme_mode, resolve_palette, AppSettings, ThemeColors};
use denise::theme::{ColorScheme, Theme};
use denise::Color;
use std::path::Path;

/// Whether the operating system is showing dark mode. One that cannot say —
/// no desktop portal, or no preference set — gets ctail's own default, dark.
pub fn system_is_dark() -> bool {
    !matches!(dark_light::detect(), Ok(dark_light::Mode::Light))
}

/// The theme a window draws `settings` in, at scale 1: its mode as chosen, or
/// the system's when it follows the system.
pub fn for_settings(settings: &AppSettings, themes_dir: &Path) -> Theme {
    let mode = effective_theme_mode(&settings.theme_mode, system_is_dark());
    let palette = resolve_palette(&settings.theme, mode, Some(themes_dir));
    from_palette(&settings.theme, mode, &palette)
}

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
