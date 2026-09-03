//! UniFFI surface of the engine. Kept separate from the engine proper: the
//! foreign side sees plain records, one object per tailer, and two callback
//! protocols. Callbacks run on the tailer's worker thread.

use crate::tailer::{LogLine, Tailer, TailerEvents, TailerOptions};
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
    /// worker thread. Empty until indexing is complete.
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
