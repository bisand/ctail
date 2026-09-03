//! Polling-based file tailer, ported from the Swift engine
//! (`macos/Sources/ctailmac/Tailer.swift`), which was itself a port of the Go
//! tailer in `legacy/wails/internal/tailer`.
//!
//! Design choices:
//!   - Polling (not kqueue/inotify/FSEvents) so slow or unreachable network
//!     mounts can't wedge the UI — every I/O op runs on a dedicated I/O thread
//!     under a timeout, and a wedged op is abandoned rather than waited on.
//!   - Inode-change detection for log rotation; truncation detection on shrink.
//!   - Only *complete* lines (ending in `\n`) are committed; a trailing partial
//!     is left until the next poll reads it whole.
//!
//! Instant tail (the important bit): for large files we seek near the end and
//! show + live-follow the tail IMMEDIATELY, numbering those lines *locally*
//! from the tail start (1, 2, …). The expensive part — counting how many lines
//! precede the tail (`base`) so we can show true line numbers, plus indexing
//! the head for scrollback — runs on a background thread and never blocks
//! display or following. Absolute line number = `base + local`; until `base` is
//! known the UI shows a placeholder in the gutter.
//!
//! The byte-offset index is split into two disjoint, independently-owned
//! regions so the background scan and the live poller never touch the same
//! state:
//!   - `head_checkpoints`: sparse offsets for lines before the tail (absolute),
//!     written once when the background scan lands.
//!   - `tail_checkpoints`: sparse offsets for lines from the tail onward
//!     (local), appended by the live poller.
//!
//! Two layers:
//!   - [`Engine`] — the synchronous state machine. Deterministic, no threads of
//!     its own, and the seam the parity tests drive directly.
//!   - [`Tailer`] — the threaded driver the UI talks to: owns an `Engine` on a
//!     worker thread, runs the poll timer, spawns head scans, and delivers
//!     [`TailerEvents`] callbacks.

use memchr::{memchr, memchr_iter, memrchr};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

/// A single log line with its 1-based number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogLine {
    pub number: i64,
    pub text: String,
}

/// Engine tuning knobs. Defaults match the shipping macOS app.
#[derive(Clone, Debug)]
pub struct TailerOptions {
    /// Poll cadence; clamped to at least 50 ms.
    pub poll_interval: Duration,
    /// Upper bound for any single stat/read before it is abandoned.
    pub read_timeout: Duration,
    /// Files larger than this open tail-first with a background head count.
    pub tail_first_threshold: i64,
    /// How far before EOF the instant tail starts.
    pub tail_seek_back: i64,
    /// Largest single disk read; bursts and re-reads are pumped in chunks of
    /// this size so transient memory never exceeds one chunk.
    pub max_read_chunk: i64,
    /// Sparse index granularity: one checkpoint per this many lines.
    pub index_stride: i64,
}

impl Default for TailerOptions {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_millis(250),
            read_timeout: Duration::from_secs(30),
            tail_first_threshold: 1024 * 1024,
            tail_seek_back: 512 * 1024,
            max_read_chunk: 4 * 1024 * 1024,
            index_stride: 1000,
        }
    }
}

/// Callbacks from the engine. Invoked on the tailer's worker thread — front
/// ends must hop to their UI thread themselves.
pub trait TailerEvents: Send + Sync + 'static {
    /// New complete lines, numbered absolutely (or locally until the base is known).
    fn on_lines(&self, _lines: Vec<LogLine>) {}
    /// Truncation, rotation or refresh: the view must clear.
    fn on_reset(&self) {}
    /// The file became unavailable (reported once per outage).
    fn on_error(&self, _message: String) {}
    /// Initial read done, or the file came back after an outage.
    fn on_ready(&self) {}
    /// Background head count finished; arg = lines before the tail.
    fn on_base_resolved(&self, _base: i64) {}
}

/// A thread-safe one-way "cancelled" flag shared with a background head scan.
#[derive(Debug, Default)]
pub struct CancelToken(AtomicBool);

