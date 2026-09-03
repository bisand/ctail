//! Settings, profile and theme-file persistence, mirroring the Go app's
//! `internal/config`. Storage is a directory with `settings.json`,
//! `profiles/<name>.json` and `themes/<name>.json`. Writes are atomic (temp
//! file + rename) so a crash mid-write can't corrupt a config; parse failures
//! fall back to defaults rather than erroring.

use crate::models::{default_profile, AppSettings, Profile};
use std::fs;
use std::path::{Path, PathBuf};

/// The config directory and everything in it.
#[derive(Debug)]
pub struct ConfigStore {
    dir: PathBuf,
    profiles_dir: PathBuf,
    themes_dir: PathBuf,
}

impl ConfigStore {
    /// `root` overrides the location (tests, embedding). Otherwise the
    /// `CTAIL_CONFIG_DIR` environment variable does (isolated dev runs), and
    /// failing that the platform's per-user config location:
    /// `~/Library/Application Support/ctail` on macOS, `$XDG_CONFIG_HOME/ctail`
    /// (or `~/.config/ctail`) on Linux, `%APPDATA%\ctail` on Windows.
    pub fn new(root: Option<PathBuf>) -> Self {
        let dir = root
            .or_else(|| {
                std::env::var_os("CTAIL_CONFIG_DIR")
                    .filter(|v| !v.is_empty())
                    .map(PathBuf::from)
            })
            .unwrap_or_else(Self::platform_dir);
        let profiles_dir = dir.join("profiles");
        let themes_dir = dir.join("themes");
        let _ = fs::create_dir_all(&profiles_dir);
        let _ = fs::create_dir_all(&themes_dir);
        Self {
            dir,
            profiles_dir,
            themes_dir,
        }
    }

    fn platform_dir() -> PathBuf {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        #[cfg(target_os = "macos")]
        let base = home.map(|h| h.join("Library").join("Application Support"));
        #[cfg(target_os = "windows")]
        let base = std::env::var_os("APPDATA").map(PathBuf::from).or(home);
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| home.map(|h| h.join(".config")));
        base.unwrap_or_else(|| PathBuf::from(".")).join("ctail")
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }
    pub fn themes_dir(&self) -> &Path {
        &self.themes_dir
    }

    // --- settings ---------------------------------------------------------

    fn settings_path(&self) -> PathBuf {
        self.dir.join("settings.json")
    }

    // --- the automatic update check -----------------------------------------

    fn update_stamp_path(&self) -> PathBuf {
        self.dir.join("last-update-check")
    }

    /// When the last automatic update check ran, in seconds since the epoch;
    /// 0 when it never has. Kept beside the settings rather than in them, so
    /// a check leaves the settings file — and its modification time — alone.
    pub fn last_update_check(&self) -> i64 {
        fs::read_to_string(self.update_stamp_path())
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0)
    }

    pub fn set_last_update_check(&self, seconds: i64) {
        let _ = fs::write(self.update_stamp_path(), seconds.to_string());
    }

    /// Whether an automatic check is due at `now` under `settings`: enabled,
    /// and the interval has passed since the last one.
    pub fn update_check_due(&self, settings: &AppSettings, now: i64) -> bool {
        if settings.disable_update_check {
            return false;
        }
        let interval = i64::from(settings.update_check_interval_hours.max(1)) * 3600;
        now - self.last_update_check() >= interval
    }

    // --- the Copilot sign-in ---------------------------------------------------

    fn copilot_token_path(&self) -> PathBuf {
        self.dir.join("copilot-token")
    }

    /// The GitHub OAuth token a Copilot sign-in ended in, for a front end
    /// without a keychain of its own. The macOS app keeps its own copy in the
    /// user defaults.
    pub fn copilot_token(&self) -> Option<String> {
        fs::read_to_string(self.copilot_token_path())
            .ok()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
    }

    pub fn set_copilot_token(&self, token: &str) {
        let _ = fs::write(self.copilot_token_path(), token);
    }

    pub fn clear_copilot_token(&self) {
        let _ = fs::remove_file(self.copilot_token_path());
    }

    pub fn load_settings(&self) -> AppSettings {
        fs::read_to_string(self.settings_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save_settings(&self, settings: &AppSettings) -> bool {
        match serde_json::to_string_pretty(settings) {
            Ok(json) => atomic_write(&self.settings_path(), json.as_bytes()),
            Err(_) => false,
        }
    }

    // --- recent files (stored in settings, MRU order, capped) --------------

    pub fn recent_files(&self) -> Vec<String> {
        self.load_settings().recent_files
    }

    pub fn add_recent_file(&self, path: &str, max: usize) {
        let mut s = self.load_settings();
        s.recent_files.retain(|p| p != path);
        s.recent_files.insert(0, path.to_string());
        s.recent_files.truncate(max);
        self.save_settings(&s);
    }

    pub fn clear_recent_files(&self) {
        let mut s = self.load_settings();
        s.recent_files.clear();
        self.save_settings(&s);
    }

    // --- profiles ---------------------------------------------------------

    fn profile_path(&self, name: &str) -> PathBuf {
        self.profiles_dir
            .join(format!("{}.json", sanitize_name(name)))
    }

    /// Names of all stored profiles, sorted.
    pub fn list_profiles(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(&self.profiles_dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|x| x == "json"))
                    .filter_map(|p| fs::read_to_string(p).ok())
                    .filter_map(|s| serde_json::from_str::<Profile>(&s).ok())
                    .map(|p| p.name)
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    pub fn load_profile(&self, name: &str) -> Option<Profile> {
        let s = fs::read_to_string(self.profile_path(name)).ok()?;
        serde_json::from_str(&s).ok()
    }

    pub fn save_profile(&self, profile: &Profile) -> bool {
        match serde_json::to_string_pretty(profile) {
            Ok(json) => atomic_write(&self.profile_path(&profile.name), json.as_bytes()),
            Err(_) => false,
        }
    }

    pub fn delete_profile(&self, name: &str) {
        let _ = fs::remove_file(self.profile_path(name));
    }

    pub fn rename_profile(&self, old: &str, new: &str) -> bool {
        let Some(mut p) = self.load_profile(old) else {
            return false;
        };
        self.delete_profile(old);
        p.name = new.to_string();
        self.save_profile(&p)
    }

    /// Writes the built-in profile if no profiles exist yet.
    pub fn ensure_default_profile(&self) {
        if self.list_profiles().is_empty() {
            self.save_profile(&default_profile());
        }
    }
}

/// Mirrors sanitizeFilename in the Go app — strips path-hostile characters.
pub fn sanitize_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if "/\\:*?\"<>|".contains(c) { '_' } else { c })
        .collect();
    if cleaned.is_empty() {
        "profile".into()
    } else {
        cleaned
    }
}

