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
use std::cell::Cell;
use std::collections::VecDeque;
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
    follow: bool,
    total_lines: i64,
    selection: Option<(usize, usize)>, // (anchor, cursor) as displayed rows
    dragging: bool,
    highlighter: Arc<Highlighter>,
    styles: Vec<RuleStyle>,
    /// Rows that fit, discovered while painting (the one place that knows the
    /// row height) and read back by scrolling.
    visible: Cell<usize>,
    waiting_older: bool,

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
            follow: true,
            total_lines: 0,
            selection: None,
            dragging: false,
            highlighter: Arc::new(Highlighter::new(&[])),
            styles: Vec::new(),
            visible: Cell::new(0),
            waiting_older: false,
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

    pub fn following(&self) -> bool {
        self.follow
    }

    pub fn set_follow(&mut self, follow: bool) {
        self.follow = follow;
        if follow {
            self.scroll_to_bottom();
        }
    }

    pub fn total_lines(&self) -> i64 {
        self.total_lines
    }

    pub fn first_number(&self) -> Option<i64> {
        self.lines.front().map(|l| l.number)
    }

    pub fn reset(&mut self) {
        self.lines.clear();
        self.top = 0;
        self.selection = None;
        self.provisional = false;
        self.total_lines = 0;
        self.waiting_older = false;
        self.recompute_search();
    }

    /// Live lines from the engine (numbered locally until `apply_base`).
    pub fn append(&mut self, new: Vec<LogLine>, provisional: bool) {
        if new.is_empty() {
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

    pub fn selected_text(&self) -> Option<String> {
        let (a, c) = self.selection?;
        let (lo, hi) = (a.min(c), a.max(c));
        let text: Vec<&str> = (lo..=hi)
            .filter_map(|row| self.line_at(row))
            .map(|l| l.text.as_str())
            .collect();
        (!text.is_empty()).then(|| text.join("\n"))
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
        self.top = self.top.min(self.max_top());
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

    fn shift_indices(&mut self, by: usize) {
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
    }

    fn max_top(&self) -> usize {
        self.row_count().saturating_sub(self.visible_rows())
    }

    fn scroll_rows(&mut self, delta: i64, ctx: &mut EventCtx<'_, M>) {
        if self.follow {
            self.top = self.max_top(); // where paint has been showing us
        }
        let was_top = self.top;
        let new_top = (self.top as i64 + delta).clamp(0, self.max_top() as i64) as usize;
        self.top = new_top;
        let at_bottom = new_top >= self.max_top();
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

    fn row_at(&self, bounds: Rect, row_h: i32, p: Point) -> Option<usize> {
        if !bounds.contains(p) || row_h <= 0 {
            return None;
        }
        let top = if self.follow {
            self.max_top()
        } else {
            self.top
        };
        let row = ((p.y - bounds.y) / row_h) as usize + top;
        (row < self.row_count()).then_some(row)
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
        // Following pins the window to its end at the row count paint actually
        // has; `top` is only authoritative once the user has scrolled away.
        let top = if self.follow {
            self.row_count().saturating_sub(rows)
        } else {
            self.top
        };
        let digits = self.total_lines.max(1).to_string().len().max(4);
        let zero_w = ctx.text.measure_line(style, "0").max(1);
        let gutter_w = zero_w * digits as i32 + zero_w;
        let text_x = bounds.x + gutter_w;
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

        for i in 0..rows {
            let index = top + i;
            let Some(line) = self.line_at(index) else {
                break;
            };
            let row = Rect::new(bounds.x, bounds.y + i as i32 * row_h, bounds.width, row_h);
            let mut pen = canvas.with_clip(row);
            let styled = self.highlighter.apply(&line.text);
            let line_style =
                (styled.line_rule >= 0).then(|| &self.styles[styled.line_rule as usize]);
            if let Some(bg) = line_style.and_then(|s| s.bg) {
                pen.fill_rect(Rect::new(text_x, row.y, row.width - gutter_w, row_h), bg);
            }
            let base_fg = line_style.and_then(|s| s.fg).unwrap_or(fg);

            // Gutter number, right-aligned, a placeholder while provisional.
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

            // Runs first: each character takes the highest-priority rule span
            // covering it. Backgrounds are laid down before any glyph so a
            // search hit can tint over them without tinting the text.
            let runs = split_runs(&line.text, &styled.spans);
            let mut placed = Vec::with_capacity(runs.len());
            let mut x = text_x;
            for (text, rule) in runs {
                let w = ctx.text.measure_line(style, text);
                placed.push((x, w, text, rule));
                x += w;
            }
            for &(x, w, _, rule) in &placed {
                if let Some(bg) = rule.and_then(|r| self.styles[r as usize].bg) {
                    pen.fill_rect(Rect::new(x, row.y, w, row_h), bg);
                }
            }
            if let Some(m) = &self.matcher {
                let tint = if Some(index) == current_row {
                    hit_current
                } else {
                    hit
                };
                for (start, end) in byte_ranges(&line.text, &m.ranges(&line.text)) {
                    let x = text_x + measure_prefix(ctx.text, style, &line.text[..start]);
                    let w = ctx.text.measure_line(style, &line.text[start..end]);
                    pen.fill_rect(Rect::new(x, row.y, w.max(1), row_h), tint);
                }
            }
            let baseline = row.y + 1 + metrics.ascent;
            for &(x, _, text, rule) in &placed {
                let color = rule
                    .and_then(|r| self.styles[r as usize].fg)
                    .unwrap_or(base_fg);
                ctx.text
                    .draw_line(&mut pen, style, Point::new(x, baseline), text, color);
            }
            if index >= lo && index <= hi {
                pen.fill_rect(row, sel);
            }
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
                        if let Some(row) = self.row_at(bounds, row_h, *position) {
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
                if let (Some((a, _)), Some(row)) =
                    (self.selection, self.row_at(bounds, row_h, *position))
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

/// Splits `text` into (run, rule) pieces by UTF-16 span boundaries; where spans
/// overlap the later one (higher priority) wins, matching the paint order.
fn split_runs<'a>(text: &'a str, spans: &[ctail_core::Span]) -> Vec<(&'a str, Option<u32>)> {
    if spans.is_empty() {
        return vec![(text, None)];
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
            runs.push((&text[table[run_start].1..table[i].1], current));
            run_start = i;
            current = next;
        }
    }
    runs
}