impl CancelToken {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

// ---------------------------------------------------------------------------
// Pure helpers (the parse hot paths)
// ---------------------------------------------------------------------------

/// Result of [`split_lines`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SplitResult {
    /// Complete lines numbered from `start_num + 1`.
    pub lines: Vec<LogLine>,
    /// Byte offset (relative to `base_offset`) where each line starts.
    pub offsets: Vec<i64>,
    /// Byte offset just past the last complete line.
    pub consumed: i64,
}

/// Splits a buffer into complete lines numbered from `start_num`, their start
/// offsets, and the byte offset just past the last complete line. A trailing
/// partial line is left out; a trailing CR is stripped. Invalid UTF-8 is
/// replaced rather than rejected.
pub fn split_lines(data: &[u8], start_num: i64, base_offset: i64) -> SplitResult {
    let mut out = SplitResult {
        consumed: base_offset,
        ..Default::default()
    };
    let mut line_start = 0usize;
    for (num, nl) in (start_num + 1..).zip(memchr_iter(b'\n', data)) {
        let mut end = nl;
        if end > line_start && data[end - 1] == b'\r' {
            end -= 1;
        }
        out.offsets.push(base_offset + line_start as i64);
        out.lines.push(LogLine {
            number: num,
            text: decode(&data[line_start..end]),
        });
        out.consumed = base_offset + nl as i64 + 1;
        line_start = nl + 1;
    }
    out
}

#[inline]
fn decode(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_owned(),
        Err(_) => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// Result of [`index_file`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IndexResult {
    /// Byte offset of line 1, and of every `stride`-th line after it.
    pub checkpoints: Vec<i64>,
    /// Number of complete lines in `[0, up_to)`.
    pub total: i64,
    /// Byte offset just past the last complete line.
    pub consumed: i64,
}

/// Scans `[0, up_to)` of `path` building a sparse offset index (one checkpoint
/// per `stride` lines), the complete-line count, and the byte offset past the
/// last complete line.
///
/// A reader thread streams 1 MiB chunks through a short bounded queue so disk
/// reads overlap parsing; the parser counts newlines with SIMD and only walks
/// individual hits to pin down each checkpoint. Transient memory stays at a few
/// chunks. `is_cancelled` is polled between chunks.
pub fn index_file(
    path: &Path,
    up_to: i64,
    stride: i64,
    is_cancelled: &dyn Fn() -> bool,
) -> IndexResult {
    let mut result = IndexResult {
        checkpoints: vec![0],
        ..Default::default()
    };
    if up_to <= 0 {
        return result;
    }
    let Ok(file) = File::open(path) else {
        return result;
    };
    let stride = stride.max(1);
    const CHUNK: usize = 1 << 20;
    const DEPTH: usize = 3;
    let (full_tx, full_rx) = mpsc::sync_channel::<Vec<u8>>(DEPTH);
    let (free_tx, free_rx) = mpsc::channel::<Vec<u8>>();
    for _ in 0..=DEPTH {
        let _ = free_tx.send(Vec::with_capacity(CHUNK));
    }
    thread::scope(|scope| {
        scope.spawn(move || {
            let mut pos: i64 = 0;
            while pos < up_to {
                let Ok(mut buf) = free_rx.recv() else { break };
                let want = CHUNK.min((up_to - pos) as usize);
                buf.resize(want, 0);
                let n = pread_full(&file, pos as u64, &mut buf);
                buf.truncate(n);
                if n == 0 || full_tx.send(buf).is_err() {
                    break;
                }
                pos += n as i64;
            }
        });
        let mut until_checkpoint = stride;
        let mut pos: i64 = 0;
        for buf in &full_rx {
            if is_cancelled() {
                break; // tab closed / file reset — abandon a now-irrelevant scan
            }
            scan_chunk(&buf, pos, up_to, stride, &mut until_checkpoint, &mut result);
            pos += buf.len() as i64;
            let _ = free_tx.send(buf);
        }
        drop(free_tx); // unblocks a reader waiting for a buffer
        drop(full_rx); // ...or one about to hand a chunk over
    });
    result
}

