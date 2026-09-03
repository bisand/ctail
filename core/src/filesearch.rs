//! Whole-file search: the scan behind a find bar's counter and its ↑/↓.
//!
//! A find bar can only match what its view holds, and a view of a tailed file
//! holds its last few thousand lines. This scans what is on disk and keeps the
//! numbers of every line that matches, so "3 of 41 892" is the truth about the
//! file rather than about the buffer, and stepping can go to a match a million
//! lines above anything loaded.
//!
//! The scanning, the waiting and the stepping are all here rather than in a
//! front end, because none of them has anything to do with how the bar is
//! drawn: a query is typed a character at a time and a scan of a large file
//! outlives several keystrokes, so a scan starts only once the typing pauses,
//! and one already running is called off rather than waited for.

use crate::search::{search_file_cancellable, SearchMatcher};
use crate::tailer::CancelToken;
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How long the typing has to stop before a file is scanned.
const SETTLE: Duration = Duration::from_millis(250);

/// What a scan is of. Anything else means the answer in hand is not an answer
/// to the question being asked.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct FileSearchQuery {
    pub path: String,
    pub text: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub is_regex: bool,
}

/// Where the search stands, for a counter to render.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
pub enum FileSearchStatus {
    /// Nothing asked for, or what is in hand answers a different query.
    Idle,
    /// A scan is running, or waiting for the typing to settle.
    Scanning,
    /// `current` is 1-based, and 0 until a match has been stepped to.
    Ready { current: u32, total: u32 },
}

/// Told when a scan finishes. Called on the scanning thread.
pub trait FileSearchEvents: Send + Sync {
    fn on_result(&self, query: FileSearchQuery, total: u32);
}

enum Cmd {
    Search(FileSearchQuery, Arc<CancelToken>),
    Stop,
}

#[derive(Default)]
struct State {
    /// What is being scanned, and the flag that calls that scan off.
    running: Option<FileSearchQuery>,
    cancel: Option<Arc<CancelToken>>,
    /// The answer in hand, and which query it answers.
    ready: Option<(FileSearchQuery, Vec<i64>)>,
    /// Index into those matches of the one the reader is on.
    current: Option<usize>,
}

/// One file scanner. Dropping it calls off any scan in flight.
pub struct FileSearch {
    tx: Sender<Cmd>,
    state: Arc<Mutex<State>>,
}

impl FileSearch {
    pub fn new(listener: Arc<dyn FileSearchEvents>) -> Self {
        let (tx, rx) = channel();
        let state = Arc::new(Mutex::new(State::default()));
        let worker = state.clone();
        std::thread::Builder::new()
            .name("ctail-filesearch".into())
            .spawn(move || run(rx, worker, listener))
            .expect("file search thread");
        Self { tx, state }
    }

    /// Asks for `query` to be scanned for, unless that is already in hand or
    /// under way. Cheap enough to call on every keystroke, which is how a
    /// front end calls it.
    pub fn request(&self, query: FileSearchQuery) {
        let mut state = self.state.lock().unwrap();
        if state.ready.as_ref().is_some_and(|(q, _)| *q == query)
            || state.running.as_ref() == Some(&query)
        {
            return;
        }
        // Whatever was being scanned is now the answer to a question nobody is
        // asking; the scan notices within a few thousand lines.
        if let Some(cancel) = state.cancel.take() {
            cancel.cancel();
        }
        let cancel = CancelToken::new();
        state.cancel = Some(cancel.clone());
        state.running = Some(query.clone());
        state.ready = None;
        state.current = None;
        drop(state);
        let _ = self.tx.send(Cmd::Search(query, cancel));
    }

    /// Forgets everything: the bar is closed, or has nothing usable in it.
    pub fn clear(&self) {
        let mut state = self.state.lock().unwrap();
        if let Some(cancel) = state.cancel.take() {
            cancel.cancel();
        }
        *state = State::default();
    }

    /// The state of the answer to `query` — anything else reads as idle, so a
    /// stale count is never shown against a query it does not belong to.
    pub fn status(&self, query: &FileSearchQuery) -> FileSearchStatus {
        let state = self.state.lock().unwrap();
        if let Some((q, matches)) = &state.ready {
            if q == query {
                return FileSearchStatus::Ready {
                    current: state.current.map_or(0, |c| c as u32 + 1),
                    total: matches.len() as u32,
                };
            }
        }
        if state.running.as_ref() == Some(query) {
            FileSearchStatus::Scanning
        } else {
            FileSearchStatus::Idle
        }
    }

    /// The matching line numbers, for a front end that lists them.
    pub fn matches(&self, query: &FileSearchQuery) -> Vec<i64> {
        let state = self.state.lock().unwrap();
        match &state.ready {
            Some((q, matches)) if q == query => matches.clone(),
            _ => Vec::new(),
        }
    }

    /// Steps to the next or previous match of `query`, wrapping at the ends,
    /// and answers with the line number to go to.
    ///
    /// `from` is the line the view is showing: the first step goes to the
    /// match nearest it rather than to the top of the file, because a reader
    /// watching the end of a log did not ask to be sent back a million lines.
    pub fn step(&self, query: &FileSearchQuery, forward: bool, from: Option<i64>) -> Option<i64> {
        let mut state = self.state.lock().unwrap();
        let current = state.current;
        let (q, matches) = state.ready.as_ref()?;
        if q != query || matches.is_empty() {
            return None;
        }
        let n = matches.len();
        let next = match current {
            Some(c) if forward => (c + 1) % n,
            Some(c) => (c + n - 1) % n,
            None => {
                let anchor = from.map_or(0, |line| matches.partition_point(|&m| m < line));
                if forward {
                    anchor % n
                } else {
                    (anchor + n - 1) % n
                }
            }
        };
        let line = matches[next];
        state.current = Some(next);
        Some(line)
    }
}

