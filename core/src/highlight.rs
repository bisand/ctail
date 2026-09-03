//! Regex highlighting: compiles a profile's rules once, then classifies lines.
//!
//! Semantics follow the Go engine: the highest-priority matching line-level
//! rule styles the whole line; match-level spans are returned in ascending
//! priority order so a front end that paints them in sequence lets the higher
//! priority win where they overlap. Disabled and invalid rules are skipped.
//!
//! Patterns use `fancy-regex`: plain patterns run on the linear-time `regex`
//! engine, and only patterns with lookaround or backreferences fall back to
//! backtracking, so both RE2-style rules from the Go app and ICU-style rules
//! written against the earlier Swift engine keep working.

use crate::models::{Profile, Rule};
use crate::text::to_utf16_ranges;
use fancy_regex::Regex;

/// A matched substring, as UTF-16 code-unit offsets, tagged with the index of
/// the rule (into [`Highlighter::rules`]) that produced it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Span {
    pub start: u32,
    pub end: u32,
    pub rule: u32,
}

/// How to paint one line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct LineStyle {
    /// Index of the line-level rule that styles the whole line, or -1.
    pub line_rule: i32,
    /// Match-level spans in paint order (ascending priority).
    pub spans: Vec<Span>,
}

struct Compiled {
    rule: Rule,
    regex: Regex,
}

/// A compiled rule set.
pub struct Highlighter {
    /// Ascending priority (paint order for match-level rules).
    rules: Vec<Compiled>,
}

impl Highlighter {
    /// Compiles the enabled, valid rules of `rules`, ordered by ascending
    /// priority (ties keep their original order).
    pub fn new(rules: &[Rule]) -> Self {
        let mut compiled: Vec<Compiled> = rules
            .iter()
            .filter(|r| r.enabled)
            .filter_map(|r| {
                Regex::new(&r.pattern).ok().map(|regex| Compiled {
                    rule: r.clone(),
                    regex,
                })
            })
            .collect();
        compiled.sort_by_key(|c| c.rule.priority); // stable
        Self { rules: compiled }
    }

    pub fn from_profile(profile: &Profile) -> Self {
        Self::new(&profile.rules)
    }

    /// The compiled rules in index order, for precomputing colours/fonts.
    pub fn rules(&self) -> Vec<Rule> {
        self.rules.iter().map(|c| c.rule.clone()).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    pub fn apply(&self, line: &str) -> LineStyle {
        let mut style = LineStyle {
            line_rule: -1,
            spans: Vec::new(),
        };
        if self.rules.is_empty() {
            return style;
        }
        let mut byte_spans: Vec<(usize, usize, u32)> = Vec::new();
        for (i, c) in self.rules.iter().enumerate() {
            if c.rule.is_line_level() {
                // Ascending order: the last (highest-priority) match wins.
                if c.regex.is_match(line).unwrap_or(false) {
                    style.line_rule = i as i32;
                }
            } else {
                for m in c.regex.find_iter(line).flatten() {
                    if m.end() > m.start() {
                        byte_spans.push((m.start(), m.end(), i as u32));
                    }
                }
            }
        }
        if !byte_spans.is_empty() {
            let ranges = to_utf16_ranges(line, byte_spans.iter().map(|s| (s.0, s.1)));
            style.spans = ranges
                .into_iter()
                .zip(&byte_spans)
                .map(|((start, end), s)| Span {
                    start,
                    end,
                    rule: s.2,
                })
                .collect();
        }
        style
    }
}

/// Validates a pattern for the rules editor; `None` means it compiles.
pub fn validate_pattern(pattern: &str) -> Option<String> {
    Regex::new(pattern).err().map(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::default_profile;

    fn rule(id: &str, pattern: &str, match_type: &str, priority: i32) -> Rule {
        Rule {
            id: id.into(),
            pattern: pattern.into(),
            match_type: match_type.into(),
            priority,
            ..Default::default()
        }
    }

    #[test]
    fn common_logs_profile_semantics() {
        let h = Highlighter::from_profile(&default_profile());
        let ids: Vec<String> = h.rules().iter().map(|r| r.id.clone()).collect();
        assert_eq!(
            ids,
            ["timestamp", "debug", "info", "warn", "error", "fatal"],
            "ascending priority"
        );

        let s = h.apply("2026-09-03 12:00:00 ERROR something broke");
        assert_eq!(h.rules()[s.line_rule as usize].id, "error");
        assert_eq!(s.spans.len(), 1, "timestamp span");
        assert_eq!((s.spans[0].start, s.spans[0].end), (0, 19));

        // FATAL and ERROR on one line: highest priority (fatal) wins.
        let s = h.apply("ERROR then FATAL");
        assert_eq!(h.rules()[s.line_rule as usize].id, "fatal");

        let s = h.apply("plain info line");
        assert_eq!(s.line_rule, -1);
        assert_eq!(s.spans.len(), 1);
        assert_eq!(h.rules()[s.spans[0].rule as usize].id, "info");
    }

    #[test]
    fn skips_disabled_and_invalid_rules() {
        let mut bad = rule("bad", "(unclosed", "match", 1);
        let mut off = rule("off", "x", "match", 2);
        off.enabled = false;
        let ok = rule("ok", "x", "match", 3);
        bad.enabled = true;
        let h = Highlighter::new(&[bad, off, ok]);
        assert_eq!(h.rules().len(), 1);
        assert_eq!(h.apply("xx").spans.len(), 2);
    }

    #[test]
    fn spans_are_utf16_and_lookaround_works() {
        let h = Highlighter::new(&[rule("w", r"(?<=é)\d+", "match", 0)]);
        let s = h.apply("aé42 b");
        assert_eq!(
            s.spans,
            [Span {
                start: 2,
                end: 4,
                rule: 0
            }]
        );
        assert!(
            validate_pattern(r"(?<=x)y").is_none(),
            "lookbehind accepted"
        );
        assert!(validate_pattern("(").is_some());
    }

    #[test]
    fn paint_order_is_ascending_priority() {
        let h = Highlighter::new(&[rule("hi", "ab", "match", 10), rule("lo", "abc", "match", 1)]);
        let s = h.apply("abc");
        let order: Vec<&str> = s
            .spans
            .iter()
            .map(|sp| h.rules()[sp.rule as usize].id.clone())
            .map(|_| "")
            .collect();
        assert_eq!(order.len(), 2);
        assert_eq!(h.rules()[s.spans[0].rule as usize].id, "lo");
        assert_eq!(h.rules()[s.spans[1].rule as usize].id, "hi");
    }
}