/// Indexes one chunk. Works in 16 KiB blocks: a block with no checkpoint in it
/// is just counted; a block containing one is bisected by counts down to a
/// small window before individual newlines are walked.
fn scan_chunk(
    buf: &[u8],
    chunk_pos: i64,
    up_to: i64,
    stride: i64,
    until_checkpoint: &mut i64,
    r: &mut IndexResult,
) {
    const BLOCK: usize = 16 << 10;
    let mut off = 0usize;
    while off < buf.len() {
        let end = (off + BLOCK).min(buf.len());
        let mut block = &buf[off..end];
        let mut block_pos = chunk_pos + off as i64;
        loop {
            let c = count_newlines(block) as i64;
            if c < *until_checkpoint {
                r.total += c;
                *until_checkpoint -= c;
                if c > 0 {
                    let last = memrchr(b'\n', block).expect("counted newline");
                    r.consumed = block_pos + last as i64 + 1;
                }
                break;
            }
            let k = nth_newline(block, *until_checkpoint as usize);
            r.total += *until_checkpoint;
            *until_checkpoint = stride;
            let next = block_pos + k as i64 + 1;
            r.consumed = next;
            if next < up_to {
                r.checkpoints.push(next);
            }
            block = &block[k + 1..];
            block_pos = next;
        }
        off = end;
    }
}

#[inline]
fn count_newlines(b: &[u8]) -> usize {
    bytecount::count(b, b'\n')
}

/// Index of the `n`-th (1-based) newline in `b`, which must contain at least
/// `n`. Bisects by SIMD counts until the window is small, then walks hits.
fn nth_newline(mut b: &[u8], mut n: usize) -> usize {
    let mut lo = 0usize;
    while b.len() > 1024 {
        let mid = b.len() / 2;
        let c = count_newlines(&b[..mid]);
        if n <= c {
            b = &b[..mid];
        } else {
            n -= c;
            lo += mid;
            b = &b[mid..];
        }
    }
    lo + memchr_iter(b'\n', b)
        .nth(n - 1)
        .expect("nth newline exists")
}

