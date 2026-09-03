//! ctail engine.
//!
//! Platform-neutral core shared by the native front ends: the tail engine
//! ([`tailer`]), the data model ([`models`]) and its persistence ([`config`]),
//! the theme catalogue ([`themes`]), regex highlighting ([`highlight`]) and
//! search ([`search`], and [`filesearch`] for scanning a whole file). With the `ffi` feature the [`ffi`] module exposes it
//! all through UniFFI.

pub mod config;
pub mod filesearch;
pub mod highlight;
pub mod models;
pub mod search;
pub mod tailer;
mod text;
pub mod themes;

#[cfg(feature = "ffi")]
pub mod ffi;

#[cfg(feature = "ffi")]
uniffi::setup_scaffolding!();

pub use config::ConfigStore;
pub use filesearch::{FileSearch, FileSearchEvents, FileSearchQuery, FileSearchStatus};
pub use highlight::{Highlighter, LineStyle, Span};
pub use models::{AppSettings, Profile, Rule, TabState, Theme, ThemeColors, WindowState};
pub use search::{search_file, search_file_cancellable, SearchMatcher, SearchResult, TextRange};
pub use tailer::{
    index_file, split_lines, CancelToken, Counters, Engine, HeadScan, IndexResult, LogLine,
    SplitResult, Tailer, TailerEvents, TailerOptions,
};
pub use themes::{all_themes, built_in_themes, resolve_palette};
