//! Data model shared by every front end. JSON keys match the original Go app
//! (`internal/config/types.go`) so settings, profile and theme files round-trip
//! unchanged, and every field has a default so missing or unknown keys never
//! fail a load.

use serde::{Deserialize, Serialize};

/// A single highlighting rule.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Rule {
    #[cfg_attr(feature = "ffi", uniffi(default = ""))]
    pub id: String,
    #[cfg_attr(feature = "ffi", uniffi(default = ""))]
    pub name: String,
    #[cfg_attr(feature = "ffi", uniffi(default = ""))]
    pub pattern: String,
    /// "line" styles the whole line; "match" styles each matched substring.
    #[cfg_attr(feature = "ffi", uniffi(default = "match"))]
    pub match_type: String,
    #[cfg_attr(feature = "ffi", uniffi(default = ""))]
    pub foreground: String,
    #[cfg_attr(feature = "ffi", uniffi(default = ""))]
    pub background: String,
    #[cfg_attr(feature = "ffi", uniffi(default = false))]
    pub bold: bool,
    #[cfg_attr(feature = "ffi", uniffi(default = false))]
    pub italic: bool,
    #[cfg_attr(feature = "ffi", uniffi(default = true))]
    pub enabled: bool,
    #[cfg_attr(feature = "ffi", uniffi(default = 0))]
    pub priority: i32,
}

impl Default for Rule {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            pattern: String::new(),
            match_type: "match".into(),
            foreground: String::new(),
            background: String::new(),
            bold: false,
            italic: false,
            enabled: true,
            priority: 0,
        }
    }
}

impl Rule {
    pub fn is_line_level(&self) -> bool {
        self.match_type == "line"
    }
}

/// A named set of highlighting rules.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Profile {
    #[cfg_attr(feature = "ffi", uniffi(default = ""))]
    pub name: String,
    #[cfg_attr(feature = "ffi", uniffi(default = []))]
    pub rules: Vec<Rule>,
}

/// Per-tab persisted state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct TabState {
    #[cfg_attr(feature = "ffi", uniffi(default = ""))]
    pub file_path: String,
    #[cfg_attr(feature = "ffi", uniffi(default = ""))]
    pub profile_id: String,
    #[cfg_attr(feature = "ffi", uniffi(default = true))]
    pub auto_scroll: bool,
    #[cfg_attr(feature = "ffi", uniffi(default = ""))]
    pub label: String,
    #[cfg_attr(feature = "ffi", uniffi(default = ""))]
    pub color: String,
    #[cfg_attr(feature = "ffi", uniffi(default = 0))]
    pub position: i32,
}

impl Default for TabState {
    fn default() -> Self {
        Self {
            file_path: String::new(),
            profile_id: String::new(),
            auto_scroll: true,
            label: String::new(),
            color: String::new(),
            position: 0,
        }
    }
}

/// Window geometry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct WindowState {
    #[cfg_attr(feature = "ffi", uniffi(default = 0))]
    pub x: i32,
    #[cfg_attr(feature = "ffi", uniffi(default = 0))]
    pub y: i32,
    #[cfg_attr(feature = "ffi", uniffi(default = 1200))]
    pub width: i32,
    #[cfg_attr(feature = "ffi", uniffi(default = 800))]
    pub height: i32,
    #[cfg_attr(feature = "ffi", uniffi(default = false))]
    pub maximised: bool,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            width: 1200,
            height: 800,
            maximised: false,
        }
    }
}

/// Global application settings (superset; Linux-only keys kept for round-trip).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct AppSettings {
    pub poll_interval_ms: i32,
    pub buffer_size: i32,
    pub scroll_buffer: i32,
    pub scroll_speed: i32,
    pub smooth_scroll: bool,
    pub theme: String,
    pub theme_mode: String,
    pub font_size: i32,
    pub show_line_numbers: bool,
    pub word_wrap: bool,
    pub restore_tabs: bool,
    pub new_tab_position: String,
    pub last_active_tab_path: String,
    pub active_profile: String,
    pub tabs: Vec<TabState>,
    pub recent_files: Vec<String>,
    pub window: WindowState,
    pub display_backend: String,
    pub disable_dmabuf: bool,
    pub gpu_policy: String,
    pub read_timeout_sec: i32,
    pub disable_update_check: bool,
    pub update_check_interval_hours: i32,
    pub ai_provider: String,
    pub ai_endpoint: String,
    pub ai_key: String,
    pub ai_model: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            poll_interval_ms: 500,
            buffer_size: 10_000,
            scroll_buffer: 500,
            scroll_speed: 1,
            smooth_scroll: false,
            theme: "catppuccin".into(),
            theme_mode: "dark".into(),
            font_size: 14,
            show_line_numbers: true,
            word_wrap: false,
            restore_tabs: true,
            new_tab_position: "end".into(),
            last_active_tab_path: String::new(),
            active_profile: "Common Logs".into(),
            tabs: Vec::new(),
            recent_files: Vec::new(),
            window: WindowState::default(),
            display_backend: "auto".into(),
            disable_dmabuf: false,
            gpu_policy: String::new(),
            read_timeout_sec: 30,
            disable_update_check: false,
            update_check_interval_hours: 24,
            ai_provider: String::new(),
            ai_endpoint: String::new(),
            ai_key: String::new(),
            ai_model: String::new(),
        }
    }
}

