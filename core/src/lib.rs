//! ctail engine.
//!
//! Platform-neutral core shared by the native front ends. Today it holds the
//! tail engine ([`tailer`]); highlighting, search and configuration follow.
//! With the `ffi` feature the [`ffi`] module exposes it through UniFFI.

pub mod tailer;

#[cfg(feature = "ffi")]
pub mod ffi;

#[cfg(feature = "ffi")]
uniffi::setup_scaffolding!();

pub use tailer::{
    index_file, split_lines, CancelToken, Counters, Engine, HeadScan, IndexResult, LogLine,
    SplitResult, Tailer, TailerEvents, TailerOptions,
};