impl Drop for FileSearch {
    fn drop(&mut self) {
        // Trip the flag as well as closing the channel: a scan halfway through
        // a large file would not otherwise notice for some seconds.
        if let Some(cancel) = self.state.lock().unwrap().cancel.take() {
            cancel.cancel();
        }
        let _ = self.tx.send(Cmd::Stop);
    }
}

/// The scanning thread: wait out the typing, scan, report.
fn run(rx: Receiver<Cmd>, state: Arc<Mutex<State>>, listener: Arc<dyn FileSearchEvents>) {
    while let Ok(cmd) = rx.recv() {
        let (mut query, mut cancel) = match cmd {
            Cmd::Search(query, cancel) => (query, cancel),
            Cmd::Stop => return,
        };
        // Anything arriving inside the settle window replaces what we were
        // about to scan for, which is what makes typing cost one scan.
        loop {
            match rx.recv_timeout(SETTLE) {
                Ok(Cmd::Search(q, c)) => {
                    query = q;
                    cancel = c;
                }
                Ok(Cmd::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                Err(RecvTimeoutError::Timeout) => break,
            }
        }
        if cancel.is_cancelled() {
            continue;
        }
        let matcher = SearchMatcher::new(
            &query.text,
            query.case_sensitive,
            query.whole_word,
            query.is_regex,
        );
        // A cancelled scan answers with nothing, and nothing is what the
        // caller wants: it asked another question already.
        let Some(result) = search_file_cancellable(Path::new(&query.path), &matcher, &cancel)
        else {
            continue;
        };
        {
            let mut state = state.lock().unwrap();
            if cancel.is_cancelled() {
                continue;
            }
            state.running = None;
            state.cancel = None;
            state.current = None;
            state.ready = Some((query.clone(), result.match_line_numbers));
        }
        listener.on_result(query, result.total_matches);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::Sender as MpscSender;

    struct Notify(Mutex<MpscSender<FileSearchQuery>>);

    impl FileSearchEvents for Notify {
        fn on_result(&self, query: FileSearchQuery, _total: u32) {
            let _ = self.0.lock().unwrap().send(query);
        }
    }

    fn query(path: &str, text: &str) -> FileSearchQuery {
        FileSearchQuery {
            path: path.into(),
            text: text.into(),
            case_sensitive: false,
            whole_word: false,
            is_regex: false,
        }
    }

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ctail-filesearch-{}-{tag}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn scans_a_file_and_steps_through_its_matches() {
        let dir = temp_dir("steps");
        let path = dir.join("a.log");
        std::fs::write(&path, "one\nerror two\nthree\nerror four\n").unwrap();
        let q = query(&path.to_string_lossy(), "error");

        let (tx, rx) = channel();
        let search = FileSearch::new(Arc::new(Notify(Mutex::new(tx))));
        search.request(q.clone());
        assert_eq!(
            search.status(&q),
            FileSearchStatus::Scanning,
            "not before it settles"
        );

        assert_eq!(
            rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            q,
            "the scan reports the query it answered"
        );
        assert_eq!(
            search.status(&q),
            FileSearchStatus::Ready {
                current: 0,
                total: 2
            }
        );
        assert_eq!(search.matches(&q), [2, 4]);

        assert_eq!(search.step(&q, true, None), Some(2));
        assert_eq!(search.step(&q, true, None), Some(4));
        assert_eq!(search.step(&q, true, None), Some(2), "wraps at the end");
        assert_eq!(search.step(&q, false, None), Some(4), "and at the start");
        assert_eq!(
            search.status(&q),
            FileSearchStatus::Ready {
                current: 2,
                total: 2
            }
        );

        // The first step lands on the match nearest what is on screen.
        search.state.lock().unwrap().current = None;
        assert_eq!(search.step(&q, true, Some(3)), Some(4));
        search.state.lock().unwrap().current = None;
        assert_eq!(search.step(&q, false, Some(3)), Some(2));

        // An answer belongs to its query and to no other.
        let other = query(&path.to_string_lossy(), "three");
        assert_eq!(search.status(&other), FileSearchStatus::Idle);
        assert_eq!(search.step(&other, true, None), None);

        search.clear();
        assert_eq!(search.status(&q), FileSearchStatus::Idle);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn typing_costs_one_scan() {
        let dir = temp_dir("typing");
        let path = dir.join("b.log");
        std::fs::write(&path, "alpha\nbeta\nbetamax\n").unwrap();
        let (tx, rx) = channel();
        let search = FileSearch::new(Arc::new(Notify(Mutex::new(tx))));

        // "b", "be", "bet", "beta" in quick succession.
        for text in ["b", "be", "bet", "beta"] {
            search.request(query(&path.to_string_lossy(), text));
        }
        let last = query(&path.to_string_lossy(), "beta");
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), last);
        assert_eq!(
            rx.recv_timeout(Duration::from_millis(300)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout),
            "the queries typed through are never scanned for"
        );
        assert_eq!(search.matches(&last), [2, 3]);

        // Asking again for what is already in hand does not scan again.
        search.request(last.clone());
        assert_eq!(
            rx.recv_timeout(Duration::from_millis(400)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
