//! Parity suite: mirrors the Swift `SelfTest.tailerSuite` (and a few Go cases)
//! so the Rust engine is checked against the behaviour the app ships with.

use ctail_core::*;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

// --- fixtures ---------------------------------------------------------------

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "ctail-core-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Rotates a log the way logrotate does: the old file is moved aside and the
/// path is free for a new one. Deleting it instead would not do on Linux,
/// where ext4 hands the freed inode straight to the next file created, and a
/// recreate inside the same clock tick gets the old birth time too — the
/// engine would be right to call that the same file, because by every fact
/// the file system offers, it is.
fn rotate(file: &Path) {
    let aside = file.with_extension("log.1");
    fs::rename(file, aside).unwrap();
}

fn write(path: &Path, s: &str) {
    fs::write(path, s).unwrap();
}

fn append(path: &Path, s: &str) {
    let mut f = fs::OpenOptions::new().append(true).open(path).unwrap();
    f.write_all(s.as_bytes()).unwrap();
}

#[derive(Debug, PartialEq, Eq, Clone)]
enum Ev {
    Lines(Vec<LogLine>),
    Reset,
    Error(String),
    Ready,
    Base(i64),
}

#[derive(Default)]
struct Recorder {
    events: Mutex<Vec<Ev>>,
    notify: Mutex<Option<mpsc::Sender<Ev>>>,
}

impl Recorder {
    fn push(&self, ev: Ev) {
        if let Some(tx) = &*self.notify.lock().unwrap() {
            let _ = tx.send(ev.clone());
        }
        self.events.lock().unwrap().push(ev);
    }
    fn take(&self) -> Vec<Ev> {
        std::mem::take(&mut *self.events.lock().unwrap())
    }
    fn texts(&self) -> Vec<String> {
        self.take()
            .into_iter()
            .filter_map(|e| match e {
                Ev::Lines(l) => Some(l),
                _ => None,
            })
            .flatten()
            .map(|l| l.text)
            .collect()
    }
}

impl TailerEvents for Recorder {
    fn on_lines(&self, lines: Vec<LogLine>) {
        self.push(Ev::Lines(lines));
    }
    fn on_reset(&self) {
        self.push(Ev::Reset);
    }
    fn on_error(&self, m: String) {
        self.push(Ev::Error(m));
    }
    fn on_ready(&self) {
        self.push(Ev::Ready);
    }
    fn on_base_resolved(&self, b: i64) {
        self.push(Ev::Base(b));
    }
}

fn engine(path: &Path, opts: TailerOptions) -> (Engine, Arc<Recorder>) {
    let rec = Arc::new(Recorder::default());
    (Engine::new(path, opts, rec.clone()), rec)
}

fn texts(lines: &[LogLine]) -> Vec<&str> {
    lines.iter().map(|l| l.text.as_str()).collect()
}

fn small_opts() -> TailerOptions {
    TailerOptions {
        poll_interval: Duration::from_millis(50),
        ..Default::default()
    }
}

// --- pure line splitter -----------------------------------------------------

#[test]
fn split_lines_drops_partial_and_numbers_from_one() {
    let r = split_lines(b"a\nb\npartial", 0, 0);
    assert_eq!(
        texts(&r.lines),
        ["a", "b"],
        "splits complete lines, drops partial"
    );
    assert_eq!(r.lines.iter().map(|l| l.number).collect::<Vec<_>>(), [1, 2]);
    assert_eq!(r.offsets, [0, 2], "line start offsets");
    assert_eq!(r.consumed, 4, "consumed stops before the partial line");
}

#[test]
fn split_lines_strips_cr_and_continues_numbering() {
    let r = split_lines(b"x\r\ny\r\n", 10, 100);
    assert_eq!(texts(&r.lines), ["x", "y"], "strips trailing CR");
    assert_eq!(
        r.lines.iter().map(|l| l.number).collect::<Vec<_>>(),
        [11, 12]
    );
    assert_eq!(r.offsets, [100, 103], "offsets are base_offset-relative");
    assert_eq!(r.consumed, 106);
}

