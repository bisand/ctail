//! Tail-engine benchmark. Mirrors `macos/scripts/tailbench/main.swift` so the
//! Rust and Swift engines can be measured on the same file.
//!
//!   cargo run --release -p ctail-core --example tailbench -- --gen 2G [--cold] [--file PATH]
//!   cargo run --release -p ctail-core --example tailbench -- --file /path/to/existing.log
//!
//! `--gen SIZE` writes a synthetic log of roughly SIZE bytes (K/M/G suffix).
//! `--cold` writes it with F_NOCACHE (macOS) so the run measures a cold page
//! cache; without it (or on a second run) the file is warm. `--gen-only` stops
//! after writing, e.g. to hand a cold file to the Swift harness.
//! Reports: time to first tail lines, time until the head count lands (absolute
//! numbers available), scrollback page-in latency, and peak RSS.

use ctail_core::{LogLine, Tailer, TailerEvents, TailerOptions};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Mutex;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

struct Probe {
    t0: Instant,
    first_lines: OnceLock<(Duration, usize)>,
    base: OnceLock<(Duration, i64)>,
    lines_seen: Mutex<i64>,
    tx: mpsc::Sender<()>,
}

impl TailerEvents for Probe {
    fn on_lines(&self, lines: Vec<LogLine>) {
        *self.lines_seen.lock().unwrap() += lines.len() as i64;
        let _ = self.first_lines.set((self.t0.elapsed(), lines.len()));
    }
    fn on_base_resolved(&self, base: i64) {
        let _ = self.base.set((self.t0.elapsed(), base));
        let _ = self.tx.send(());
    }
    fn on_ready(&self) {
        let _ = self.tx.send(());
    }
}

fn parse_size(s: &str) -> u64 {
    let (num, mult) = match s.chars().last().map(|c| c.to_ascii_uppercase()) {
        Some('K') => (&s[..s.len() - 1], 1u64 << 10),
        Some('M') => (&s[..s.len() - 1], 1u64 << 20),
        Some('G') => (&s[..s.len() - 1], 1u64 << 30),
        _ => (s, 1),
    };
    (num.parse::<f64>().expect("size") * mult as f64) as u64
}

fn generate(path: &PathBuf, size: u64, cold: bool) {
    let t = Instant::now();
    let f = File::create(path).expect("create");
    #[cfg(target_os = "macos")]
    if cold {
        use std::os::unix::io::AsRawFd;
        // Keep the freshly written file out of the page cache so the read side
        // sees a genuinely cold file.
        unsafe { libc::fcntl(f.as_raw_fd(), libc::F_NOCACHE, 1) };
    }
    #[cfg(not(target_os = "macos"))]
    let _ = cold;
    let mut w = std::io::BufWriter::with_capacity(4 << 20, f);
    let levels = ["INFO ", "DEBUG", "WARN ", "ERROR"];
    let mut written = 0u64;
    let mut n = 0u64;
    while written < size {
        let line = format!(
            "2026-09-03T12:{:02}:{:02}.{:03} {} worker-{} request id={} took {}ms path=/api/v1/items/{}\n",
            (n / 60000) % 60,
            (n / 1000) % 60,
            n % 1000,
            levels[(n % 7 % 4) as usize],
            n % 16,
            n,
            (n * 7919) % 500,
            (n * 31) % 100_000
        );
        w.write_all(line.as_bytes()).unwrap();
        written += line.len() as u64;
        n += 1;
    }
    w.flush().unwrap();
    eprintln!(
        "generated {} ({} lines, {:.1} MB/s){}",
        human(written),
        n,
        written as f64 / 1e6 / t.elapsed().as_secs_f64(),
        if cold { ", F_NOCACHE" } else { "" }
    );
}

fn human(b: u64) -> String {
    if b >= 1 << 30 {
        format!("{:.2} GB", b as f64 / (1u64 << 30) as f64)
    } else {
        format!("{:.1} MB", b as f64 / (1u64 << 20) as f64)
    }
}

fn peak_rss_mb() -> f64 {
    unsafe {
        let mut ru: libc::rusage = std::mem::zeroed();
        libc::getrusage(libc::RUSAGE_SELF, &mut ru);
        #[cfg(target_os = "macos")]
        let bytes = ru.ru_maxrss as f64;
        #[cfg(not(target_os = "macos"))]
        let bytes = ru.ru_maxrss as f64 * 1024.0;
        bytes / (1 << 20) as f64
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut file: Option<PathBuf> = None;
    let mut gen: Option<u64> = None;
    let mut cold = false;
    let mut gen_only = false;
    let mut page = 10_000usize;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--file" => {
                file = Some(PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--gen" => {
                gen = Some(parse_size(&args[i + 1]));
                i += 1;
            }
            "--cold" => cold = true,
            "--gen-only" => gen_only = true,
            "--page" => {
                page = args[i + 1].parse().unwrap();
                i += 1;
            }
            other => panic!("unknown arg {other}"),
        }
        i += 1;
    }
    let file = file.unwrap_or_else(|| std::env::temp_dir().join("ctail-bench.log"));
    if let Some(size) = gen {
        generate(&file, size, cold);
        if gen_only {
            return;
        }
    }
    let size = std::fs::metadata(&file).expect("bench file").len();
    println!("engine=rust file={} size={}", file.display(), human(size));

    let (tx, rx) = mpsc::channel();
    let probe = Arc::new(Probe {
        t0: Instant::now(),
        first_lines: OnceLock::new(),
        base: OnceLock::new(),
        lines_seen: Mutex::new(0),
        tx,
    });
    let tailer = Tailer::new(&file, TailerOptions::default(), probe.clone());
    tailer.start();

    // Wait for ready + (if tail-first) base resolved.
    let deadline = Instant::now() + Duration::from_secs(600);
    let _ = rx.recv_timeout(deadline - Instant::now()); // ready
    if !tailer.indexing_complete() {
        while Instant::now() < deadline && !tailer.indexing_complete() {
            let _ = rx.recv_timeout(Duration::from_millis(50));
        }
    }
    let (t_first, n_first) = probe
        .first_lines
        .get()
        .copied()
        .unwrap_or((Duration::ZERO, 0));
    let total = tailer.total_lines();
    println!(
        "first_lines_ms={:.2} first_batch={}",
        t_first.as_secs_f64() * 1e3,
        n_first
    );
    match probe.base.get() {
        Some((t, base)) => println!(
            "index_ms={:.1} base={} total_lines={} ({:.2} GB/s)",
            t.as_secs_f64() * 1e3,
            base,
            total,
            size as f64 / (1u64 << 30) as f64 / t.as_secs_f64()
        ),
        None => println!("index_ms=0 (small file) total_lines={total}"),
    }

    // Scrollback page-ins: head, middle, near the tail.
    for (label, start) in [
        ("head", 1),
        ("middle", total / 2),
        ("tail", (total - page as i64).max(1)),
    ] {
        let (rtx, rrx) = mpsc::channel();
        let t = Instant::now();
        tailer.fetch_range(start, page, move |l| rtx.send(l).unwrap());
        let lines = rrx.recv().unwrap();
        println!(
            "page_{label}_ms={:.2} lines={} first={}",
            t.elapsed().as_secs_f64() * 1e3,
            lines.len(),
            lines.first().map(|l| l.number).unwrap_or(0)
        );
    }
    println!("peak_rss_mb={:.1}", peak_rss_mb());
}