/// A full theme palette as hex strings. JSON keys are the Go app's kebab-case
/// names so custom theme files drop in unchanged; a missing key falls back to
/// a neutral grey rather than failing the load.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct ThemeColors {
    pub bg_primary: String,
    pub bg_secondary: String,
    pub bg_surface: String,
    pub bg_hover: String,
    pub text_primary: String,
    pub text_secondary: String,
    pub text_muted: String,
    pub accent: String,
    pub accent_hover: String,
    pub border: String,
    pub danger: String,
    pub success: String,
    pub warning: String,
    pub tab_active: String,
    pub tab_inactive: String,
    pub badge_color: String,
    pub scrollbar_track: String,
    pub scrollbar_thumb: String,
}

impl Default for ThemeColors {
    fn default() -> Self {
        let g = || "#808080".to_string();
        Self {
            bg_primary: g(),
            bg_secondary: g(),
            bg_surface: g(),
            bg_hover: g(),
            text_primary: g(),
            text_secondary: g(),
            text_muted: g(),
            accent: g(),
            accent_hover: g(),
            border: g(),
            danger: g(),
            success: g(),
            warning: g(),
            tab_active: g(),
            tab_inactive: g(),
            badge_color: g(),
            scrollbar_track: g(),
            scrollbar_thumb: g(),
        }
    }
}

/// A named theme with dark and light variants.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Theme {
    pub name: String,
    pub display_name: String,
    pub dark: ThemeColors,
    pub light: ThemeColors,
    pub built_in: bool,
}

impl Theme {
    pub fn palette(&self, mode: &str) -> &ThemeColors {
        if mode == "light" {
            &self.light
        } else {
            &self.dark
        }
    }
}

/// Lenient theme file shape: only `name` and `dark` are required; the display
/// name defaults to the name and the light palette to the dark one.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThemeFile {
    name: String,
    #[serde(default)]
    display_name: Option<String>,
    dark: ThemeColors,
    #[serde(default)]
    light: Option<ThemeColors>,
    #[serde(default)]
    built_in: bool,
}

impl<'de> Deserialize<'de> for Theme {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let f = ThemeFile::deserialize(d)?;
        Ok(Theme {
            display_name: f
                .display_name
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| f.name.clone()),
            light: f.light.unwrap_or_else(|| f.dark.clone()),
            name: f.name,
            dark: f.dark,
            built_in: f.built_in,
        })
    }
}

/// The built-in "Common Logs" profile (matches config.DefaultProfile in Go).
pub fn default_profile() -> Profile {
    let rule = |id: &str,
                name: &str,
                pattern: &str,
                match_type: &str,
                fg: &str,
                bg: &str,
                bold: bool,
                priority: i32| Rule {
        id: id.into(),
        name: name.into(),
        pattern: pattern.into(),
        match_type: match_type.into(),
        foreground: fg.into(),
        background: bg.into(),
        bold,
        italic: false,
        enabled: true,
        priority,
    };
    Profile {
        name: "Common Logs".into(),
        rules: vec![
            rule(
                "error",
                "Error",
                r"(?i)\bERROR\b",
                "line",
                "#ff6b6b",
                "#3d1f1f",
                true,
                100,
            ),
            rule(
                "fatal",
                "Fatal",
                r"(?i)\bFATAL\b",
                "line",
                "#ffffff",
                "#cc0000",
                true,
                110,
            ),
            rule(
                "warn",
                "Warning",
                r"(?i)\bWARN(ING)?\b",
                "line",
                "#ffd93d",
                "#3d3520",
                false,
                90,
            ),
            rule(
                "info",
                "Info",
                r"(?i)\bINFO?\b",
                "match",
                "#6bcbff",
                "",
                false,
                50,
            ),
            rule(
                "debug",
                "Debug",
                r"(?i)\bDEBUG\b",
                "match",
                "#888888",
                "",
                false,
                40,
            ),
            rule(
                "timestamp",
                "Timestamp",
                r"\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}",
                "match",
                "#88cc88",
                "",
                false,
                30,
            ),
        ],
    }
}

/// Parses settings JSON leniently: unknown keys are ignored, missing keys take
/// their defaults, and unparsable input yields the defaults.
pub fn settings_from_json(json: &str) -> AppSettings {
    serde_json::from_str(json).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_lenient_decode() {
        let s =
            settings_from_json(r#"{"bufferSize": 500, "theme": "dracula", "unknownKey": true}"#);
        assert_eq!(s.buffer_size, 500);
        assert_eq!(s.theme, "dracula");
        assert_eq!(s.font_size, 14, "missing keys default");
        assert_eq!(settings_from_json("not json"), AppSettings::default());
    }

    #[test]
    fn settings_json_keys_match_go() {
        let json = serde_json::to_string(&AppSettings::default()).unwrap();
        for key in [
            "pollIntervalMs",
            "recentFiles",
            "activeProfile",
            "readTimeoutSec",
            "aiKey",
            "\"window\":{\"x\"",
        ] {
            assert!(json.contains(key), "{key} in {json}");
        }
    }

    #[test]
    fn rule_defaults_and_keys() {
        let r: Rule = serde_json::from_str(r#"{"pattern":"x"}"#).unwrap();
        assert_eq!(r.match_type, "match");
        assert!(r.enabled);
        assert!(serde_json::to_string(&r)
            .unwrap()
            .contains("\"matchType\":\"match\""));
    }

    #[test]
    fn theme_lenient_decode() {
        let t: Theme =
            serde_json::from_str(r##"{"name":"nord","dark":{"bg-primary":"#010203"}}"##).unwrap();
        assert_eq!(t.display_name, "nord");
        assert_eq!(t.dark.bg_primary, "#010203");
        assert_eq!(t.dark.text_primary, "#808080", "missing colour is neutral");
        assert_eq!(t.light, t.dark, "light defaults to dark");
        assert!(!t.built_in);
        assert!(serde_json::to_string(&t)
            .unwrap()
            .contains("\"bg-primary\""));
    }
}