#[test]
fn split_lines_empty_and_invalid_utf8() {
    assert_eq!(
        split_lines(b"", 5, 0).lines.len(),
        0,
        "empty buffer yields no lines"
    );
    let r = split_lines(b"ok\n\xff\xfe bad\n", 0, 0);
    assert_eq!(r.lines.len(), 2);
    assert_eq!(
        r.lines[1].text, "\u{FFFD}\u{FFFD} bad",
        "invalid UTF-8 is repaired, not dropped"
    );
}

// --- file-driven engine -----------------------------------------------------

#[test]
fn initial_read_append_partial_truncate_rotate() {
    let dir = TempDir::new();
    let file = dir.file("app.log");
    write(&file, "line1\nline2\nline3\n");
    let (mut t, rec) = engine(&file, small_opts());

    assert!(
        t.perform_initial_read().is_none(),
        "small file: no head scan"
    );
    assert_eq!(t.total_lines(), 3, "initial read indexes 3 lines");
    assert_eq!(rec.texts(), ["line1", "line2", "line3"]);
    assert_eq!(
        texts(&t.read_range(1, 3)),
        ["line1", "line2", "line3"],
        "read_range full"
    );
    assert_eq!(texts(&t.read_range(2, 1)), ["line2"], "read_range windowed");

    // Append -> poll picks up only the new line.
    append(&file, "line4\n");
    t.perform_poll();
    assert_eq!(t.total_lines(), 4, "poll appends new line to index");
    assert_eq!(rec.texts(), ["line4"]);
    assert_eq!(texts(&t.read_range(4, 1)), ["line4"], "new line readable");

    // Partial line not committed until its newline arrives.
    append(&file, "partial-no-newline");
    t.perform_poll();
    assert_eq!(t.total_lines(), 4, "partial line not counted yet");
    assert!(rec.take().is_empty(), "no event for a partial line");
    append(&file, "\n");
    t.perform_poll();
    assert_eq!(t.total_lines(), 5, "partial completes on newline");
    assert_eq!(
        texts(&t.read_range(5, 1)),
        ["partial-no-newline"],
        "completed partial text"
    );

    // Truncation -> index resets and re-reads.
    write(&file, "fresh1\nfresh2\n");
    t.perform_poll();
    assert_eq!(t.total_lines(), 2, "truncation resets to new content");
    assert_eq!(
        rec.take(),
        [
            Ev::Lines(vec![LogLine {
                number: 5,
                text: "partial-no-newline".into()
            }]),
            Ev::Reset,
            Ev::Lines(vec![
                LogLine {
                    number: 1,
                    text: "fresh1".into()
                },
                LogLine {
                    number: 2,
                    text: "fresh2".into()
                }
            ]),
        ],
        "reset fires before the re-read"
    );
    assert_eq!(
        texts(&t.read_range(1, 2)),
        ["fresh1", "fresh2"],
        "post-truncation content"
    );

    // Rotation (different inode) -> treated like truncation/reset.
    rotate(&file);
    write(&file, "rotated\n");
    t.perform_poll();
    assert_eq!(t.total_lines(), 1, "rotation re-reads new inode");
    assert_eq!(texts(&t.read_range(1, 1)), ["rotated"], "rotated content");

    // A second rotation with a *larger* file must still be detected (the Swift
    // engine lost the inode after the first reset and only saw shrinks).
    rotate(&file);
    write(&file, "rotated-again-1\nrotated-again-2\n");
    t.perform_poll();
    assert_eq!(
        t.total_lines(),
        2,
        "second rotation detected by inode even though the file grew"
    );
    assert_eq!(
        texts(&t.read_range(1, 2)),
        ["rotated-again-1", "rotated-again-2"]
    );
}

#[test]
fn error_reported_once_and_ready_on_recovery() {
    let dir = TempDir::new();
    let file = dir.file("app.log");
    write(&file, "a\n");
    let (mut t, rec) = engine(&file, small_opts());
    t.perform_initial_read();
    rec.take();

    fs::remove_file(&file).unwrap();
    t.perform_poll();
    t.perform_poll();
    let evs = rec.take();
    assert_eq!(evs.len(), 1, "error reported once per outage: {evs:?}");
    assert!(matches!(&evs[0], Ev::Error(m) if m.contains("file unavailable")));

    // The file comes back (new inode) -> ready + reset + content.
    write(&file, "b\n");
    t.perform_poll();
    let evs = rec.take();
    assert!(evs.contains(&Ev::Reset), "recreated file resets: {evs:?}");
    assert_eq!(t.total_lines(), 1);
    assert_eq!(texts(&t.read_range(1, 1)), ["b"]);
}

