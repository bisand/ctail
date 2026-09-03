//! Search box semantics: case-sensitive, whole-word and regex toggles. Plain
//! queries are escaped into a regex so boolean matching and match ranges share
//! one code path.

use crate::text::to_utf16_ranges;
use fancy_regex::Regex;

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
}
