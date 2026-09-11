//! The log surface: a virtualized, highlighted, searchable view over a window
//! of lines. Only the rows inside the widget's bounds are ever laid out or
//! drawn; the window itself is a bounded slice of the file that the app keeps
//! fed from the engine (live lines at the bottom, scrollback at the top).

use ctail_core::{Highlighter, LogLine, Rule, SearchMatcher};
use denise::{
    Color, ElementState, InputEvent, KeyCode, Modifiers, Pen, Point, PointerButton, Rect, Role,
};
use denise_text::{TextEngine, TextStyle};
use denise_ui::widget::{Event, EventCtx, Handled, PaintCtx, Widget};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// What the backend calls a line when the platform reports scrolling in
/// notches rather than pixels (`denise_winit::LINE_HEIGHT_PX`). A wheel's
/// deltas are whole numbers of these; a trackpad reports true pixels.
const WHEEL_LINE_PX: f32 = 16.0;

/// How long a gesture is taken to still be under way after the last delta only
/// a gesture could have produced.
const GESTURE_MEMORY: Duration = Duration::from_millis(500);

/// Whether a scroll delta is one a wheel could have produced.
///
/// Not enough on its own to call it one: a trackpad reports pixels, and a
/// tenth of them land on an exact multiple of sixteen by chance — which, taken
/// for a notch, threw the view five rows down the file in the middle of a
/// smooth drag. So the test that matters is the negative one. A delta that is
/// *not* a whole number of lines proves fingers are on the glass, and
/// [`LogView::is_notch`] remembers that for as long as a gesture plausibly
/// lasts.
fn could_be_notch(delta: f32) -> bool {
    delta != 0.0 && (delta % WHEEL_LINE_PX).abs() < f32::EPSILON
}

/// Whether an offset into the top row would take the view past an end of the
/// file, where it would show a strip of nothing.
///
/// `rows` is the whole-row part of the movement that has just been made, and
/// it is what tells the two cases at the top of a file apart: a movement that
/// asked to cross the top (`rows < 0`) is cut back to the first line, while
/// standing on the first line and moving *down* (`rows == 0`) is a request to
/// hide the top of that line — which is how a view leaves the top three pixels
/// at a time.
fn past_end(at: (usize, usize), max: (usize, usize), rows: i64) -> bool {
    (at == (0, 0) && rows < 0) || at >= max
}

/// Splits a pixel movement into the whole rows it crosses and the offset into
/// the row it lands in. `sub_px` is the offset it starts from.
///
/// Euclidean rather than truncating division: Rust rounds a negative quotient
/// towards zero, and moving up by less than a row has to land on the row above
/// with a large offset, not on the same row with a negative one.
fn split_pixels(sub_px: i32, pixels: i32, row_h: i32) -> (i64, i32) {
    let row_h = row_h.max(1);
    let total = sub_px + pixels;
    (total.div_euclid(row_h) as i64, total.rem_euclid(row_h))
}

/// What the view asks the app for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogRequest {
    /// The user scrolled to the top of the window: fetch older lines.
    Older,
    /// Ctrl/Cmd+C with a selection.
    Copy,
    /// Follow mode changed (scrolling up pauses it, End resumes it).
    Follow(bool),
    /// Following was asked for while the window sat somewhere else in the
    /// file: the tail has to be read again before it can be followed.
    Reattach,
}

/// Where the search is, for the bar's counter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SearchStatus {
    /// 1-based position of the current match, or 0 when there is none.
    pub current: usize,
    pub total: usize,
}

/// A line's highlighting, worked out once and kept.
///
/// Both halves of it are dear: `Highlighter::apply` runs every rule's regex
/// over the line, and `split_runs` then walks it character by character. Doing
/// that for seventy visible lines is a third of a frame, and the answer cannot
/// change between one frame and the next — a line's text never changes once it
/// has arrived.
struct Styled {
    /// Index of the line-level rule that styles the whole line, or -1.
    line_rule: i32,
    /// Byte ranges of the line and the rule that paints each, in order.
    runs: Vec<(usize, usize, Option<u32>)>,
}

struct RuleStyle {
    fg: Option<Color>,
    bg: Option<Color>,
}

pub struct LogView<M> {
    to_message: fn(LogRequest) -> M,
    style: TextStyle,
    lines: VecDeque<LogLine>,
    /// Lines carry engine-local numbers until the head count lands.
    provisional: bool,
    /// Cap on the window while following (older lines are dropped).
    cap: usize,
    /// Index of the first visible row within the displayed sequence.
    top: usize,
    /// Which wrapped segment of that row the view starts at. Always 0 without
    /// word wrap, which is what makes the two modes share every other index.
    top_seg: usize,
    /// Pixels of the top row hidden above the viewport, in `0..row height`.
    /// This is what makes a trackpad feel like a trackpad: a gesture that has
    /// not yet covered a whole row still moves the log by what it covered,
    /// rather than being rounded away and then arriving all at once.
    sub_px: i32,
    /// Pixel motion a wheel or gesture reported that is not yet a whole pixel.
    /// Kept so a slow drag accumulates instead of being truncated to nothing.
    scroll_residue: f32,
    /// When a delta last arrived that no wheel could have sent, which is what
    /// tells a trackpad from a mouse. See [`could_be_notch`].
    gesture_at: Option<Instant>,
    /// Long lines are broken to fit the width instead of running off it.
    wrap: bool,
    follow: bool,
    total_lines: i64,
    selection: Option<(usize, usize)>, // (anchor, cursor) as displayed rows
    dragging: bool,
    highlighter: Arc<Highlighter>,
    styles: Vec<RuleStyle>,
    /// Rows that fit, discovered while painting (the one place that knows the
    /// row height) and read back by scrolling.
    visible: Cell<usize>,
    /// Width available to text, and the rows painted last frame as
    /// (row, y within the widget, height) — both learnt while painting, and
    /// read back by scrolling and by hit-testing, which have no other way to
    /// know how tall a wrapped line turned out.
    wrap_width: Cell<i32>,
    painted: RefCell<Vec<(usize, i32, i32)>>,
    /// Rows actually drawn by the last paint, as against merely placed on
    /// `painted`: the measure of what a scroll saved. For the trace.
    drawn: Cell<usize>,
    /// Pixels the text is scrolled sideways. The gutter stays put; only the
    /// text moves. Always 0 with word wrap, which has no sideways to go.
    scroll_x: i32,
    /// Bytes of the longest resident line — in a monospaced face, an upper
    /// bound on its width, and what sideways scrolling is clamped to. A running
    /// maximum as lines arrive, recounted when lines are dropped.
    widest: usize,
    /// The text area's width and one glyph's advance, learnt while painting,
    /// so a scroll can clamp without measuring anything itself.
    text_area: Cell<(i32, i32)>,
    /// Where "scrolled all the way down" is, as (row, segment).
    bottom: Cell<(usize, usize)>,
    /// Character advances, memoised: wrapping walks a line character by
    /// character, and a log is written in the same few dozen of them.
    advances: RefCell<HashMap<char, i32>>,
    /// Highlighting per line number, which is the one key that survives lines
    /// arriving at either end of the window. Cleared whenever the rules or the
    /// numbering change, and capped so a long session cannot grow it forever.
    styled: RefCell<HashMap<i64, Rc<Styled>>>,
    show_numbers: bool,
    waiting_older: bool,
    /// The window is a range from the middle of the file rather than the tail,
    /// so live lines do not belong on the end of it.
    detached: bool,