// --- sparse offset indexer --------------------------------------------------

#[test]
fn index_file_dense_sparse_partial_and_cancel() {
    let dir = TempDir::new();
    let file = dir.file("idx.log");
    write(&file, "aa\nbb\ncc\n"); // line starts at bytes 0,3,6; size 9
    let never = || false;

    let dense = index_file(&file, 9, 1, &never);
    assert_eq!(
        dense.checkpoints,
        [0, 3, 6],
        "stride 1 records every line start"
    );
    assert_eq!(dense.total, 3, "counts all complete lines");
    assert_eq!(dense.consumed, 9, "consumed stops past the last newline");

    let sparse = index_file(&file, 9, 2, &never);
    assert_eq!(
        sparse.checkpoints,
        [0, 6],
        "stride 2 keeps every 2nd line start"
    );
    assert_eq!(sparse.total, 3, "sparse index still counts all lines");

    write(&file, "aa\nbb\npartial");
    let partial = index_file(&file, 12, 1, &never);
    assert_eq!(partial.total, 2, "trailing partial line not counted");
    assert_eq!(
        partial.consumed, 6,
        "consumed excludes the trailing partial"
    );

    let aborted = index_file(&file, 1_000, 1000, &|| true);
    assert_eq!(aborted.total, 0, "a pre-cancelled scan does no work");

    let missing = index_file(&dir.file("nope.log"), 10, 1, &never);
    assert_eq!(
        missing,
        IndexResult {
            checkpoints: vec![0],
            total: 0,
            consumed: 0
        }
    );
}

#[test]
fn index_file_matches_split_lines_across_chunk_boundaries() {
    // Lines straddling the indexer's 1 MiB chunk boundary must be counted and
    // checkpointed exactly like a single-buffer split would.
    let dir = TempDir::new();
    let file = dir.file("chunks.log");
    let mut body = String::new();
    for n in 1..=40_000 {
        body.push_str(&format!("line-{n}-{}\n", "x".repeat(n % 97)));
    }
    write(&file, &body);
    let size = body.len() as i64;
    let idx = index_file(&file, size, 1000, &|| false);
    let split = split_lines(body.as_bytes(), 0, 0);
    assert_eq!(idx.total, split.lines.len() as i64);
    assert_eq!(idx.consumed, split.consumed);
    let expected: Vec<i64> = split.offsets.iter().step_by(1000).copied().collect();
    assert_eq!(idx.checkpoints, expected);
}

#[test]
fn index_file_irregular_lines_match_split_lines_for_many_strides() {
    // Empty lines, CRLF, very long lines and a trailing partial, checked for a
    // range of strides so block/chunk/bisection boundaries all get exercised.
    let dir = TempDir::new();
    let file = dir.file("irregular.log");
    let mut body = Vec::new();
    let mut seed = 12345u64;
    for n in 0..60_000u64 {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let len = match seed >> 60 {
            0 => 0,
            1..=9 => (seed >> 40) as usize % 120,
            _ => (seed >> 40) as usize % 5000,
        };
        body.extend(std::iter::repeat_n(b'a' + (n % 26) as u8, len));
        body.extend_from_slice(if n % 3 == 0 { b"\r\n" } else { b"\n" });
    }
    body.extend_from_slice(b"trailing partial without newline");
    fs::write(&file, &body).unwrap();
    let size = body.len() as i64;
    let split = split_lines(&body, 0, 0);
    // Like the Swift indexer, a checkpoint may land on the start of a trailing
    // partial line (it is simply never reached while `total` excludes it).
    let mut starts = split.offsets.clone();
    starts.push(split.consumed);
    for stride in [1, 2, 3, 7, 100, 1000, 4096, 1_000_000] {
        let idx = index_file(&file, size, stride, &|| false);
        assert_eq!(
            idx.total,
            split.lines.len() as i64,
            "stride {stride}: total"
        );
        assert_eq!(idx.consumed, split.consumed, "stride {stride}: consumed");
        let expected: Vec<i64> = starts.iter().step_by(stride as usize).copied().collect();
        assert_eq!(idx.checkpoints, expected, "stride {stride}: checkpoints");
    }
    // Truncated scans (up_to inside the file) must stop exactly there.
    let idx = index_file(&file, 100_000, 50, &|| false);
    let head = split_lines(&body[..100_000], 0, 0);
    assert_eq!(idx.total, head.lines.len() as i64);
    assert_eq!(idx.consumed, head.consumed);
    assert_eq!(
        idx.checkpoints,
        head.offsets.iter().step_by(50).copied().collect::<Vec<_>>()
    );
}