/// Positional read that keeps going through short reads until `buf` is full
/// or EOF/error. Returns the number of bytes read.
fn pread_full(file: &File, offset: u64, buf: &mut [u8]) -> usize {
    let mut filled = 0usize;
    while filled < buf.len() {
        match pread(file, offset + filled as u64, &mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    filled
}

#[cfg(unix)]
fn pread(file: &File, offset: u64, buf: &mut [u8]) -> std::io::Result<usize> {
    use std::os::unix::fs::FileExt;
    file.read_at(buf, offset)
}

#[cfg(windows)]
fn pread(file: &File, offset: u64, buf: &mut [u8]) -> std::io::Result<usize> {
    use std::os::windows::fs::FileExt;
    file.seek_read(buf, offset)
}

/// Lock-free snapshot of the counters the UI reads directly. Updated by the
/// engine before every event so a callback always observes consistent values.
#[derive(Debug, Default)]
pub struct Counters {
    total_lines: AtomicI64,
    indexing_complete: AtomicBool,
}

impl Counters {
    pub fn total_lines(&self) -> i64 {
        self.total_lines.load(Ordering::Acquire)
    }
    pub fn indexing_complete(&self) -> bool {
        self.indexing_complete.load(Ordering::Acquire)
    }
}

#[derive(Clone, Copy, Debug)]
struct Stat {
    size: i64,
    /// Inode on Unix; creation time on Windows. Changes when the path is
    /// replaced by a new file (rotation).
    identity: u64,
}

fn stat_path(path: &Path) -> Option<Stat> {
    let md = std::fs::metadata(path).ok()?;
    #[cfg(unix)]
    let identity = {
        use std::os::unix::fs::MetadataExt;
        md.ino()
    };
    #[cfg(windows)]
    let identity = {
        use std::os::windows::fs::MetadataExt;
        md.creation_time()
    };
    Some(Stat {
        size: md.len() as i64,
        identity,
    })
}

// ---------------------------------------------------------------------------
// I/O lane: one long-lived I/O thread, abandoned and replaced on timeout
// ---------------------------------------------------------------------------

type Job = Box<dyn FnOnce() + Send>;

/// Runs blocking I/O on a dedicated thread with a deadline. If an op exceeds
/// the deadline (a dead NFS mount, say) the thread is abandoned — it exits on
/// its own once the syscall finally returns — and a fresh one takes over, so a
/// single wedged call never blocks the engine for good.
struct IoLane {
    tx: Sender<Job>,
    timeout: Duration,
}

impl IoLane {
    fn new(timeout: Duration) -> Self {
        Self {
            tx: Self::spawn(),
            timeout,
        }
    }

    fn spawn() -> Sender<Job> {
        let (tx, rx) = mpsc::channel::<Job>();
        thread::Builder::new()
            .name("ctail-io".into())
            .spawn(move || {
                for job in rx {
                    job();
                }
            })
            .expect("spawn ctail-io thread");
        tx
    }

    fn run<T: Send + 'static>(&mut self, work: impl FnOnce() -> T + Send + 'static) -> Option<T> {
        let (rtx, rrx) = mpsc::channel();
        let job: Job = Box::new(move || {
            let _ = rtx.send(work());
        });
        if self.tx.send(job).is_err() {
            self.tx = Self::spawn();
            return None;
        }
        match rrx.recv_timeout(self.timeout) {
            Ok(v) => Some(v),
            Err(_) => {
                self.tx = Self::spawn(); // abandon the wedged thread
                None
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Engine: the synchronous core
// ---------------------------------------------------------------------------

/// A background head-count job produced by [`Engine::perform_initial_read`]
/// when a file opens tail-first. The driver runs it off-thread and feeds the
/// result back through [`Engine::apply_head_count`]; the token lets a
/// superseding read (rotation, truncation, refresh, close) abandon it.
#[derive(Debug)]
pub struct HeadScan {
    pub path: Arc<PathBuf>,
    pub up_to: i64,
    pub stride: i64,
    pub token: Arc<CancelToken>,
}

impl HeadScan {
    pub fn run(&self) -> IndexResult {
        index_file(&self.path, self.up_to, self.stride, &|| {
            self.token.is_cancelled()
        })
    }
}

/// The synchronous tail engine. Not thread-safe by itself: [`Tailer`] owns one
/// on its worker thread; tests drive one directly.
pub struct Engine {
    path: Arc<PathBuf>,
    opts: TailerOptions,
    io: IoLane,
    events: Arc<dyn TailerEvents>,
    counters: Arc<Counters>,

    line_num: i64, // LOCAL line count (lines read since tail_start)
    offset: i64,
    file_size: i64,
    identity: u64,
    in_error: bool,
    tail_start: i64,            // byte offset where tail reading began
    base: i64,                  // complete lines before tail_start (absolute offset)
    base_known: bool,           // false while the background count runs
    head_checkpoints: Vec<i64>, // absolute offsets for lines 1..=base
    tail_checkpoints: Vec<i64>, // offsets for tail lines, indexed locally
    head_count_token: Option<Arc<CancelToken>>,
}

impl Engine {
    pub fn new(
        path: impl Into<PathBuf>,
        opts: TailerOptions,
        events: Arc<dyn TailerEvents>,
    ) -> Self {
        let mut opts = opts;
        opts.poll_interval = opts.poll_interval.max(Duration::from_millis(50));
        opts.max_read_chunk = opts.max_read_chunk.max(1);
        opts.index_stride = opts.index_stride.max(1);
        Self {
            path: Arc::new(path.into()),
            io: IoLane::new(opts.read_timeout),
            opts,
            events,
            counters: Arc::new(Counters::default()),
            line_num: 0,
            offset: 0,
            file_size: 0,
            identity: 0,
            in_error: false,
            tail_start: 0,
            base: 0,
            base_known: true,
            head_checkpoints: Vec::new(),
            tail_checkpoints: Vec::new(),
            head_count_token: None,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn options(&self) -> &TailerOptions {
        &self.opts
    }
    /// Total lines known so far (grows as the file is tailed; absolute once based).
    pub fn total_lines(&self) -> i64 {
        self.base + self.line_num
    }
    /// Whether absolute line numbers / scrollback are available yet.
    pub fn indexing_complete(&self) -> bool {
        self.base_known
    }
    pub fn tail_start(&self) -> i64 {
        self.tail_start
    }
    pub fn base(&self) -> i64 {
        self.base
    }
    /// Shared counters, kept current before every event.
    pub fn counters(&self) -> Arc<Counters> {
        self.counters.clone()
    }

    fn sync_counters(&self) {
        self.counters
            .total_lines
            .store(self.total_lines(), Ordering::Release);
        self.counters
            .indexing_complete
            .store(self.base_known, Ordering::Release);
    }

    fn emit_lines(&self, lines: Vec<LogLine>) {
        self.sync_counters();
        if !lines.is_empty() {
            self.events.on_lines(lines);
        }
    }

    /// Reads the file from scratch. Large files show their tail immediately and
    /// return a [`HeadScan`] the caller must run in the background.
    pub fn perform_initial_read(&mut self) -> Option<HeadScan> {
        let Some(st) = self.stat() else {
            self.sync_counters();
            return None;
        };
        self.cancel_head_count(); // supersede any in-flight head count
        self.identity = st.identity;
        self.file_size = st.size;
        self.line_num = 0;
        self.offset = 0;
        self.base = 0;
        self.head_checkpoints.clear();
        self.tail_checkpoints.clear();

        if st.size > self.opts.tail_first_threshold {
            // Instant tail: show the last chunk now (numbered locally), follow live,
            // and count the head in the background to fill in real numbers.
            let seek = (st.size - self.opts.tail_seek_back).max(0);
            self.tail_start = self.align_to_line_boundary(seek);
            self.base_known = false;
            let (lines, consumed) = self.read_new_lines(self.tail_start, st.size, true);
            self.offset = consumed;
            self.emit_lines(lines);
            let token = CancelToken::new();
            self.head_count_token = Some(token.clone());
            Some(HeadScan {
                path: self.path.clone(),
                up_to: self.tail_start,
                stride: self.opts.index_stride,
                token,
            })
        } else {
            // Small file: read it all from the top; numbers are absolute immediately.
            self.tail_start = 0;
            self.base_known = true;
            let (lines, consumed) = self.read_new_lines(0, st.size, true);
            self.offset = consumed;
            self.emit_lines(lines);
            None
        }
    }

    /// One poll tick: detect rotation/truncation, read whatever is new.
    /// Returns a [`HeadScan`] if a rotated/truncated file re-opened tail-first.
    pub fn perform_poll(&mut self) -> Option<HeadScan> {
        let Some(st) = self.stat() else {
            if !self.in_error {
                self.in_error = true;
                self.events
                    .on_error(format!("file unavailable: {}", self.path.display()));
            }
            return None;
        };
        let was_in_error = self.in_error;
        self.in_error = false;

        let rotated = self.identity != 0 && st.identity != self.identity;

        if rotated || st.size < self.file_size {
            // Rotation or truncation -> start over on the new content. Re-reading
            // from 0 goes through the same tail-first path as opening the file,
            // so a huge rotated-in file still shows its tail instantly.
            self.reset_state(st.size);
            self.events.on_reset();
            return self.perform_initial_read();
        }

        if was_in_error {
            self.events.on_ready();
        }
        if st.size == self.offset {
            return None; // nothing new
        }
        self.pump(self.offset, st.size, true);
        self.file_size = st.size;
        None
    }

    /// Manual refresh: discard state and re-read from scratch.
    pub fn refresh(&mut self) -> Option<HeadScan> {
        self.reset_state(0);
        self.events.on_reset();
        self.perform_initial_read()
    }

    /// Adopts a background head index: records the absolute offsets for the head
    /// region and the line count before the tail, then notifies so the UI can
    /// swap placeholder gutters for real numbers. Returns false (and does
    /// nothing) if `token` is not the current scan — i.e. the file was re-read
    /// meanwhile and the result is stale.
    pub fn apply_head_count(
        &mut self,
        token: &Arc<CancelToken>,
        checkpoints: Vec<i64>,
        base: i64,
    ) -> bool {
        match &self.head_count_token {
            Some(cur) if Arc::ptr_eq(cur, token) && !token.is_cancelled() => {}
            _ => return false,
        }
        self.head_count_token = None;
        self.head_checkpoints = checkpoints;
        self.base = base;
        self.base_known = true;
        self.sync_counters();
        self.events.on_base_resolved(base);
        true
    }

    /// Reads `count` lines starting at 1-based absolute `start` directly from
    /// disk, seeking to the nearest checkpoint (head or tail region) and
    /// scanning forward. Returns nothing until the head count is known.
    pub fn read_range(&mut self, start: i64, count: usize) -> Vec<LogLine> {
        let total = self.total_lines();
        if !self.base_known || start < 1 || count == 0 || start > total {
            return Vec::new();
        }
        let last_line = total.min(start + count as i64 - 1);
        let Some((from_byte, line_at_byte)) = self.checkpoint_at_or_before(start) else {
            return Vec::new();
        };
        let to_byte = self.checkpoint_after(last_line).unwrap_or(self.file_size);
        let Some(data) = self.read_bytes(from_byte, to_byte) else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity((last_line - start + 1) as usize);
        for (i, line) in split_lines(&data, 0, 0).lines.into_iter().enumerate() {
            let num = line_at_byte + i as i64;
            if num > last_line {
                break;
            }
            if num >= start {
                out.push(LogLine {
                    number: num,
                    text: line.text,
                });
            }
        }
        out
    }

    /// Cancels any in-flight background head count.
    pub fn cancel_head_count(&mut self) {
        if let Some(token) = self.head_count_token.take() {
            token.cancel();
        }
    }

    // --- internals -------------------------------------------------------

    /// Reads `[from, to)` in bounded chunks, emitting lines per chunk so a huge
    /// gap (a burst append) never allocates more than `max_read_chunk` at once.
    /// Advances `offset` as it goes. A line straddling a chunk boundary is
    /// re-read whole from the next chunk (`consumed` lands on a line boundary);
    /// a single line longer than a chunk — pathological for a log — falls back
    /// to one full read so it's never split.
    fn pump(&mut self, from: i64, to: i64, build_tail_index: bool) {
        let mut cursor = from;
        while cursor < to {
            let chunk_end = to.min(cursor + self.opts.max_read_chunk);
            let (lines, consumed) = self.read_new_lines(cursor, chunk_end, build_tail_index);
            if consumed <= cursor {
                // No complete line in this chunk.
                if chunk_end < to {
                    // An over-long line spans past the cap — read it whole.
                    let (rest, rest_consumed) = self.read_new_lines(cursor, to, build_tail_index);
                    self.offset = rest_consumed;
                    self.emit_lines(rest);
                }
                break; // else: a trailing partial awaits more data
            }
            cursor = consumed;
            self.offset = consumed;
            self.emit_lines(lines);
        }
    }

    fn reset_state(&mut self, new_size: i64) {
        self.cancel_head_count(); // any in-flight head count is now stale
        self.line_num = 0;
        self.offset = 0;
        self.file_size = new_size;
        self.identity = 0;
        self.tail_start = 0;
        self.base = 0;
        self.base_known = true;
        self.head_checkpoints.clear();
        self.tail_checkpoints.clear();
        self.sync_counters();
    }

    /// Byte offset + the absolute line number there, for the checkpoint at or
    /// just before `abs_line`. Resolves head vs. tail region.
    fn checkpoint_at_or_before(&self, abs_line: i64) -> Option<(i64, i64)> {
        let s = self.opts.index_stride;
        if abs_line <= self.base {
            let k = ((abs_line - 1) / s) as usize;
            let byte = *self.head_checkpoints.get(k)?;
            Some((byte, k as i64 * s + 1))
        } else {
            let local = abs_line - self.base;
            let k = ((local - 1) / s) as usize;
            let byte = *self.tail_checkpoints.get(k)?;
            Some((byte, self.base + k as i64 * s + 1))
        }
    }

    /// Byte offset of the first checkpoint strictly after `abs_line`, or None (= EOF).
    fn checkpoint_after(&self, abs_line: i64) -> Option<i64> {
        let s = self.opts.index_stride;
        if abs_line < self.base {
            let k = (abs_line / s) as usize + 1;
            if let Some(&b) = self.head_checkpoints.get(k) {
                return Some(b);
            }
            self.tail_checkpoints.first().copied() // crossing into the tail at tail_start
        } else {
            let local = abs_line - self.base;
            let k = (local / s) as usize + 1;
            self.tail_checkpoints.get(k).copied()
        }
    }

    /// Reads `[from, to)`, splits complete lines numbered absolutely
    /// (`base + local`), advances the LOCAL `line_num`, and appends sparse tail
    /// checkpoints.
    fn read_new_lines(
        &mut self,
        from: i64,
        to: i64,
        build_tail_index: bool,
    ) -> (Vec<LogLine>, i64) {
        let Some(data) = self.read_bytes(from, to) else {
            return (Vec::new(), from);
        };
        if data.is_empty() {
            return (Vec::new(), from);
        }
        let split = split_lines(&data, self.base + self.line_num, from);
        if let Some(last) = split.lines.last() {
            self.line_num = last.number - self.base; // absolute -> local
            if build_tail_index {
                let s = self.opts.index_stride;
                for (line, &off) in split.lines.iter().zip(&split.offsets) {
                    if (line.number - self.base - 1) % s == 0 {
                        self.tail_checkpoints.push(off);
                    }
                }
            }
        }
        (split.lines, split.consumed)
    }

    fn align_to_line_boundary(&mut self, start: i64) -> i64 {
        if start <= 0 {
            return start;
        }
        match self.read_bytes(start, start + 64 * 1024) {
            Some(data) => match memchr(b'\n', &data) {
                Some(i) => start + i as i64 + 1,
                None => start,
            },
            None => start,
        }
    }

    // --- I/O (all under a timeout so dead mounts can't wedge the engine) ---

    fn stat(&mut self) -> Option<Stat> {
        let path = self.path.clone();
        self.io.run(move || stat_path(&path)).flatten()
    }

    fn read_bytes(&mut self, from: i64, to: i64) -> Option<Vec<u8>> {
        if to <= from {
            return Some(Vec::new());
        }
        let path = self.path.clone();
        self.io
            .run(move || {
                let file = File::open(&*path).ok()?;
                let len = (to - from) as usize;
                let mut buf = vec![0u8; len];
                let n = pread_full(&file, from as u64, &mut buf);
                buf.truncate(n);
                Some(buf)
            })
            .flatten()
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.cancel_head_count();
    }
}

// ---------------------------------------------------------------------------
// Tailer: threaded driver
// ---------------------------------------------------------------------------

enum Cmd {
    Start,
    Stop,
    Refresh,
    SetPollInterval(Duration),
    FetchRange {
        start: i64,
        count: usize,
        reply: Box<dyn FnOnce(Vec<LogLine>) + Send>,
    },
    HeadScanDone {
        token: Arc<CancelToken>,
        result: IndexResult,
    },
    Shutdown,
}

/// The tailer handle the UI owns. All work happens on a worker thread; the
/// handle is cheap to poke from anywhere. Dropping it stops everything.
pub struct Tailer {
    tx: Sender<Cmd>,
    counters: Arc<Counters>,
    worker: Option<thread::JoinHandle<()>>,
}

impl Tailer {
    pub fn new(
        path: impl Into<PathBuf>,
        opts: TailerOptions,
        events: Arc<dyn TailerEvents>,
    ) -> Self {
        let engine = Engine::new(path, opts, events);
        let counters = engine.counters();
        counters.indexing_complete.store(true, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel::<Cmd>();
        let worker = {
            let tx = tx.clone();
            thread::Builder::new()
                .name("ctail-tailer".into())
                .spawn(move || Worker::run(engine, rx, tx))
                .expect("spawn ctail-tailer thread")
        };
        Self {
            tx,
            counters,
            worker: Some(worker),
        }
    }

    /// Begins reading (tail-first for large files) and live polling. After a
    /// `stop`, `start` just resumes polling; the next poll picks up whatever
    /// changed meanwhile (including a rotation).
    pub fn start(&self) {
        let _ = self.tx.send(Cmd::Start);
    }
    /// Pauses polling; `start` resumes without re-reading.
    pub fn stop(&self) {
        let _ = self.tx.send(Cmd::Stop);
    }
    /// Discards state and re-reads from scratch.
    pub fn refresh(&self) {
        let _ = self.tx.send(Cmd::Refresh);
    }
    /// Adjusts the poll cadence at runtime (slow inactive/backgrounded tabs).
    pub fn set_poll_interval(&self, interval: Duration) {
        let _ = self.tx.send(Cmd::SetPollInterval(interval));
    }
    /// Reads `count` lines from 1-based `start` and hands them to `reply` on
    /// the worker thread. Empty until indexing is complete.
    pub fn fetch_range(
        &self,
        start: i64,
        count: usize,
        reply: impl FnOnce(Vec<LogLine>) + Send + 'static,
    ) {
        let _ = self.tx.send(Cmd::FetchRange {
            start,
            count,
            reply: Box::new(reply),
        });
    }
    /// Total lines known so far (absolute once the base is resolved).
    pub fn total_lines(&self) -> i64 {
        self.counters.total_lines()
    }
    /// Whether absolute line numbers / scrollback are available yet.
    pub fn indexing_complete(&self) -> bool {
        self.counters.indexing_complete()
    }
}

impl Drop for Tailer {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Shutdown);
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}

struct Worker {
    engine: Engine,
    tx: Sender<Cmd>,
    running: bool,
    opened: bool, // initial read done at least once
    interval: Duration,
    next_poll: Option<Instant>,
}

impl Worker {
    fn run(engine: Engine, rx: mpsc::Receiver<Cmd>, tx: Sender<Cmd>) {
        let interval = engine.options().poll_interval;
        let mut w = Worker {
            engine,
            tx,
            running: false,
            opened: false,
            interval,
            next_poll: None,
        };
        loop {
            let wait = match (w.running, w.next_poll) {
                (true, Some(t)) => t.saturating_duration_since(Instant::now()),
                _ => Duration::from_secs(3600),
            };
            match rx.recv_timeout(wait) {
                Ok(Cmd::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(cmd) => w.handle(cmd),
                Err(RecvTimeoutError::Timeout) => {
                    if w.running {
                        let scan = w.engine.perform_poll();
                        w.spawn_scan(scan);
                        w.next_poll = Some(Instant::now() + w.interval);
                    }
                }
            }
        }
        w.engine.cancel_head_count();
    }

    fn handle(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Start => {
                if !self.running {
                    self.running = true;
                    if !self.opened {
                        self.opened = true;
                        let scan = self.engine.perform_initial_read(); // shows + follows the tail at once
                        self.spawn_scan(scan);
                        self.engine_events().on_ready();
                    }
                    self.next_poll = Some(Instant::now() + self.interval);
                }
            }
            Cmd::Stop => {
                self.running = false;
                self.next_poll = None;
            }
            Cmd::Refresh => {
                let scan = self.engine.refresh();
                self.spawn_scan(scan);
                if self.running && self.next_poll.is_none() {
                    self.next_poll = Some(Instant::now() + self.interval);
                }
            }
            Cmd::SetPollInterval(d) => {
                let clamped = d.max(Duration::from_millis(50));
                if self.running && clamped != self.interval {
                    self.interval = clamped;
                    self.next_poll = Some(Instant::now() + clamped);
                }
            }
            Cmd::FetchRange {
                start,
                count,
                reply,
            } => {
                reply(self.engine.read_range(start, count));
            }
            Cmd::HeadScanDone { token, result } => {
                // Dropped by the engine if the file was re-read meanwhile.
                self.engine
                    .apply_head_count(&token, result.checkpoints, result.total);
            }
            Cmd::Shutdown => {}
        }
    }

    fn spawn_scan(&self, scan: Option<HeadScan>) {
        let Some(scan) = scan else { return };
        let tx = self.tx.clone();
        thread::Builder::new()
            .name("ctail-index".into())
            .spawn(move || {
                let result = scan.run();
                if !scan.token.is_cancelled() {
                    let _ = tx.send(Cmd::HeadScanDone {
                        token: scan.token.clone(),
                        result,
                    });
                }
            })
            .expect("spawn ctail-index thread");
    }

    fn engine_events(&self) -> Arc<dyn TailerEvents> {
        self.engine.events.clone()
    }
}