    // --- search ---
    matcher: Option<Arc<SearchMatcher>>,
    /// Filter mode: only matching lines are displayed. Only ever on with a
    /// usable query, so clearing the field always brings every line back.
    filter: bool,
    /// In filter mode, the indices into `lines` that are displayed.
    filtered: Vec<usize>,
    /// Displayed rows that match, in order.
    matches: Vec<usize>,
    /// Index into `matches`.
    current: Option<usize>,
}

impl<M: 'static> LogView<M> {
    pub fn new(
        to_message: fn(LogRequest) -> M,
        style: TextStyle,
        rules: &[Rule],
        cap: usize,
    ) -> Self {
        let mut v = Self {
            to_message,
            style,
            lines: VecDeque::new(),
            provisional: false,
            cap: cap.max(200),
            top: 0,
            top_seg: 0,
            sub_px: 0,
            scroll_residue: 0.0,
            gesture_at: None,
            wrap: false,
            follow: true,
            total_lines: 0,
            selection: None,
            dragging: false,
            highlighter: Arc::new(Highlighter::new(&[])),
            styles: Vec::new(),
            visible: Cell::new(0),
            wrap_width: Cell::new(0),
            drawn: Cell::new(0),
            painted: RefCell::new(Vec::new()),
            scroll_x: 0,
            widest: 0,
            text_area: Cell::new((0, 1)),
            bottom: Cell::new((0, 0)),
            advances: RefCell::new(HashMap::new()),
            styled: RefCell::new(HashMap::new()),
            show_numbers: true,
            waiting_older: false,
            detached: false,
            matcher: None,
            filter: false,
            filtered: Vec::new(),
            matches: Vec::new(),
            current: None,
        };
        v.set_rules(rules);
        v
    }

    pub fn set_rules(&mut self, rules: &[Rule]) {
        self.highlighter = Arc::new(Highlighter::new(rules));
        self.styled.borrow_mut().clear();
        self.styles = self
            .highlighter
            .rules()
            .iter()
            .map(|r| RuleStyle {
                fg: (!r.foreground.is_empty()).then(|| crate::theme::hex(&r.foreground)),
                bg: (!r.background.is_empty()).then(|| crate::theme::hex(&r.background)),
            })
            .collect();
    }

    /// Font and size for the log rows.
    pub fn set_style(&mut self, style: TextStyle) {
        self.style = style;
        self.advances.borrow_mut().clear();
    }

    /// Whether long lines are broken to fit the width.
    pub fn set_word_wrap(&mut self, wrap: bool) {
        if wrap {
            self.scroll_x = 0;
        }
        if wrap == self.wrap {
            return;
        }
        self.wrap = wrap;
        // A segment offset means nothing in the other mode, and the line the
        // reader was on is the thing worth keeping.
        self.top_seg = 0;
    }

    /// How many lines the window keeps while following.
    pub fn set_cap(&mut self, cap: usize) {
        self.cap = cap.max(200);
    }

    /// Whether the gutter is drawn.
    pub fn set_show_line_numbers(&mut self, show: bool) {
        self.show_numbers = show;
    }

    pub fn following(&self) -> bool {
        self.follow
    }

    pub fn set_follow(&mut self, follow: bool) {
        self.follow = follow;
        if follow {
            self.scroll_to_bottom();
        }
    }

    /// Whether the window is a jumped-to range rather than the tail.
    pub fn is_detached(&self) -> bool {
        self.detached
    }

    pub fn total_lines(&self) -> i64 {
        self.total_lines
    }

    /// Bytes this view holds for its file: the lines in the window, their
    /// highlighting, and the rows a search or filter picked out. Summed when
    /// asked — every two seconds, over a window of ten thousand lines by
    /// default — rather than kept current everywhere lines come and go.
    pub fn memory_bytes(&self) -> usize {
        use std::mem::size_of;
        let lines = self.lines.capacity() * size_of::<LogLine>()
            + self.lines.iter().map(|l| l.text.capacity()).sum::<usize>();
        let styled = self.styled.borrow();
        let highlighting = styled.capacity() * (size_of::<i64>() + size_of::<Rc<Styled>>())
            + styled
                .values()
                .map(|s| {
                    // An `Rc`'s allocation carries its two counts beside the value.
                    2 * size_of::<usize>()
                        + size_of::<Styled>()
                        + s.runs.capacity() * size_of::<(usize, usize, Option<u32>)>()
                })
                .sum::<usize>();
        let search = (self.filtered.capacity() + self.matches.capacity()) * size_of::<usize>();
        lines + highlighting + search
    }

    pub fn first_number(&self) -> Option<i64> {
        self.lines.front().map(|l| l.number)
    }

    /// The number of the line at the top of the view, which is where a search
    /// step measures "nearest" from.
    pub fn first_visible_number(&self) -> Option<i64> {
        self.line_at(self.effective_top()).map(|l| l.number)
    }

    pub fn reset(&mut self) {
        self.lines.clear();
        self.styled.borrow_mut().clear();
        self.top = 0;
        self.top_seg = 0;
        self.sub_px = 0;
        self.scroll_x = 0;
        self.widest = 0;
        self.detached = false;
        self.selection = None;
        self.provisional = false;
        self.total_lines = 0;
        self.waiting_older = false;
        self.recompute_search();
    }

    /// Live lines from the engine (numbered locally until `apply_base`).
    pub fn append(&mut self, new: Vec<LogLine>, provisional: bool) {
        // A window that has jumped elsewhere in the file has no end for these
        // to be appended to: their numbers would follow on from a line that is
        // nowhere near them.
        if new.is_empty() || self.detached {
            return;
        }
        self.provisional = provisional;
        self.note_widest(&new);
        self.lines.extend(new);
        self.total_lines = self
            .lines
            .back()
            .map_or(0, |l| l.number)
            .max(self.total_lines);
        if self.follow {
            let over = self.lines.len().saturating_sub(self.cap);
            if over > 0 {
                self.lines.drain(..over);
                self.shift_indices(over);
                self.recount_widest();
            }
        } else if self.lines.len() > self.cap * 3 {
            let over = self.lines.len() - self.cap * 3;
            self.lines.drain(..over);
            self.shift_indices(over);
            self.recount_widest();
        }
        self.recompute_search();
        if self.follow {
            self.scroll_to_bottom();
        }
    }

    /// Replaces the window with a range from somewhere else in the file and
    /// puts `target` in the middle of the view. The window stays detached —
    /// the tail is elsewhere — until following is asked for again.
    pub fn show_range(&mut self, lines: Vec<LogLine>, target: i64) {
        if lines.is_empty() {
            return;
        }
        self.lines.clear();
        self.lines.extend(lines);
        self.recount_widest();
        self.provisional = false;
        self.detached = true;
        self.follow = false;
        self.selection = None;
        self.waiting_older = false;
        self.top = 0;
        self.top_seg = 0;
        self.recompute_search();
        self.reveal_number(target);
    }

    /// Scrolls the line numbered `number` into the middle of the view and
    /// makes it the current match if it is one. False when that line is not in
    /// the window, which is the caller's cue to go and fetch it.
    pub fn reveal_number(&mut self, number: i64) -> bool {
        let Some(row) = self.row_of_number(number) else {
            return false;
        };
        let rows = self.visible_rows();
        self.follow = false;
        self.top = row.saturating_sub(rows / 2).min(self.max_top());
        self.top_seg = 0;
        self.current = self.matches.iter().position(|&m| m == row);
        true
    }

    /// The displayed row holding a given line number.
    fn row_of_number(&self, number: i64) -> Option<usize> {
        if self.filter {
            self.filtered
                .iter()
                .position(|&i| self.lines.get(i).is_some_and(|l| l.number == number))
        } else {
            // Numbers ascend through the window, so this is a lookup rather
            // than a scan of everything the buffer holds.
            self.lines.binary_search_by_key(&number, |l| l.number).ok()
        }
    }

    /// Older lines fetched for scrollback; they go in front.
    pub fn prepend(&mut self, older: Vec<LogLine>) {
        self.waiting_older = false;
        if older.is_empty() {
            return;
        }
        let n = older.len();
        self.note_widest(&older);
        for line in older.into_iter().rev() {
            self.lines.push_front(line);
        }
        if self.filter {
            // Displayed rows are filtered positions, so they cannot be shifted
            // by a line count; the recompute below restores them.
            self.recompute_search();
        } else {
            self.top += n;
            if let Some((a, c)) = self.selection {
                self.selection = Some((a + n, c + n));
            }
            self.recompute_search();
        }
    }

    /// The head count landed: local numbers become absolute.
    pub fn apply_base(&mut self, base: i64, total: i64) {
        if self.provisional {
            for l in &mut self.lines {
                l.number += base;
            }
            self.provisional = false;
            // Every line is keyed by its number, and every number just moved.
            self.styled.borrow_mut().clear();
        }
        self.total_lines = total;
    }

    /// Selects every displayed row — in filter mode, every matching one.
    pub fn select_all(&mut self) {
        let count = self.row_count();
        self.selection = (count > 0).then(|| (0, count - 1));
    }

    pub fn selected_text(&self) -> Option<String> {
        let (a, c) = self.selection?;
        let (lo, hi) = (a.min(c), a.max(c));
        let text: Vec<&str> = (lo..=hi)
            .filter_map(|row| self.line_at(row))
            .map(|l| l.text.as_str())
            .collect();
        (!text.is_empty()).then(|| text.join("\n"))
    }

    /// What the assistant is shown: the selection if there is one, otherwise
    /// the last `n` lines — the macOS app's choice too.
    pub fn context_text(&self, n: usize) -> String {
        if let Some(selected) = self.selected_text() {
            return selected;
        }
        let skip = self.lines.len().saturating_sub(n);
        self.lines
            .iter()
            .skip(skip)
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    // --- search ---------------------------------------------------------

    /// Applies a query. `None` clears it; `filter` hides non-matching lines,
    /// and is ignored for an empty or invalid query so the view can never end
    /// up blank with no way back.
    pub fn set_search(&mut self, matcher: Option<Arc<SearchMatcher>>, filter: bool) {
        let usable = matcher
            .as_ref()
            .is_some_and(|m| !m.is_empty() && m.is_valid());
        self.matcher = matcher.filter(|_| usable);
        let was_filtering = self.filter;
        self.filter = filter && usable;
        if self.filter != was_filtering {
            // Row indices mean something different on each side of this.
            self.selection = None;
            self.top = 0;
            self.top_seg = 0;
            self.follow = false;
        }
        // Deliberately no scroll: a query is typed a character at a time, and
        // jumping to the oldest match on the first keystroke throws the reader
        // off the lines they were watching. Enter and ↓ are what move.
        self.recompute_search();
    }

    /// Where the search stands now — the match list moves on its own as lines
    /// arrive, so the bar has to ask rather than be told once.
    pub fn search_status(&self) -> SearchStatus {
        self.status()
    }

    pub fn next_match(&mut self) {
        self.step(1)
    }

    pub fn prev_match(&mut self) {
        self.step(-1)
    }

    /// The counter reads "where you are": until a match has been stepped to,
    /// that is the first one at or after the top of the view, so it answers
    /// the question the reader actually has rather than counting from a line
    /// that scrolled out of the window long ago.
    fn status(&self) -> SearchStatus {
        if self.matches.is_empty() {
            return SearchStatus::default();
        }
        SearchStatus {
            current: self.current.unwrap_or_else(|| self.anchor()) + 1,
            total: self.matches.len(),
        }
    }

    fn step(&mut self, dir: isize) {
        if self.matches.is_empty() {
            self.current = None;
            return;
        }
        let n = self.matches.len() as isize;
        let next = match self.current {
            Some(c) => (c as isize + dir).rem_euclid(n),
            // The first step goes to a match near what is on screen, not to
            // the oldest one in the buffer.
            None => {
                let anchor = self.anchor() as isize;
                if dir > 0 {
                    anchor
                } else {
                    (anchor - 1).rem_euclid(n)
                }
            }
        };
        self.current = Some(next as usize);
        self.reveal_current();
    }

    /// Rebuilds the filtered set and the match list, keeping the current match
    /// on the same *line* where that line is still there.
    fn recompute_search(&mut self) {
        let keep = self
            .current
            .and_then(|c| self.matches.get(c).copied())
            .and_then(|row| self.line_at(row))
            .map(|l| l.number);
        self.filtered.clear();
        self.matches.clear();
        if let Some(m) = self.matcher.clone() {
            if self.filter {
                for (i, line) in self.lines.iter().enumerate() {
                    if m.matches(&line.text) {
                        self.filtered.push(i);
                    }
                }
                self.matches = (0..self.filtered.len()).collect();
            } else {
                for (i, line) in self.lines.iter().enumerate() {
                    if m.matches(&line.text) {
                        self.matches.push(i);
                    }
                }
            }
        }
        // A match that was stepped to stays current while its line is still
        // here. Otherwise there is no current match until the reader asks for
        // one, and `anchor` answers from the viewport when they do.
        self.current = keep.and_then(|number| {
            self.matches
                .iter()
                .position(|&row| self.line_at(row).is_some_and(|l| l.number == number))
        });
        if self.top > self.max_top() {
            self.top = self.max_top();
            self.top_seg = self.max_top_seg();
        }
    }

    /// Index into `matches` of the first match at or after the top of the
    /// view, wrapping to the first when every match is above it. Read lazily,
    /// because before the first paint the view does not yet know its height.
    fn anchor(&self) -> usize {
        let top = self.effective_top();
        self.matches.iter().position(|&m| m >= top).unwrap_or(0)
    }

    /// The row actually at the top: while following, paint pins the window to
    /// its end and `top` is not authoritative.
    fn effective_top(&self) -> usize {
        if self.follow {
            self.max_top()
        } else {
            self.top
        }
    }

    /// Scrolls the current match into view, roughly centred, and stops
    /// following — stepping through matches is looking at one place.
    fn reveal_current(&mut self) {
        let Some(row) = self.current.and_then(|c| self.matches.get(c).copied()) else {
            return;
        };
        let rows = self.visible_rows();
        if self.follow || row < self.top || row >= self.top + rows {
            self.top = row.saturating_sub(rows / 2).min(self.max_top());
            self.top_seg = 0;
            self.follow = false;
        }
    }

    // --- the displayed sequence (all rows, or only matching ones) ---------

    fn row_count(&self) -> usize {
        if self.filter {
            self.filtered.len()
        } else {
            self.lines.len()
        }
    }

    fn line_at(&self, row: usize) -> Option<&LogLine> {
        if self.filter {
            self.lines.get(*self.filtered.get(row)?)
        } else {
            self.lines.get(row)
        }
    }

    // --- wrapping ---------------------------------------------------------

    /// Advance of one character, measured once and remembered.
    fn advance(&self, text: &mut TextEngine, ch: char) -> i32 {
        if let Some(&w) = self.advances.borrow().get(&ch) {
            return w;
        }
        let mut buf = [0u8; 4];
        let w = text.measure_line(self.style, ch.encode_utf8(&mut buf));
        self.advances.borrow_mut().insert(ch, w);
        w
    }

    /// The byte ranges `line` breaks into to fit `width`.
    ///
    /// Greedy, breaking after the last space that fits, and mid-character when
    /// no space fits — a log line is as likely to be one unbroken token as a
    /// sentence, and a stack frame that runs off the edge is exactly what word
    /// wrap was turned on to see. Always at least one range, so an empty line
    /// still occupies a row.
    fn segments(&self, text: &mut TextEngine, line: &str, width: i32) -> Vec<(usize, usize)> {
        if !self.wrap || width <= 0 || line.is_empty() {
            return vec![(0, line.len())];
        }
        let mut out = Vec::new();
        let mut start = 0;
        let mut x = 0; // width of line[start..i]
        let mut since = 0; // width since the last space
        let mut last_space: Option<usize> = None;
        for (i, ch) in line.char_indices() {
            let w = self.advance(text, ch);
            if x + w > width && i > start {
                match last_space.filter(|&b| b > start) {
                    Some(b) => {
                        out.push((start, b));
                        start = b;
                        x = since;
                    }
                    None => {
                        out.push((start, i));
                        start = i;
                        x = 0;
                    }
                }
                last_space = None;
                since = 0;
            }
            x += w;
            since += w;
            if ch == ' ' || ch == '\t' {
                last_space = Some(i + ch.len_utf8());
                since = 0;
            }
        }
        out.push((start, line.len()));
        out
    }

    /// How many rows the displayed row `row` occupies.
    fn seg_count(&self, text: &mut TextEngine, row: usize, width: i32) -> usize {
        if !self.wrap {
            return 1;
        }
        match self.line_at(row) {
            Some(line) => self.segments(text, &line.text, width).len().max(1),
            None => 1,
        }
    }

    /// The (row, segment) that puts the end of the file at the bottom of a view
    /// `rows` rows tall.
    fn bottom_anchor(&self, text: &mut TextEngine, width: i32, rows: usize) -> (usize, usize) {
        let count = self.row_count();
        if !self.wrap || count == 0 {
            return (count.saturating_sub(rows), 0);
        }
        let mut left = rows.max(1);
        let mut row = count - 1;
        loop {
            let segs = self.seg_count(text, row, width);
            if segs >= left {
                return (row, segs - left);
            }
            left -= segs;
            if row == 0 {
                return (0, 0);
            }
            row -= 1;
        }
    }

    /// Moves the viewport `delta` visual rows, stopping at either end.
    fn walk_rows(&self, text: &mut TextEngine, delta: i64) -> (usize, usize) {
        if !self.wrap {
            let top = (self.top as i64 + delta).clamp(0, self.max_top() as i64) as usize;
            return (top, 0);
        }
        let width = self.wrap_width.get();
        let (mut row, mut seg) = (self.top, self.top_seg);
        let count = self.row_count();
        let mut left = delta;
        while left > 0 {
            let segs = self.seg_count(text, row, width);
            if seg + 1 < segs {
                seg += 1;
            } else if row + 1 < count {
                row += 1;
                seg = 0;
            } else {
                break;
            }
            left -= 1;
        }
        while left < 0 {
            if seg > 0 {
                seg -= 1;
            } else if row > 0 {
                row -= 1;
                seg = self.seg_count(text, row, width) - 1;
            } else {
                break;
            }
            left += 1;
        }
        let (max_row, max_seg) = (self.max_top(), self.max_top_seg());
        if (row, seg) > (max_row, max_seg) {
            (max_row, max_seg)
        } else {
            (row, seg)
        }
    }

    fn shift_indices(&mut self, by: usize) {
        if by > self.top {
            self.top_seg = 0;
        }
        self.top = self.top.saturating_sub(by);
        self.selection = match self.selection {
            Some((a, c)) if a >= by && c >= by => Some((a - by, c - by)),
            _ => None,
        };
    }

    /// This line's highlighting, from the cache or worked out and kept.
    fn styled(&self, line: &LogLine) -> Rc<Styled> {
        if let Some(hit) = self.styled.borrow().get(&line.number) {
            return hit.clone();
        }
        let style = self.highlighter.apply(&line.text);
        let entry = Rc::new(Styled {
            line_rule: style.line_rule,
            runs: split_runs(&line.text, &style.spans),
        });
        let mut cache = self.styled.borrow_mut();
        // Twice the window is room for every line it can hold and the ones it
        // has just scrolled past; beyond that, start again rather than track
        // ages for entries that cost thirty microseconds to rebuild.
        if cache.len() > self.cap * 2 {
            cache.clear();
        }
        cache.insert(line.number, entry.clone());
        entry
    }

    fn note_widest(&mut self, lines: &[LogLine]) {
        for line in lines {
            self.widest = self.widest.max(line.text.len());
        }
    }

    fn recount_widest(&mut self) {
        self.widest = self.lines.iter().map(|l| l.text.len()).max().unwrap_or(0);
    }

    /// Bytes of the longest resident line.
    #[cfg(test)]
    pub fn widest(&self) -> usize {
        self.widest
    }

    /// How far the text can go sideways: the widest line's width less the
    /// area it is shown in, and never past the last glyph. From what paint
    /// measured last; 0 before the first paint, when there is nothing to
    /// scroll anyway.
    fn max_scroll_x(&self) -> i32 {
        let (width, zero_w) = self.text_area.get();
        (self.widest as i32 * zero_w + zero_w - width).max(0)
    }

    /// Scrolls the text sideways to `px` from its left edge, within what the
    /// widest line allows.
    pub fn set_scroll_x(&mut self, px: i32) {
        self.scroll_x = if self.wrap {
            0
        } else {
            px.clamp(0, self.max_scroll_x())
        };
    }

    /// Where the viewport is, as (top row, segment, pixels of it hidden,
    /// following), for the scroll trace.
    pub fn trace_position(&self) -> (usize, usize, i32, bool, usize) {
        (
            self.top,
            self.top_seg,
            self.sub_px,
            self.follow,
            self.drawn.get(),
        )
    }

    /// Visual rows from one viewport position to another: positive when `to`
    /// is further down the file. Without wrapping that is arithmetic; with it
    /// the lines in between are measured, and a jump too long to be worth
    /// measuring — more than two screens — answers `None`, which is also
    /// too far to move pixels for.
    fn rows_between(
        &self,
        text: &mut TextEngine,
        from: (usize, usize),
        to: (usize, usize),
    ) -> Option<i64> {
        if !self.wrap {
            return Some(to.0 as i64 - from.0 as i64);
        }
        let (a, b, sign) = if from <= to {
            (from, to, 1)
        } else {
            (to, from, -1)
        };
        if b.0 - a.0 > self.visible_rows() * 2 {
            return None;
        }
        let width = self.wrap_width.get();
        let rows = if a.0 == b.0 {
            b.1 as i64 - a.1 as i64
        } else {
            let mut rows = self.seg_count(text, a.0, width) as i64 - a.1 as i64;
            for row in a.0 + 1..b.0 {
                rows += self.seg_count(text, row, width) as i64;
            }
            rows + b.1 as i64
        };
        Some(rows * sign)
    }

    /// The part of the widget a vertical scroll moves: everything but the
    /// sideways scrollbar along the bottom, when there is one, which stays
    /// where it is and is repainted.
    fn moving_area(&self, bounds: Rect) -> Rect {
        if !self.wrap && self.max_scroll_x() > 0 {
            Rect::new(bounds.x, bounds.y, bounds.width, bounds.height - 6)
        } else {
            bounds
        }
    }

    pub fn visible_rows(&self) -> usize {
        self.visible.get().max(1)
    }

    fn scroll_to_bottom(&mut self) {
        self.top = self.max_top();
        self.top_seg = self.max_top_seg();
        self.sub_px = 0;
    }

    /// The topmost row of the bottom-most view. Without wrapping that is
    /// arithmetic; with it, only paint knows how tall the last lines are, so
    /// the answer comes from there.
    fn max_top(&self) -> usize {
        if self.wrap {
            self.bottom.get().0.min(self.row_count().saturating_sub(1))
        } else {
            self.row_count().saturating_sub(self.visible_rows())
        }
    }

    fn max_top_seg(&self) -> usize {
        if self.wrap && self.bottom.get().0 < self.row_count() {
            self.bottom.get().1
        } else {
            0
        }
    }

    /// Moves the viewport by pixels: the whole rows it covers, and whatever is
    /// left over as an offset into the top one. Every displayed row is exactly
    /// one row high — a wrapped line is several of them — so the leftover is
    /// the same measurement whichever mode the view is in.
    fn scroll_pixels(&mut self, pixels: i32, row_h: i32, ctx: &mut EventCtx<'_, M>) {
        let (rows, offset) = split_pixels(self.sub_px, pixels, row_h);
        self.sub_px = offset;
        self.scroll_rows(rows, ctx);
        // The ends of the file are hard stops: an offset past either of them
        // would show a strip of nothing. Only a movement that *asked* to cross
        // the end is cut back — `rows < 0`, not `<= 0`. Standing on the first
        // line while scrolling down is a request to hide the top of it, which
        // is how a view leaves the top of a file three pixels at a time, and
        // treating that as an overscroll pinned it there until a gesture
        // happened to report a whole row at once.
        let at = (self.top, self.top_seg);
        if past_end(at, (self.max_top(), self.max_top_seg()), rows) {
            self.sub_px = 0;
        }
    }

    /// Whether this delta is a wheel notch, worth three rows, rather than the
    /// pixels a trackpad reports.
    ///
    /// A wheel only ever speaks in whole nominal lines, so one delta that is
    /// not a whole line is proof of a gesture — and for as long as that gesture
    /// lasts every delta is pixels, including the ones that happen to land on a
    /// multiple of sixteen.
    fn is_notch(&mut self, delta: f32) -> bool {
        if !could_be_notch(delta) {
            self.gesture_at = Some(Instant::now());
            return false;
        }
        !self
            .gesture_at
            .is_some_and(|at| at.elapsed() < GESTURE_MEMORY)
    }

    /// Moves by whole rows and lands on one: what a key press means, as
    /// against the pixels a gesture reports.
    fn scroll_rows_aligned(&mut self, delta: i64, ctx: &mut EventCtx<'_, M>) {
        self.sub_px = 0;
        self.scroll_rows(delta, ctx);
    }

    fn scroll_rows(&mut self, delta: i64, ctx: &mut EventCtx<'_, M>) {
        if self.follow {
            // Where paint has been showing us.
            self.top = self.max_top();
            self.top_seg = self.max_top_seg();
        }
        let was_top = self.top;
        if delta == 0 {
            return;
        }
        let (new_top, new_seg) = self.walk_rows(ctx.text, delta);
        self.top = new_top;
        self.top_seg = new_seg;
        let at_bottom = (new_top, new_seg) >= (self.max_top(), self.max_top_seg());
        if delta < 0 && self.follow && !at_bottom {
            self.follow = false;
            ctx.emit((self.to_message)(LogRequest::Follow(false)));
        }
        // Scrollback pages the file in; filter mode searches the window it has,
        // so reaching the top of a filtered list is not a request for more.
        let can_page = !self.filter && !self.waiting_older && !self.provisional;
        if new_top == 0 && (was_top > 0 || delta < 0) && can_page && self.first_number() > Some(1) {
            self.waiting_older = true;
            ctx.emit((self.to_message)(LogRequest::Older));
        }
    }

    /// Which row a point is on, from the layout paint left behind — with
    /// wrapping a row is as tall as the line needed, so nothing else knows.
    fn row_at(&self, bounds: Rect, p: Point) -> Option<usize> {
        if !bounds.contains(p) {
            return None;
        }
        let y = p.y - bounds.y;
        self.painted
            .borrow()
            .iter()
            .find(|&&(_, ry, h)| y >= ry && y < ry + h)
            .map(|&(row, _, _)| row)
    }
}