// --- windowed reads over the sparse index -----------------------------------

#[test]
fn read_range_seeks_via_checkpoints() {
    let dir = TempDir::new();
    let file = dir.file("big.log");
    let mut big = String::new();
    for n in 1..=2500 {
        big.push_str(&format!("L{n}\n"));
    }
    write(&file, &big);
    let (mut bt, _) = engine(&file, small_opts());
    bt.perform_initial_read();
    assert_eq!(
        bt.total_lines(),
        2500,
        "sparse-indexed file counts all lines"
    );
    assert_eq!(
        texts(&bt.read_range(1, 1)),
        ["L1"],
        "first line via checkpoint 0"
    );
    assert_eq!(
        texts(&bt.read_range(1500, 2)),
        ["L1500", "L1501"],
        "scanned forward from checkpoint 1"
    );
    assert_eq!(
        texts(&bt.read_range(2001, 1)),
        ["L2001"],
        "exact checkpoint-boundary line"
    );
    assert_eq!(texts(&bt.read_range(2500, 1)), ["L2500"], "last line");
    assert_eq!(
        texts(&bt.read_range(999, 3)),
        ["L999", "L1000", "L1001"],
        "window across a checkpoint"
    );
    assert!(bt.read_range(2501, 1).is_empty(), "past the end");
    assert!(bt.read_range(0, 1).is_empty(), "before the start");
    assert_eq!(bt.read_range(2490, 100).len(), 11, "count clamps at EOF");
}

// --- tail-first (instant tail) ---------------------------------------------

#[test]
fn tail_first_defers_count_then_resolves_absolute_numbers() {
    let dir = TempDir::new();
    let file = dir.file("tf.log");
    let mut body = String::new();
    for n in 1..=50 {
        body.push_str(&format!("L{n}\n"));
    }
    write(&file, &body);
    // Tiny thresholds force the large-file tail-first path on a small fixture.
    let opts = TailerOptions {
        tail_first_threshold: 20,
        tail_seek_back: 12,
        ..small_opts()
    };
    let (mut tf, rec) = engine(&file, opts);
    let scan = tf
        .perform_initial_read()
        .expect("large file returns a head scan");
    assert!(
        !tf.indexing_complete(),
        "tail-first defers the full line count"
    );
    assert!(
        tf.total_lines() < 50,
        "before count, total is just the local tail"
    );
    let first = rec.texts();
    // Before the count lands the tail is readable by its local numbers.
    let provisional = tf.read_range(1, 1);
    assert_eq!(provisional.len(), 1, "provisional tail read");
    assert_eq!(provisional[0].number, 1);
    assert_eq!(
        provisional[0].text, first[0],
        "local line 1 is the first tail line"
    );
    assert!(
        tf.read_range(tf.total_lines() + 1, 1).is_empty(),
        "nothing past the local tail"
    );
    assert!(
        first[0].starts_with('L') && first.last().unwrap() == "L50",
        "tail shown immediately: {first:?}"
    );
    let local_total = first.len() as i64;

    // Run the head count "in the background" and apply it.
    let head = scan.run();
    assert!(tf.apply_head_count(&scan.token, head.checkpoints.clone(), head.total));
    assert!(tf.indexing_complete(), "count ready after head index");
    assert_eq!(tf.total_lines(), 50, "absolute total after head count");
    assert_eq!(head.total + local_total, 50);
    assert_eq!(rec.take(), [Ev::Base(head.total)]);
    assert_eq!(
        texts(&tf.read_range(1, 1)),
        ["L1"],
        "head-region line via scrollback"
    );
    assert_eq!(
        texts(&tf.read_range(head.total + 1, 1)),
        [format!("L{}", head.total + 1).as_str()],
        "first tail line at absolute number base+1"
    );
    assert_eq!(
        texts(&tf.read_range(50, 1)),
        ["L50"],
        "tail-region last line"
    );
    let span = tf.read_range(head.total - 1, 4);
    assert_eq!(span.len(), 4, "window spanning head->tail boundary");
    assert_eq!(span[0].number, head.total - 1);

    // New lines after init are numbered absolutely once based.
    append(&file, "L51\n");
    tf.perform_poll();
    assert_eq!(tf.total_lines(), 51);
    assert_eq!(
        rec.take(),
        [Ev::Lines(vec![LogLine {
            number: 51,
            text: "L51".into()
        }])]
    );

    // A stale token (superseded by a refresh) is rejected, and a refresh while a
    // scan is still pending cancels it.
    let scan2 = tf.refresh().expect("refresh re-opens tail-first");
    assert!(
        !tf.apply_head_count(&scan.token, vec![0], 1),
        "stale head count dropped"
    );
    assert!(!tf.indexing_complete());
    let scan3 = tf.refresh().expect("second refresh re-opens tail-first");
    assert!(scan2.token.is_cancelled(), "refresh cancels a pending scan");
    assert!(
        !tf.apply_head_count(&scan2.token, vec![0], 1),
        "cancelled scan result dropped"
    );
    let head3 = scan3.run();
    assert!(tf.apply_head_count(&scan3.token, head3.checkpoints, head3.total));
    assert_eq!(tf.total_lines(), 51);
}

