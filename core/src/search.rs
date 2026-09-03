//! Search box semantics: case-sensitive, whole-word and regex toggles. Plain
//! queries are escaped into a regex so boolean matching and match ranges share
//! one code path.

use crate::tailer::CancelToken;
use crate::text::to_utf16_ranges;
use fancy_regex::Regex;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// A matched range in UTF-16 code units.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct TextRange {
    pub start: u32,
    pub end: u32,
}

/// A compiled search query.
pub struct SearchMatcher {
    regex: Option<Regex>,
    empty: bool,
}

impl SearchMatcher {
    pub fn new(text: &str, case_sensitive: bool, whole_word: bool, is_regex: bool) -> Self {
        if text.is_empty() {
            return Self {
                regex: None,
                empty: true,
            };
        }
        let mut pattern = if is_regex {
            text.to_string()
        } else {
            fancy_regex::escape(text).into_owned()
        };
        if whole_word {
            pattern = format!(r"\b{pattern}\b");
        }
        if !case_sensitive {
            pattern = format!("(?i){pattern}");
        }
        Self {
            regex: Regex::new(&pattern).ok(),
            empty: false,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.empty
    }

    /// Empty queries are valid; a non-empty one is valid if it compiled.
    pub fn is_valid(&self) -> bool {
        self.empty || self.regex.is_some()
    }

    pub fn matches(&self, line: &str) -> bool {
        self.regex
            .as_ref()
            .is_some_and(|r| r.is_match(line).unwrap_or(false))
    }

    pub fn ranges(&self, line: &str) -> Vec<TextRange> {
        let Some(r) = &self.regex else {
            return Vec::new();
        };
        let byte_ranges: Vec<(usize, usize)> = r
            .find_iter(line)
            .flatten()
            .map(|m| (m.start(), m.end()))
            .collect();
        to_utf16_ranges(line, byte_ranges.into_iter())
            .into_iter()
            .map(|(start, end)| TextRange { start, end })
            .collect()
    }

    /// Indices of the lines that match (one call for a whole window).
    pub fn matching_indices<S: AsRef<str>>(&self, lines: &[S]) -> Vec<u32> {
        lines
            .iter()
            .enumerate()
            .filter(|(_, l)| self.matches(l.as_ref()))
            .map(|(i, _)| i as u32)
            .collect()
    }
}

/// Result of a whole-file search.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct SearchResult {
    pub match_line_numbers: Vec<i64>,
    pub total_matches: u32,
    pub total_lines: i64,
}

/// Scans an entire file line by line (streaming, bounded memory) and returns
/// the 1-based numbers of the lines that match. An empty or invalid query
/// matches nothing.
pub fn search_file(path: &Path, matcher: &SearchMatcher) -> SearchResult {
    search_file_cancellable(path, matcher, &CancelToken::default()).unwrap_or_default()
}

/// How often the scan looks at the cancel flag. Often enough that a keystroke
/// does not wait on it, rarely enough that a scan of ten million lines does
/// not spend its time on atomics.
const CANCEL_EVERY: i64 = 4096;

/// [`search_file`], abandoning the scan when `cancel` is tripped.
///
/// `None` means it stopped early: a half-scanned file gives a match count that
/// is not merely imprecise but wrong, and the caller who asked it to stop has
/// already moved on to another query.
pub fn search_file_cancellable(
    path: &Path,
    matcher: &SearchMatcher,
    cancel: &CancelToken,
) -> Option<SearchResult> {
    let mut result = SearchResult::default();
    if matcher.is_empty() || !matcher.is_valid() {
        return Some(result);
    }
    let Ok(file) = std::fs::File::open(path) else {
        return Some(result);
    };
    let mut reader = BufReader::with_capacity(1 << 20, file);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        if buf.last() != Some(&b'\n') {
            break; // trailing partial line is not a line yet
        }
        result.total_lines += 1;
        if result.total_lines % CANCEL_EVERY == 0 && cancel.is_cancelled() {
            return None;
        }
        let mut end = buf.len() - 1;
        if end > 0 && buf[end - 1] == b'\r' {
            end -= 1;
        }
        if matcher.matches(&String::from_utf8_lossy(&buf[..end])) {
            result.match_line_numbers.push(result.total_lines);
        }
    }
    result.total_matches = result.match_line_numbers.len() as u32;
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_case_word_regex() {
        let q = SearchMatcher::new("error", false, false, false);
        assert!(q.matches("an ERROR happened"));
        assert!(!q.matches("all good"));
        let q = SearchMatcher::new("Error", true, false, false);
        assert!(!q.matches("an error happened"));
        assert!(q.matches("an Error happened"));
        let q = SearchMatcher::new("err", false, true, false);
        assert!(!q.matches("error"));
        assert!(q.matches("an err here"));
        let q = SearchMatcher::new(r"\d{3}", false, false, true);
        assert!(q.matches("code 404 returned"));
        assert_eq!(
            q.ranges("a 404 b 500"),
            [
                TextRange { start: 2, end: 5 },
                TextRange { start: 8, end: 11 }
            ]
        );
        let q = SearchMatcher::new("a.b", false, false, false);
        assert!(q.matches("a.b"));
        assert!(!q.matches("axb"));
    }

    #[test]
    fn validity_and_batch() {
        assert!(!SearchMatcher::new("(unclosed", false, false, true).is_valid());
        let q = SearchMatcher::new("", false, false, true);
        assert!(q.is_valid() && q.is_empty());
        assert!(!q.matches("anything"));
        let q = SearchMatcher::new("x", false, false, false);
        assert_eq!(q.matching_indices(&["a", "x", "b", "xx"]), [1, 3]);
        assert_eq!(
            SearchMatcher::new("é", false, false, false).ranges("aé"),
            [TextRange { start: 1, end: 2 }]
        );
    }

    #[test]
    fn whole_file_search() {
        let path = std::env::temp_dir().join(format!("ctail-search-{}.log", std::process::id()));
        std::fs::write(&path, "alpha\nERROR one\r\nbeta\nerror two\npartial ERROR").unwrap();
        let r = search_file(&path, &SearchMatcher::new("error", false, false, false));
        assert_eq!(r.match_line_numbers, [2, 4]);
        assert_eq!(r.total_matches, 2);
        assert_eq!(r.total_lines, 4, "trailing partial not counted");
        assert_eq!(
            search_file(&path, &SearchMatcher::new("", false, false, false)),
            SearchResult::default()
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_cancelled_scan_answers_with_nothing() {
        let dir = std::env::temp_dir().join(format!("ctail-search-cancel-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("big.log");
        // More lines than the cancel check's stride, so the flag is looked at.
        let body: String = (0..CANCEL_EVERY * 2 + 7)
            .map(|i| format!("line {i} error\n"))
            .collect();
        std::fs::write(&path, body).unwrap();
        let q = SearchMatcher::new("error", false, false, false);

        let live = CancelToken::default();
        let full = search_file_cancellable(&path, &q, &live).expect("not cancelled");
        assert_eq!(full.total_lines, CANCEL_EVERY * 2 + 7);
        assert_eq!(full.total_matches as i64, CANCEL_EVERY * 2 + 7);

        let stopped = CancelToken::default();
        stopped.cancel();
        assert_eq!(search_file_cancellable(&path, &q, &stopped), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
