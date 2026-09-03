//! ctail engine.
//!
//! Platform-neutral core shared by the native front ends. Today it holds the
//! tail engine ([`tailer`]); highlighting, search and configuration follow.

pub mod tailer;

pub use tailer::{
    index_file, split_lines, CancelToken, Counters, Engine, HeadScan, IndexResult, LogLine,
    SplitResult, Tailer, TailerEvents, TailerOptions,
};
