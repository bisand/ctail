//! UniFFI surface of the engine. Kept separate from the engine proper: the
//! foreign side sees plain records, one object per tailer, and two callback
//! protocols. Callbacks run on the tailer's worker thread.

use crate::config;
use crate::filesearch::{self, FileSearchEvents, FileSearchQuery, FileSearchStatus};
use crate::highlight::{self, LineStyle};
use crate::models::{self, AppSettings, Profile, Rule, Theme, ThemeColors};
use crate::search::{self, TextRange};
use crate::tailer::{LogLine, Tailer, TailerEvents, TailerOptions};
use crate::themes;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Foreign-implemented listener; mirrors [`TailerEvents`].
#[uniffi::export(with_foreign)]
pub trait TailerListener: Send + Sync {
    fn on_lines(&self, lines: Vec<LogLine>);
    fn on_reset(&self);
    fn on_error(&self, message: String);
    fn on_ready(&self);
    fn on_base_resolved(&self, base: i64);
}

/// Foreign-implemented completion for [`TailerHandle::fetch_range`].
#[uniffi::export(with_foreign)]
pub trait FetchReply: Send + Sync {
    fn deliver(&self, lines: Vec<LogLine>);
}

struct ListenerAdapter(Arc<dyn TailerListener>);

impl TailerEvents for ListenerAdapter {
    fn on_lines(&self, lines: Vec<LogLine>) {
        self.0.on_lines(lines)
    }
    fn on_reset(&self) {
        self.0.on_reset()
    }
    fn on_error(&self, message: String) {
        self.0.on_error(message)
    }
    fn on_ready(&self) {
        self.0.on_ready()
    }
    fn on_base_resolved(&self, base: i64) {
        self.0.on_base_resolved(base)
    }
}

/// Engine defaults (the values the shipping app uses).
#[uniffi::export]
pub fn default_tailer_options() -> TailerOptions {
    TailerOptions::default()
}

/// One tailed file. Dropping the last reference stops it.
#[derive(uniffi::Object)]
pub struct TailerHandle {
    inner: Tailer,
}

#[uniffi::export]
impl TailerHandle {
    #[uniffi::constructor]
    pub fn new(
        path: String,
        options: TailerOptions,
        listener: Arc<dyn TailerListener>,
    ) -> Arc<Self> {
        Arc::new(Self {
            inner: Tailer::new(path, options, Arc::new(ListenerAdapter(listener))),
        })
    }

    /// Begins reading (tail-first for large files) and live polling; after a
    /// `stop`, resumes polling.
    pub fn start(&self) {
        self.inner.start()
    }

    /// Pauses polling.
    pub fn stop(&self) {
        self.inner.stop()
    }

    /// Discards state and re-reads from scratch.
    pub fn refresh(&self) {
        self.inner.refresh()
    }

    /// Adjusts the poll cadence at runtime.
    pub fn set_poll_interval(&self, interval: Duration) {
        self.inner.set_poll_interval(interval)
    }

    /// Reads `count` lines from 1-based `start`; `reply` is invoked on the
    /// worker thread. Numbers are local to the tail until indexing completes.
    pub fn fetch_range(&self, start: i64, count: u32, reply: Arc<dyn FetchReply>) {
        self.inner
            .fetch_range(start, count as usize, move |lines| reply.deliver(lines))
    }

    /// Total lines known so far (absolute once the base is resolved).
    pub fn total_lines(&self) -> i64 {
        self.inner.total_lines()
    }

    /// Whether absolute line numbers / scrollback are available yet.
    pub fn indexing_complete(&self) -> bool {
        self.inner.indexing_complete()
    }
}

// ---------------------------------------------------------------------------
// Model helpers
// ---------------------------------------------------------------------------

#[uniffi::export]
pub fn default_settings() -> AppSettings {
    AppSettings::default()
}

/// The built-in "Common Logs" profile.
#[uniffi::export]
pub fn default_profile() -> Profile {
    models::default_profile()
}

/// Lenient settings parse (unknown keys ignored, missing keys defaulted).
#[uniffi::export]
pub fn settings_from_json(json: String) -> AppSettings {
    models::settings_from_json(&json)
}

/// Profile name -> file-name-safe stem.
#[uniffi::export]
pub fn sanitize_profile_name(name: String) -> String {
    config::sanitize_name(&name)
}

// ---------------------------------------------------------------------------
// Config store
// ---------------------------------------------------------------------------

/// Settings/profile persistence; see the crate's `config` module.
#[derive(uniffi::Object)]
pub struct CoreConfigStore {
    inner: config::ConfigStore,
}

#[uniffi::export]
impl CoreConfigStore {
    /// `root` overrides the directory; otherwise `CTAIL_CONFIG_DIR`, then the
    /// platform default.
    #[uniffi::constructor]
    pub fn new(root: Option<String>) -> Arc<Self> {
        Arc::new(Self {
            inner: config::ConfigStore::new(root.map(PathBuf::from)),
        })
    }
    pub fn dir(&self) -> String {
        self.inner.dir().to_string_lossy().into_owned()
    }
    pub fn themes_dir(&self) -> String {
        self.inner.themes_dir().to_string_lossy().into_owned()
    }
    pub fn load_settings(&self) -> AppSettings {
        self.inner.load_settings()
    }
    pub fn save_settings(&self, settings: AppSettings) -> bool {
        self.inner.save_settings(&settings)
    }
    pub fn recent_files(&self) -> Vec<String> {
        self.inner.recent_files()
    }
    pub fn add_recent_file(&self, path: String, max: u32) {
        self.inner.add_recent_file(&path, max as usize)
    }
    pub fn clear_recent_files(&self) {
        self.inner.clear_recent_files()
    }
    pub fn list_profiles(&self) -> Vec<String> {
        self.inner.list_profiles()
    }
    pub fn load_profile(&self, name: String) -> Option<Profile> {
        self.inner.load_profile(&name)
    }
    pub fn save_profile(&self, profile: Profile) -> bool {
        self.inner.save_profile(&profile)
    }
    pub fn delete_profile(&self, name: String) {
        self.inner.delete_profile(&name)
    }
    pub fn rename_profile(&self, old: String, new: String) -> bool {
        self.inner.rename_profile(&old, &new)
    }
    pub fn ensure_default_profile(&self) {
        self.inner.ensure_default_profile()
    }
}

