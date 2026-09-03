//! The log surface: a virtualized, highlighted, searchable view over a window
//! of lines. Only the rows inside the widget's bounds are ever laid out or
//! drawn; the window itself is a bounded slice of the file that the app keeps
//! fed from the engine (live lines at the bottom, scrollback at the top).

use ctail_core::{Highlighter, LogLine, Rule, SearchMatcher};
use denise::{
    Color, ElementState, InputEvent, KeyCode, Modifiers, Point, PointerButton, Rect, Role,
};
use denise_render::Canvas;
use denise_text::{TextEngine, TextStyle};
use denise_ui::widget::{Event, EventCtx, Handled, PaintCtx, Widget};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

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
    /// Where "scrolled all the way down" is, as (row, segment).
    bottom: Cell<(usize, usize)>,
    /// Character advances, memoised: wrapping walks a line character by
    /// character, and a log is written in the same few dozen of them.
    advances: RefCell<HashMap<char, i32>>,
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
            wrap: false,
            follow: true,
            total_lines: 0,
            selection: None,
            dragging: false,
            highlighter: Arc::new(Highlighter::new(&[])),
            styles: Vec::new(),
            visible: Cell::new(0),
            wrap_width: Cell::new(0),
            painted: RefCell::new(Vec::new()),
            bottom: Cell::new((0, 0)),
            advances: RefCell::new(HashMap::new()),
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
        self.top = 0;
        self.top_seg = 0;
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
            }
        } else if self.lines.len() > self.cap * 3 {
            let over = self.lines.len() - self.cap * 3;
            self.lines.drain(..over);
            self.shift_indices(over);
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

    pub fn visible_rows(&self) -> usize {
        self.visible.get().max(1)
    }

    fn scroll_to_bottom(&mut self) {
        self.top = self.max_top();
        self.top_seg = self.max_top_seg();
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

    fn scroll_rows(&mut self, delta: i64, ctx: &mut EventCtx<'_, M>) {
        if self.follow {
            // Where paint has been showing us.
            self.top = self.max_top();
            self.top_seg = self.max_top_seg();
        }
        let was_top = self.top;
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

    fn paint(&self, ctx: &mut PaintCtx<'_>, canvas: &mut Canvas<'_>) {
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
        let text_x = bounds.x + gutter_w;
        let wrap_w = (bounds.width - gutter_w - zero_w / 2).max(zero_w);
        self.wrap_width.set(wrap_w);
        // Following pins the window to its end at the row count paint actually
        // has; `top` is only authoritative once the user has scrolled away.
        // The anchor is worked out either way, because it is also where
        // scrolling has to stop and nothing outside paint can measure it.
        let bottom = self.bottom_anchor(ctx.text, wrap_w, rows);
        self.bottom.set(bottom);
        let (top, top_seg) = if self.follow {
            bottom
        } else {
            (self.top, self.top_seg)
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
        let mut y = bounds.y;
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
            let mut pen = canvas.with_clip(row);
            let styled = self.highlighter.apply(&line.text);
            let line_style =
                (styled.line_rule >= 0).then(|| &self.styles[styled.line_rule as usize]);
            if let Some(bg) = line_style.and_then(|s| s.bg) {
                pen.fill_rect(Rect::new(text_x, row.y, row.width - gutter_w, height), bg);
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
                    Point::new(text_x - zero_w - num_w, row.y + 1 + metrics.ascent),
                    &num,
                    muted,
                );
            }

            // Runs first: each character takes the highest-priority rule span
            // covering it. Backgrounds are laid down before any glyph so a
            // search hit can tint over them without tinting the text.
            let runs = split_runs(&line.text, &styled.spans);
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
            for (k, &(from, to)) in segs.iter().skip(skip).enumerate() {
                let sy = row.y + k as i32 * row_h;
                let mut placed = Vec::with_capacity(runs.len());
                let mut x = text_x;
                for &(rs, re, rule) in &runs {
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
    }

    fn on_event(&mut self, event: &Event<'_>, ctx: &mut EventCtx<'_, M>) -> Handled {
        let Event::Input(input) = event else {
            return Handled::No;
        };
        let row_h = ctx.text.metrics(self.style).line_height().max(1) + 2;
        let bounds = ctx.bounds;
        match input {
            InputEvent::PointerScroll { delta_y, .. } => {
                // Wheel deltas arrive in lines (mouse) or pixels (trackpad,
                // larger). The sign is the toolkit's: a positive delta moves
                // the content the way `Ui::scroll_by` would, which is what the
                // system's own scrolling direction resolves to.
                let rows = if delta_y.abs() > 20.0 {
                    *delta_y / row_h as f32
                } else {
                    *delta_y
                };
                let delta = rows.round() as i64;
                if delta != 0 {
                    self.scroll_rows(delta, ctx);
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
                    KeyCode::PageUp => self.scroll_rows(-page, ctx),
                    KeyCode::PageDown => self.scroll_rows(page, ctx),
                    KeyCode::ArrowUp => self.scroll_rows(-1, ctx),
                    KeyCode::ArrowDown => self.scroll_rows(1, ctx),
                    KeyCode::Home => self.scroll_rows(-(self.row_count() as i64), ctx),
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
}