#[test]
fn rotation_into_large_file_reopens_tail_first() {
    let dir = TempDir::new();
    let file = dir.file("rot.log");
    write(&file, "small\n");
    let opts = TailerOptions {
        tail_first_threshold: 20,
        tail_seek_back: 12,
        ..small_opts()
    };
    let (mut t, rec) = engine(&file, opts);
    assert!(t.perform_initial_read().is_none());
    rec.take();
    let mut body = String::new();
    for n in 1..=50 {
        body.push_str(&format!("R{n}\n"));
    }
    rotate(&file);
    write(&file, &body);
    let scan = t
        .perform_poll()
        .expect("rotated-in large file is read tail-first");
    assert!(!t.indexing_complete());
    let evs = rec.take();
    assert_eq!(evs[0], Ev::Reset);
    let head = scan.run();
    assert!(t.apply_head_count(&scan.token, head.checkpoints, head.total));
    assert_eq!(t.total_lines(), 50);
    assert_eq!(texts(&t.read_range(1, 1)), ["R1"]);
}

// --- chunked pump -----------------------------------------------------------

#[test]
fn pump_handles_chunk_straddles_and_overlong_lines() {
    let dir = TempDir::new();
    let file = dir.file("pump.log");
    write(&file, "seed\n");
    // A tiny chunk (7 bytes) forces many lines to straddle chunk boundaries.
    let opts = TailerOptions {
        max_read_chunk: 7,
        ..small_opts()
    };
    let (mut pumped, rec) = engine(&file, opts);
    pumped.perform_initial_read();
    assert_eq!(pumped.total_lines(), 1, "pump: initial seed line");
    rec.take();
    let mut burst = String::new();
    for n in 1..=200 {
        burst.push_str(&format!("entry-{n}\n")); // each line > the 7-byte chunk
    }
    append(&file, &burst);
    pumped.perform_poll();
    assert_eq!(
        pumped.total_lines(),
        201,
        "pump: all burst lines counted across chunk boundaries"
    );
    let got = rec.texts();
    assert_eq!(got.len(), 200);
    assert_eq!(got[0], "entry-1");
    assert_eq!(got[199], "entry-200");
    assert_eq!(
        texts(&pumped.read_range(2, 1)),
        ["entry-1"],
        "pump: first burst line intact"
    );
    assert_eq!(
        texts(&pumped.read_range(201, 1)),
        ["entry-200"],
        "pump: last burst line intact"
    );
    assert_eq!(
        texts(&pumped.read_range(50, 2)),
        ["entry-49", "entry-50"],
        "pump: mid-burst lines intact"
    );

    // A single line longer than max_read_chunk is never split.
    let long_line = "x".repeat(100);
    append(&file, &format!("{long_line}\n"));
    pumped.perform_poll();
    assert_eq!(pumped.total_lines(), 202, "pump: over-long line counted");
    assert_eq!(
        texts(&pumped.read_range(202, 1)),
        [long_line.as_str()],
        "pump: over-long line intact"
    );
}