fn atomic_write(path: &Path, data: &[u8]) -> bool {
    let tmp = path.with_extension("json.tmp");
    if fs::write(&tmp, data).is_ok() && fs::rename(&tmp, path).is_ok() {
        return true;
    }
    let _ = fs::remove_file(&tmp);
    fs::write(path, data).is_ok() // best-effort fallback
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Rule;

    fn temp_root(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ctail-config-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn settings_round_trip_and_defaults() {
        let root = temp_root("settings");
        let store = ConfigStore::new(Some(root.clone()));
        let s = AppSettings {
            buffer_size: 42_000,
            theme: "nord".into(),
            recent_files: vec!["/a.log".into(), "/b.log".into()],
            ..Default::default()
        };
        assert!(store.save_settings(&s));
        assert_eq!(store.load_settings(), s);
        assert!(!fs::read_dir(&root).unwrap().any(|e| e
            .unwrap()
            .path()
            .extension()
            .is_some_and(|x| x == "tmp")));

        let empty = ConfigStore::new(Some(root.join("empty")));
        assert_eq!(empty.load_settings().buffer_size, 10_000);
        assert_eq!(empty.load_settings().active_profile, "Common Logs");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn profile_crud_and_recent_files() {
        let root = temp_root("profiles");
        let store = ConfigStore::new(Some(root.clone()));
        store.ensure_default_profile();
        assert_eq!(store.list_profiles(), ["Common Logs"]);
        let p = Profile {
            name: "My Profile".into(),
            rules: vec![Rule {
                id: "x".into(),
                name: "X".into(),
                pattern: "foo".into(),
                match_type: "line".into(),
                ..Default::default()
            }],
        };
        assert!(store.save_profile(&p));
        assert_eq!(store.list_profiles(), ["Common Logs", "My Profile"]);
        assert_eq!(store.load_profile("My Profile"), Some(p));
        assert!(store.rename_profile("My Profile", "Renamed"));
        assert!(store.load_profile("My Profile").is_none());
        assert_eq!(
            store.load_profile("Renamed").unwrap().rules[0].pattern,
            "foo"
        );
        store.delete_profile("Renamed");
        assert_eq!(store.list_profiles(), ["Common Logs"]);

        for i in 0..20 {
            store.add_recent_file(&format!("/log/{i}.log"), 15);
        }
        assert_eq!(store.recent_files().len(), 15);
        assert_eq!(store.recent_files()[0], "/log/19.log");
        store.add_recent_file("/log/5.log", 15);
        assert_eq!(store.recent_files()[0], "/log/5.log");
        assert_eq!(
            store
                .recent_files()
                .iter()
                .filter(|p| *p == "/log/5.log")
                .count(),
            1
        );
        store.clear_recent_files();
        assert!(store.recent_files().is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn sanitize() {
        assert_eq!(sanitize_name("a/b:c"), "a_b_c");
        assert_eq!(sanitize_name(""), "profile");
        assert_eq!(sanitize_name("plain name"), "plain name");
    }
}
