//! An opt-in trace of scrolling, for judging smoothness by measurement.
//!
//! `CTAIL_DEBUG_SCROLL_TRACE=<file>` appends one line per scroll event the
//! log view handles and one per frame the window paints, each stamped in
//! microseconds since the process started. The intervals between frames, and
//! how much of the log each one moved, are what "choppy" is made of, and a
//! gesture cannot be synthesised from outside the window without accessibility
//! permission — so it is recorded from inside instead.

use std::fs::File;
use std::io::Write as _;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

struct Trace {
    file: Mutex<File>,
    started: Instant,
}

fn trace() -> Option<&'static Trace> {
    static TRACE: OnceLock<Option<Trace>> = OnceLock::new();
    TRACE
        .get_or_init(|| {
            let path = std::env::var_os("CTAIL_DEBUG_SCROLL_TRACE")?;
            let file = File::options().create(true).append(true).open(path).ok()?;
            Some(Trace {
                file: Mutex::new(file),
                started: Instant::now(),
            })
        })
        .as_ref()
}

/// Whether tracing is on, so callers can skip formatting when it is not.
pub fn enabled() -> bool {
    trace().is_some()
}

/// Appends `what` under a timestamp. Nothing happens when tracing is off.
pub fn log(what: std::fmt::Arguments<'_>) {
    let Some(trace) = trace() else {
        return;
    };
    let us = trace.started.elapsed().as_micros();
    if let Ok(mut file) = trace.file.lock() {
        let _ = writeln!(file, "{us} {what}");
    }
}