impl<M: 'static> Widget<M> for LogView<M> {
    fn accepts_pointer(&self) -> bool {
        true
    }

    fn focusable(&self) -> bool {
        true
    }

    fn paint(&self, ctx: &mut PaintCtx<'_>, canvas: &mut Pen<'_>) {
        let bounds = ctx.bounds;
        let theme = ctx.theme;
        let style = self.style;
        canvas.fill_rect(bounds, theme.color(Role::Base100));
        let metrics = ctx.text.metrics(style);
        let row_h = metrics.line_height().max(1) + 2;
        let rows = (bounds.height / row_h).max(1) as usize;
        self.visible.set(rows);
        let digits = self.total_lines.max(1).to_string().len().max(4);
        let zero_w = ctx.text.measure_line(style, "0").max(1);
        let gutter_w = if self.show_numbers {
            zero_w * digits as i32 + zero_w
        } else {
            zero_w / 2
        };
        // The text starts where the gutter ends, less however far it has been
        // scrolled sideways; the gutter itself does not move.
        let text_left = bounds.x + gutter_w;
        let text_w = bounds.width - gutter_w;
        self.text_area.set((text_w, zero_w));
        let scroll_x = if self.wrap {
            0
        } else {
            self.scroll_x.min(self.max_scroll_x())
        };
        let text_x = text_left - scroll_x;
        let wrap_w = (bounds.width - gutter_w - zero_w / 2).max(zero_w);
        self.wrap_width.set(wrap_w);
        // Following pins the window to its end at the row count paint actually
        // has; `top` is only authoritative once the user has scrolled away.
        // The anchor is worked out either way, because it is also where
        // scrolling has to stop and nothing outside paint can measure it.
        let bottom = self.bottom_anchor(ctx.text, wrap_w, rows);
        self.bottom.set(bottom);
        let (top, top_seg, offset) = if self.follow {
            (bottom.0, bottom.1, 0)
        } else {
            (self.top, self.top_seg, self.sub_px.min(row_h - 1))
        };
        let muted = theme
            .color(Role::Base300)
            .mix(theme.color(Role::BaseContent), 128);
        let fg = theme.color(Role::BaseContent);
        let sel = theme.color(Role::Accent).with_alpha(60);
        let hit = theme.color(Role::Warning).with_alpha(115);
        let hit_current = theme.color(Role::Accent).with_alpha(215);
        let current_row = self.current.and_then(|c| self.matches.get(c).copied());
        let (lo, hi) = self
            .selection
            .map(|(a, c)| (a.min(c), a.max(c)))
            .unwrap_or((usize::MAX, usize::MAX));

        let mut painted = self.painted.borrow_mut();
        painted.clear();
        let mut drawn = 0;
        // The top row starts above the viewport by whatever part of it has
        // been scrolled past; the canvas is clipped to the widget, so the part
        // that is off the top simply is not drawn.
        let mut y = bounds.y - offset;
        let mut index = top;
        // The first line is entered part-way through when the view starts
        // inside a wrapped line.
        let mut skip = top_seg;
        while y < bounds.y + bounds.height {
            let Some(line) = self.line_at(index) else {
                break;
            };
            let segs = self.segments(ctx.text, &line.text, wrap_w);
            let shown = segs.len().saturating_sub(skip).max(1);
            let height = shown as i32 * row_h;
            let row = Rect::new(bounds.x, y, bounds.width, height);
            painted.push((index, y - bounds.y, height));
            // Only the rows the clip reaches are drawn. After a scroll the tree
            // has moved the others and asks for the strip that came into view,
            // and laying out seventy lines of text to draw two is what the
            // move was meant to save. The row is still on the record above:
            // hit-testing needs every row, drawn or not.
            if !row.intersects(&canvas.clip()) {
                y += height;
                index += 1;
                skip = 0;
                continue;
            }
            drawn += 1;
            let mut pen = canvas.with_clip(row);
            let styled = self.styled(line);
            let line_style =
                (styled.line_rule >= 0).then(|| &self.styles[styled.line_rule as usize]);
            if let Some(bg) = line_style.and_then(|s| s.bg) {
                pen.fill_rect(Rect::new(text_left, row.y, text_w, height), bg);
            }
            let base_fg = line_style.and_then(|s| s.fg).unwrap_or(fg);

            // Gutter number, right-aligned on the line's first row, a
            // placeholder while provisional.
            if self.show_numbers && skip == 0 {
                let num = if self.provisional {
                    "·".repeat(digits.min(3))
                } else {
                    line.number.to_string()
                };
                let num_w = ctx.text.measure_line(style, &num);
                ctx.text.draw_line(
                    &mut pen,
                    style,
                    Point::new(text_left - zero_w - num_w, row.y + 1 + metrics.ascent),
                    &num,
                    muted,
                );
            }

            // Runs first: each character takes the highest-priority rule span
            // covering it. Backgrounds are laid down before any glyph so a
            // search hit can tint over them without tinting the text.
            let runs = &styled.runs;
            let hits = self
                .matcher
                .as_ref()
                .map(|m| byte_ranges(&line.text, &m.ranges(&line.text)))
                .unwrap_or_default();
            let tint = if Some(index) == current_row {
                hit_current
            } else {
                hit
            };
            // Clipped to the text area, so a line scrolled sideways stops at
            // the gutter instead of running into it.
            let text_clip = Rect::new(text_left, row.y, text_w, height);
            for (k, &(from, to)) in segs.iter().skip(skip).enumerate() {
                let mut pen = pen.with_clip(text_clip);
                let sy = row.y + k as i32 * row_h;
                let mut placed = Vec::with_capacity(runs.len());
                let mut x = text_x;
                for &(rs, re, rule) in runs {
                    // Runs are in order and `x` only grows, so once one starts
                    // past the right edge the rest of the line is off-screen.
                    // A log line is often half again as wide as the window.
                    if x >= bounds.right() {
                        break;
                    }
                    let (a, b) = (rs.max(from), re.min(to));
                    if b <= a {
                        continue;
                    }
                    let text = &line.text[a..b];
                    let w = ctx.text.measure_line(style, text);
                    placed.push((x, w, text, rule));
                    x += w;
                }
                for &(x, w, _, rule) in &placed {
                    if let Some(bg) = rule.and_then(|r| self.styles[r as usize].bg) {
                        pen.fill_rect(Rect::new(x, sy, w, row_h), bg);
                    }
                }
                for &(start, end) in &hits {
                    let (a, b) = (start.max(from), end.min(to));
                    if b <= a {
                        continue;
                    }
                    let x = text_x + measure_prefix(ctx.text, style, &line.text[from..a]);
                    let w = ctx.text.measure_line(style, &line.text[a..b]);
                    pen.fill_rect(Rect::new(x, sy, w.max(1), row_h), tint);
                }
                let baseline = sy + 1 + metrics.ascent;
                for &(x, _, text, rule) in &placed {
                    let color = rule
                        .and_then(|r| self.styles[r as usize].fg)
                        .unwrap_or(base_fg);
                    ctx.text
                        .draw_line(&mut pen, style, Point::new(x, baseline), text, color);
                }
            }
            if index >= lo && index <= hi {
                pen.fill_rect(row, sel);
            }
            y += height;
            index += 1;
            skip = 0;
        }

        self.drawn.set(drawn);

        // A thin bar along the bottom says how much lies to either side, as a
        // scroll view's would; nothing is drawn when everything fits.
        let max_x = self.max_scroll_x();
        if !self.wrap && max_x > 0 {
            let content_w = (text_w + max_x).max(1);
            let track = Rect::new(text_left, bounds.bottom() - 6, text_w, 4);
            let thumb_w = (text_w as i64 * text_w as i64 / content_w as i64).max(24) as i32;
            let thumb_x = text_left
                + ((text_w - thumb_w) as i64 * scroll_x as i64 / max_x.max(1) as i64) as i32;
            canvas.fill_rounded_rect(track, 2, theme.color(Role::Base300).with_alpha(120));
            canvas.fill_rounded_rect(
                Rect::new(thumb_x, track.y, thumb_w, track.height),
                2,
                theme.color(Role::BaseContent).with_alpha(110),
            );
        }
    }

    fn on_event(&mut self, event: &Event<'_>, ctx: &mut EventCtx<'_, M>) -> Handled {
        let Event::Input(input) = event else {
            return Handled::No;
        };
        let row_h = ctx.text.metrics(self.style).line_height().max(1) + 2;
        let bounds = ctx.bounds;
        match input {
            InputEvent::PointerScroll {
                delta_x, delta_y, ..
            } => {
                // Sideways first: a two-finger drag to the side, or a wheel with
                // shift held. Pixels either way, clamped to the widest line.
                let mut moved = false;
                if !self.wrap && *delta_x != 0.0 {
                    let (_, zero_w) = self.text_area.get();
                    let px = if self.is_notch(*delta_x) {
                        delta_x / WHEEL_LINE_PX * 3.0 * zero_w as f32
                    } else {
                        *delta_x
                    };
                    let next = (self.scroll_x + px.round() as i32).clamp(0, self.max_scroll_x());
                    moved = next != self.scroll_x;
                    self.scroll_x = next;
                }
                if *delta_y == 0.0 {
                    return if moved { Handled::Yes } else { Handled::No };
                }
                // The backend reports pixels either way, but by two different
                // routes: a trackpad's own fine-grained deltas, or a wheel
                // notch multiplied out to a nominal line. Both are followed
                // pixel for pixel — that is what makes a gesture track the
                // fingers — except that a notch is worth the three lines every
                // other application gives it rather than the sixteen nominal
                // pixels the backend names, which on a Retina display is less
                // than one row. The sign is the toolkit's: a positive delta
                // moves the content the way `Ui::scroll_by` would.
                let pixels = if self.is_notch(*delta_y) {
                    delta_y / WHEEL_LINE_PX * 3.0 * row_h as f32
                } else {
                    *delta_y
                };
                // Sub-pixel remainders are kept: a slow drag is a run of them.
                let total = pixels + self.scroll_residue;
                let whole = total.trunc();
                self.scroll_residue = total - whole;
                let before = (self.top, self.top_seg, self.sub_px, self.follow);
                if whole != 0.0 {
                    self.scroll_pixels(whole as i32, row_h, ctx);
                }
                if crate::trace::enabled() {
                    crate::trace::log(format_args!(
                        "scroll dy={delta_y} px={pixels} whole={whole} top={} seg={} sub={} follow={} row_h={row_h}",
                        self.top, self.top_seg, self.sub_px, self.follow
                    ));
                }
                // A view already against the end of the file has nothing to
                // show for a gesture that would take it further. Reporting the
                // event as handled would repaint and present an identical
                // frame for every event of a flick — a hundred a second of
                // them, each swapping the buffer the compositor is reading,
                // which is what makes a log that is not moving shimmer.
                if (self.top, self.top_seg, self.sub_px, self.follow) == before && !moved {
                    return Handled::No;
                }
                // A vertical move and nothing else is one the tree can make by
                // shifting the rows already on screen and asking for the strip
                // that came into view. Not while following, when paint pins the
                // view to the end and `top` says nothing about where the rows
                // were; not with a sideways move, which every row is part of.
                if !moved && !before.3 && !self.follow {
                    let rows =
                        self.rows_between(ctx.text, (before.0, before.1), (self.top, self.top_seg));
                    let dy = rows
                        .map(|rows| rows * row_h as i64 + (self.sub_px - before.2) as i64)
                        .filter(|dy| *dy != 0 && dy.unsigned_abs() < bounds.height as u64);
                    if let Some(dy) = dy {
                        ctx.scrolled(self.moving_area(bounds), Point::new(0, dy as i32));
                    }
                }
                Handled::Yes
            }
            InputEvent::PointerButton {
                button: PointerButton::Left,
                state,
                position,
                ..
            } => {
                match state {
                    ElementState::Down => {
                        ctx.request_focus();
                        if let Some(row) = self.row_at(bounds, *position) {
                            self.selection = Some((row, row));
                            self.dragging = true;
                        } else {
                            self.selection = None;
                        }
                    }
                    ElementState::Up => self.dragging = false,
                }
                Handled::Yes
            }
            InputEvent::PointerMoved { position } if self.dragging => {
                if let (Some((a, _)), Some(row)) = (self.selection, self.row_at(bounds, *position))
                {
                    self.selection = Some((a, row));
                    return Handled::Yes;
                }
                Handled::No
            }
            InputEvent::Key {
                code,
                state: ElementState::Down,
                modifiers,
                ..
            } => {
                let page = self.visible_rows() as i64;
                let cmd =
                    modifiers.contains(Modifiers::SUPER) || modifiers.contains(Modifiers::CTRL);
                match code {
                    KeyCode::PageUp => self.scroll_rows_aligned(-page, ctx),
                    KeyCode::PageDown => self.scroll_rows_aligned(page, ctx),
                    KeyCode::ArrowUp => self.scroll_rows_aligned(-1, ctx),
                    KeyCode::ArrowDown => self.scroll_rows_aligned(1, ctx),
                    KeyCode::Home => self.scroll_rows_aligned(-(self.row_count() as i64), ctx),
                    KeyCode::End => {
                        if self.detached {
                            ctx.emit((self.to_message)(LogRequest::Reattach));
                        }
                        self.follow = true;
                        self.scroll_to_bottom();
                        ctx.emit((self.to_message)(LogRequest::Follow(true)));
                    }
                    KeyCode::C if cmd => {
                        if self.selection.is_some() {
                            ctx.emit((self.to_message)(LogRequest::Copy));
                        }
                    }
                    KeyCode::Escape => self.selection = None,
                    _ => return Handled::No,
                }
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}

/// Width of a prefix, without allocating a run for it.
fn measure_prefix(text: &mut TextEngine, style: TextStyle, prefix: &str) -> i32 {
    if prefix.is_empty() {
        0
    } else {
        text.measure_line(style, prefix)
    }
}

/// UTF-16 ranges (what the engine reports) as byte ranges into `text`.
fn byte_ranges(text: &str, ranges: &[ctail_core::TextRange]) -> Vec<(usize, usize)> {
    if ranges.is_empty() {
        return Vec::new();
    }
    let table = u16_table(text);
    ranges
        .iter()
        .map(|r| (byte_at(&table, r.start), byte_at(&table, r.end)))
        .filter(|(a, b)| b > a)
        .collect()
}

/// UTF-16 offset -> byte offset for every character boundary, plus the end.
fn u16_table(text: &str) -> Vec<(u32, usize)> {
    let mut table = Vec::with_capacity(text.len() + 1);
    let mut u = 0u32;
    for (b, ch) in text.char_indices() {
        table.push((u, b));
        u += ch.len_utf16() as u32;
    }
    table.push((u, text.len()));
    table
}

fn byte_at(table: &[(u32, usize)], off: u32) -> usize {
    match table.binary_search_by_key(&off, |&(k, _)| k) {
        Ok(i) => table[i].1,
        Err(i) => table[i.min(table.len() - 1)].1,
    }
}

/// Splits `text` into (start, end, rule) byte ranges by UTF-16 span
/// boundaries; where spans overlap the later one (higher priority) wins,
/// matching the paint order. Ranges rather than slices, because a wrapped line
/// draws each of them in pieces.
fn split_runs(text: &str, spans: &[ctail_core::Span]) -> Vec<(usize, usize, Option<u32>)> {
    if spans.is_empty() {
        return vec![(0, text.len(), None)];
    }
    let table = u16_table(text);
    // Per-character rule; later spans overwrite earlier ones.
    let mut per_char: Vec<Option<u32>> = vec![None; table.len() - 1];
    for span in spans {
        let start = byte_at(&table, span.start);
        let end = byte_at(&table, span.end);
        for (i, &(_, b)) in table.iter().enumerate().take(table.len() - 1) {
            if b >= start && b < end {
                per_char[i] = Some(span.rule);
            }
        }
    }
    let mut runs = Vec::new();
    let mut run_start = 0usize;
    let mut current = per_char.first().copied().flatten();
    for i in 1..=per_char.len() {
        let next = per_char.get(i).copied().flatten();
        if i == per_char.len() || next != current {
            runs.push((table[run_start].1, table[i].1, current));
            run_start = i;
            current = next;
        }
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;
    use denise_text::TextEngine;

    /// A view over `texts`, wrapping on, measured with the built-in font.
    fn view(texts: &[&str]) -> (LogView<()>, TextEngine, TextStyle) {
        let style = TextStyle::built_in(12);
        let mut v = LogView::new(|_| (), style, &[], 200);
        v.set_word_wrap(true);
        v.append(
            texts
                .iter()
                .enumerate()
                .map(|(i, t)| LogLine {
                    number: i as i64 + 1,
                    text: (*t).into(),
                })
                .collect(),
            false,
        );
        (v, TextEngine::new(), style)
    }

    /// Width of `n` characters of the built-in font, which is fixed-width.
    fn cols(text: &mut TextEngine, style: TextStyle, n: i32) -> i32 {
        text.measure_line(style, "0") * n
    }

    #[test]
    fn the_memory_a_view_reports_covers_the_lines_it_holds() {
        let texts = [
            "first line",
            "a second, rather longer line of text",
            "third",
        ];
        let (v, _, _) = view(&texts);
        let text: usize = texts.iter().map(|t| t.len()).sum();
        let slots = texts.len() * std::mem::size_of::<LogLine>();
        assert!(
            v.memory_bytes() >= text + slots,
            "{} bytes reported for {text} bytes of text in {slots} bytes of slots",
            v.memory_bytes()
        );
        let (empty, _, _) = view(&[]);
        assert!(empty.memory_bytes() < v.memory_bytes());
    }

    #[test]
    fn breaks_after_the_last_space_that_fits() {
        let (v, mut text, style) = view(&["alpha beta gamma"]);
        let w = cols(&mut text, style, 11);
        let segs = v.segments(&mut text, "alpha beta gamma", w);
        let pieces: Vec<&str> = segs
            .iter()
            .map(|&(a, b)| &"alpha beta gamma"[a..b])
            .collect();
        assert_eq!(pieces, vec!["alpha beta ", "gamma"]);
    }

    #[test]
    fn breaks_inside_a_word_that_never_fits() {
        let line = "aaaaaaaaaa";
        let (v, mut text, style) = view(&[line]);
        let w = cols(&mut text, style, 4);
        let segs = v.segments(&mut text, line, w);
        assert_eq!(segs.len(), 3);
        assert_eq!(&line[segs[0].0..segs[0].1], "aaaa");
        // Every byte of the line is accounted for exactly once, in order.
        assert_eq!(segs[0].0, 0);
        assert_eq!(segs.last().unwrap().1, line.len());
        assert!(segs.windows(2).all(|p| p[0].1 == p[1].0));
    }

    #[test]
    fn an_empty_line_still_takes_one_row() {
        let (v, mut text, style) = view(&[""]);
        let w = cols(&mut text, style, 10);
        assert_eq!(v.segments(&mut text, "", w), vec![(0, 0)]);
    }

    #[test]
    fn wrapping_off_never_breaks() {
        let style = TextStyle::built_in(12);
        let v: LogView<()> = LogView::new(|_| (), style, &[], 200);
        let mut text = TextEngine::new();
        let line = "a b c d e f g h i j k l m n o p";
        assert_eq!(v.segments(&mut text, line, 8), vec![(0, line.len())]);
    }

    #[test]
    fn the_bottom_anchor_leaves_the_last_row_at_the_bottom() {
        // Rows are [one] [two ] [three ] [four] [five]: the middle line wraps
        // into three, so a three-row view starts on the second of them.
        let (v, mut text, style) = view(&["one", "two three four", "five"]);
        let w = cols(&mut text, style, 7);
        assert_eq!(v.seg_count(&mut text, 1, w), 3);
        assert_eq!(v.bottom_anchor(&mut text, w, 3), (1, 1));
        assert_eq!(v.bottom_anchor(&mut text, w, 4), (1, 0));
        assert_eq!(v.bottom_anchor(&mut text, w, 5), (0, 0));
        // Taller than the content: the top of the file, not a negative row.
        assert_eq!(v.bottom_anchor(&mut text, w, 99), (0, 0));
    }

    #[test]
    fn scrolling_steps_through_a_wrapped_line_a_row_at_a_time() {
        let (mut v, mut text, style) = view(&["one", "two three four", "five"]);
        let w = cols(&mut text, style, 7);
        v.wrap_width.set(w);
        v.follow = false;
        // A one-row view, so every row of the file can be scrolled to.
        v.bottom.set(v.bottom_anchor(&mut text, w, 1));
        assert_eq!(v.bottom.get(), (2, 0));

        v.top = 0;
        v.top_seg = 0;
        assert_eq!(v.walk_rows(&mut text, 1), (1, 0));
        v.top = 1;
        assert_eq!(v.walk_rows(&mut text, 1), (1, 1));
        assert_eq!(v.walk_rows(&mut text, 2), (1, 2));
        assert_eq!(v.walk_rows(&mut text, 3), (2, 0));
        // Clamped at the bottom anchor, never past the end.
        assert_eq!(v.walk_rows(&mut text, 50), (2, 0));
        // And back up again, through the same rows.
        v.top_seg = 1;
        assert_eq!(v.walk_rows(&mut text, -1), (1, 0));
        assert_eq!(v.walk_rows(&mut text, -2), (0, 0));
        assert_eq!(v.walk_rows(&mut text, -50), (0, 0));
    }

    #[test]
    fn a_gesture_shorter_than_a_row_still_moves_the_view() {
        // Down by three pixels of a twenty-pixel row: no row is crossed, and
        // the view sits three pixels into the one it was on.
        assert_eq!(split_pixels(0, 3, 20), (0, 3));
        // Three more, and it is six in — a drag adds up rather than rounding
        // away, which is the whole point of keeping the offset.
        assert_eq!(split_pixels(3, 3, 20), (0, 6));
        // Up by three from the top of a row: the row above, near its bottom.
        assert_eq!(split_pixels(0, -3, 20), (-1, 17));
        // A whole row lands exactly on the next one.
        assert_eq!(split_pixels(0, 20, 20), (1, 0));
        assert_eq!(split_pixels(5, -25, 20), (-1, 0));
        // Several rows at once, as a flick reports.
        assert_eq!(split_pixels(0, 55, 20), (2, 15));
    }

    fn numbered(number: i64, text: &str) -> LogLine {
        LogLine {
            number,
            text: text.into(),
        }
    }

    #[test]
    fn the_widest_line_follows_the_window() {
        let (mut v, _, _) = view(&["short", "a much longer line of text"]);
        assert_eq!(v.widest(), "a much longer line of text".len());
        v.prepend(vec![numbered(0, &"x".repeat(40))]);
        assert_eq!(v.widest(), 40, "an older line can be the widest");
        // Filling the window past its cap drops the front, and the count
        // with it: the widest line must be one that is still there.
        let cap = v.cap;
        v.append(
            (0..cap as i64 + 5)
                .map(|i| numbered(100 + i, "mid"))
                .collect(),
            false,
        );
        assert_eq!(v.widest(), 3, "a dropped line no longer counts");
        v.reset();
        assert_eq!(v.widest(), 0);
    }

    #[test]
    fn only_a_movement_that_asked_to_cross_an_end_is_cut_back() {
        let top = (0, 0);
        let middle = (5, 0);
        let max = (9, 0);
        // Pushing up against the top, and sitting at the bottom: no offset.
        assert!(past_end(top, max, -1));
        assert!(past_end(max, max, 1));
        assert!(past_end(max, max, 0));
        // Leaving the top downwards keeps its offset — without this the view
        // was pinned to the first line until a gesture reported a whole row.
        assert!(!past_end(top, max, 0));
        // And anywhere in the middle, either way.
        assert!(!past_end(middle, max, 0));
        assert!(!past_end(middle, max, -1));
    }

    #[test]
    fn a_wheel_notch_is_told_from_a_trackpad_gesture() {
        let (mut v, _, _) = view(&["one"]);
        // On its own, a whole number of nominal lines is a notch.
        assert!(v.is_notch(16.0));
        assert!(v.is_notch(-48.0));
        // What a wheel cannot have sent is a gesture, and says so.
        assert!(!v.is_notch(3.0));
        assert!(!v.is_notch(-17.5));
        // Standing still is not a notch, or every idle event would scroll.
        assert!(!v.is_notch(0.0));
    }

    #[test]
    fn a_gesture_that_lands_on_a_whole_line_is_still_a_gesture() {
        let (mut v, _, _) = view(&["one"]);
        // Six pixels proves fingers are on the glass; the forty-eight that
        // follows is the same drag, not a wheel. A tenth of a trackpad's
        // deltas are whole multiples of sixteen, and taking those for notches
        // threw the view five rows mid-drag.
        assert!(!v.is_notch(6.0));
        assert!(!v.is_notch(48.0));
        assert!(!v.is_notch(16.0));
    }
}
