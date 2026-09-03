//! Theme catalogue: the built-in palettes plus user JSON themes from the
//! config directory (a custom theme with a built-in's name overrides it).

use crate::models::{Theme, ThemeColors};
use std::fs;
use std::path::Path;

#[path = "themes_generated.rs"]
mod themes_generated;

pub use self::themes_generated::built_in_themes;

/// Built-ins plus any `*.json` themes in `custom_dir`.
pub fn all_themes(custom_dir: Option<&Path>) -> Vec<Theme> {
    let mut themes = built_in_themes();
    if let Some(rd) = custom_dir.and_then(|d| fs::read_dir(d).ok()) {
        let mut paths: Vec<_> = rd
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        paths.sort();
        for p in paths {
            let Ok(text) = fs::read_to_string(&p) else {
                continue;
            };
            let Ok(mut t) = serde_json::from_str::<Theme>(&text) else {
                continue;
            };
            t.built_in = false;
            themes.retain(|b| b.name != t.name);
            themes.push(t);
        }
    }
    themes
}

/// Resolves a theme name + mode ("dark" / "light") to a palette, falling back
/// to Catppuccin dark for unknown names.
pub fn resolve_palette(name: &str, mode: &str, custom_dir: Option<&Path>) -> ThemeColors {
    let themes = all_themes(custom_dir);
    let theme = themes
        .iter()
        .find(|t| t.name == name)
        .or_else(|| themes.iter().find(|t| t.name == "catppuccin"))
        .or_else(|| themes.first())
        .expect("built-in themes");
    theme.palette(mode).clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_ins_and_known_values() {
        let all = built_in_themes();
        assert_eq!(all.len(), 21);
        assert!(all
            .iter()
            .all(|t| !t.name.is_empty() && !t.display_name.is_empty() && t.built_in));
        assert_eq!(
            resolve_palette("catppuccin", "dark", None).bg_primary,
            "#1e1e2e"
        );
        assert_eq!(
            resolve_palette("catppuccin", "light", None).bg_primary,
            "#eff1f5"
        );
        assert_eq!(resolve_palette("nord", "dark", None).accent, "#88c0d0");
        assert_eq!(
            resolve_palette("does-not-exist", "dark", None).bg_primary,
            "#1e1e2e"
        );
    }

    #[test]
    fn custom_theme_overrides_built_in() {
        let dir = std::env::temp_dir().join(format!("ctail-themes-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("nord.json"),
            r##"{"name":"nord","displayName":"My Nord","dark":{"bg-primary":"#010203","text-primary":"#ffffff"}}"##,
        )
        .unwrap();
        let p = resolve_palette("nord", "dark", Some(&dir));
        assert_eq!(p.bg_primary, "#010203");
        let all = all_themes(Some(&dir));
        assert_eq!(all.len(), 21, "override replaces, not duplicates");
        let nord = all.iter().find(|t| t.name == "nord").unwrap();
        assert_eq!(nord.display_name, "My Nord");
        assert!(!nord.built_in);
        let _ = fs::remove_dir_all(&dir);
    }
}