// ---------------------------------------------------------------------------
// Themes
// ---------------------------------------------------------------------------

#[uniffi::export]
pub fn built_in_themes() -> Vec<Theme> {
    themes::built_in_themes()
}

/// Built-ins plus custom `*.json` themes from `custom_dir`.
#[uniffi::export]
pub fn all_themes(custom_dir: Option<String>) -> Vec<Theme> {
    themes::all_themes(custom_dir.as_deref().map(Path::new))
}

/// Theme name + "dark"/"light" -> palette (Catppuccin dark for unknown names).
#[uniffi::export]
pub fn resolve_palette(name: String, mode: String, custom_dir: Option<String>) -> ThemeColors {
    themes::resolve_palette(&name, &mode, custom_dir.as_deref().map(Path::new))
}

// ---------------------------------------------------------------------------
// Highlighting
// ---------------------------------------------------------------------------

/// A compiled rule set; see the crate's `highlight` module.
#[derive(uniffi::Object)]
pub struct CoreHighlighter {
    inner: highlight::Highlighter,
}

#[uniffi::export]
impl CoreHighlighter {
    #[uniffi::constructor]
    pub fn new(rules: Vec<Rule>) -> Arc<Self> {
        Arc::new(Self {
            inner: highlight::Highlighter::new(&rules),
        })
    }
    /// Compiled rules in index order (what `LineStyle` indices refer to).
    pub fn rules(&self) -> Vec<Rule> {
        self.inner.rules()
    }
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
    pub fn apply(&self, line: String) -> LineStyle {
        self.inner.apply(&line)
    }
}

/// `None` if the pattern compiles, else the error message.
#[uniffi::export]
pub fn validate_pattern(pattern: String) -> Option<String> {
    highlight::validate_pattern(&pattern)
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

/// A compiled search query; see the crate's `search` module.
#[derive(uniffi::Object)]
pub struct CoreSearchMatcher {
    inner: search::SearchMatcher,
}

#[uniffi::export]
impl CoreSearchMatcher {
    #[uniffi::constructor]
    pub fn new(text: String, case_sensitive: bool, whole_word: bool, is_regex: bool) -> Arc<Self> {
        Arc::new(Self {
            inner: search::SearchMatcher::new(&text, case_sensitive, whole_word, is_regex),
        })
    }
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
    pub fn is_valid(&self) -> bool {
        self.inner.is_valid()
    }
    pub fn matches(&self, line: String) -> bool {
        self.inner.matches(&line)
    }
    pub fn ranges(&self, line: String) -> Vec<TextRange> {
        self.inner.ranges(&line)
    }
    /// Indices of the matching lines, for filtering a whole window in one call.
    pub fn matching_indices(&self, lines: Vec<String>) -> Vec<u32> {
        self.inner.matching_indices(&lines)
    }
}

/// Foreign-implemented listener for [`CoreFileSearch`]; mirrors
/// [`FileSearchEvents`].
#[uniffi::export(with_foreign)]
pub trait FileSearchListener: Send + Sync {
    fn on_result(&self, query: FileSearchQuery, total: u32);
}

struct FileSearchAdapter(Arc<dyn FileSearchListener>);

impl FileSearchEvents for FileSearchAdapter {
    fn on_result(&self, query: FileSearchQuery, total: u32) {
        self.0.on_result(query, total)
    }
}

/// Scans whole files for a find bar. Dropping it calls off any scan in flight.
#[derive(uniffi::Object)]
pub struct CoreFileSearch {
    inner: filesearch::FileSearch,
}

#[uniffi::export]
impl CoreFileSearch {
    #[uniffi::constructor]
    pub fn new(listener: Arc<dyn FileSearchListener>) -> Arc<Self> {
        Arc::new(Self {
            inner: filesearch::FileSearch::new(Arc::new(FileSearchAdapter(listener))),
        })
    }
    /// Safe to call on every keystroke: the scan waits for the typing to stop,
    /// and a query already answered or under way is not scanned for twice.
    pub fn request(&self, query: FileSearchQuery) {
        self.inner.request(query)
    }
    pub fn clear(&self) {
        self.inner.clear()
    }
    pub fn status(&self, query: FileSearchQuery) -> FileSearchStatus {
        self.inner.status(&query)
    }
    pub fn matches(&self, query: FileSearchQuery) -> Vec<i64> {
        self.inner.matches(&query)
    }
    /// The line number of the next or previous match, `from` being the line
    /// the view is showing.
    pub fn step(&self, query: FileSearchQuery, forward: bool, from: Option<i64>) -> Option<i64> {
        self.inner.step(&query, forward, from)
    }
}