// --- threaded driver --------------------------------------------------------

#[test]
fn threaded_tailer_delivers_initial_then_live_lines() {
    let dir = TempDir::new();
    let file = dir.file("live.log");
    write(&file, "one\ntwo\n");
    let rec = Arc::new(Recorder::default());
    let (tx, rx) = mpsc::channel();
    *rec.notify.lock().unwrap() = Some(tx);
    let opts = TailerOptions {
        poll_interval: Duration::from_millis(50),
        ..Default::default()
    };
    let tailer = Tailer::new(&file, opts, rec.clone());
    tailer.start();

    let wait = |what: &str| {
        rx.recv_timeout(Duration::from_secs(5))
            .unwrap_or_else(|_| panic!("timeout: {what}"))
    };
    match wait("initial lines") {
        Ev::Lines(l) => assert_eq!(texts(&l), ["one", "two"]),
        other => panic!("expected lines, got {other:?}"),
    }
    assert_eq!(wait("ready"), Ev::Ready);
    assert_eq!(tailer.total_lines(), 2);
    assert!(tailer.indexing_complete());

    append(&file, "three\n");
    match wait("live line") {
        Ev::Lines(l) => assert_eq!(
            l,
            [LogLine {
                number: 3,
                text: "three".into()
            }]
        ),
        other => panic!("expected lines, got {other:?}"),
    }
    assert_eq!(tailer.total_lines(), 3);

    let (rtx, rrx) = mpsc::channel();
    tailer.fetch_range(2, 2, move |lines| rtx.send(lines).unwrap());
    let got = rrx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(texts(&got), ["two", "three"]);

    // Stop pauses polling: an append is not delivered until start resumes.
    tailer.stop();
    std::thread::sleep(Duration::from_millis(120));
    append(&file, "four\n");
    assert!(
        rx.recv_timeout(Duration::from_millis(200)).is_err(),
        "no delivery while stopped"
    );
    tailer.start();
    match wait("resumed line") {
        Ev::Lines(l) => assert_eq!(texts(&l), ["four"]),
        other => panic!("expected lines, got {other:?}"),
    }
    drop(tailer);
}

#[test]
fn threaded_tailer_resolves_base_in_background() {
    let dir = TempDir::new();
    let file = dir.file("bg.log");
    let mut body = String::new();
    for n in 1..=5000 {
        body.push_str(&format!("B{n}\n"));
    }
    write(&file, &body);
    let rec = Arc::new(Recorder::default());
    let (tx, rx) = mpsc::channel();
    *rec.notify.lock().unwrap() = Some(tx);
    let opts = TailerOptions {
        poll_interval: Duration::from_millis(50),
        tail_first_threshold: 1000,
        tail_seek_back: 300,
        ..Default::default()
    };
    let tailer = Tailer::new(&file, opts, rec.clone());
    tailer.start();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut saw_base = None;
    while saw_base.is_none() && std::time::Instant::now() < deadline {
        if let Ok(Ev::Base(b)) = rx.recv_timeout(Duration::from_millis(100)) {
            saw_base = Some(b);
        }
    }
    let base = saw_base.expect("base resolved");
    assert!(base > 0 && base < 5000);
    assert_eq!(tailer.total_lines(), 5000);
    assert!(tailer.indexing_complete());
    let (rtx, rrx) = mpsc::channel();
    tailer.fetch_range(1, 2, move |l| rtx.send(l).unwrap());
    assert_eq!(
        texts(&rrx.recv_timeout(Duration::from_secs(5)).unwrap()),
        ["B1", "B2"]
    );
}
