//! The tab strip.
//!
//! Its own widget rather than the toolkit's `Tabs`, because a ctail tab is more
//! than a label: it carries a user colour, a close button, and a right-click
//! that has to report *which* tab was hit. Widths are computed the same way in
//! painting and in hit testing, so a click always lands on what was drawn.
//!
//! The proportions are the macOS window's: a 24pt pill per file, the user's
//! colour as a dot at the leading edge, and a "+" after the last one. They are
//! written here in logical pixels and scaled, because everything the widget is
//! handed — bounds, text metrics — is physical, and a 10-pixel pad on a Retina
//! display is not a pad.

use denise::{Color, ElementState, InputEvent, Pen, Point, PointerButton, Rect, Role};
use denise_text::{TextEngine, TextStyle};
use denise_ui::widget::{Event, EventCtx, Handled, PaintCtx, Widget};

/// One tab, as the strip needs to draw it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TabItem {
    /// What to show: the user's label, or the file name.
    pub label: String,
    /// Hex colour, or empty for none.
    pub color: String,
}

/// The strip's proportions in logical pixels, from the macOS tab bar.
#[derive(Clone, Copy, Debug)]
pub struct Metrics {
    /// Inset before the first tab.
    lead: i32,
    /// Height of a tab; the strip is taller, and the tabs sit centred in it.
    height: i32,
    /// Between two tabs.
    gap: i32,
    /// The user's colour.
    dot: i32,
    /// Inset inside a tab, and between the dot and the label.
    pad: i32,
    /// The close cross.
    close: i32,
    min_w: i32,
    max_w: i32,
    /// The "+" that opens a file.
    plus: i32,
    radius: i32,
}

impl Metrics {
    pub fn new(scale: f32) -> Self {
        let s = |v: f32| (v * scale + 0.5) as i32;
        Self {
            lead: s(4.0),
            height: s(24.0),
            gap: s(2.0),
            dot: s(8.0),
            pad: s(8.0),
            close: s(14.0),
            min_w: s(78.0),
            max_w: s(220.0),
            plus: s(24.0),
            radius: s(5.0),
        }
    }
}

pub struct TabBar<M> {
    items: Vec<TabItem>,
    selected: usize,
    hovered: Option<usize>,
    /// Whether the pointer is over the hovered tab's close button.
    on_close_button: bool,
    /// Whether the pointer is over the "+".
    on_plus: bool,
    select: fn(usize) -> M,
    close: fn(usize) -> M,
    context: fn(usize) -> M,
    new: fn() -> M,
    style: TextStyle,
    metrics: Metrics,
}

/// What a point in the strip is over.
enum Hit {
    /// A tab, and whether the close cross was hit.
    Tab(usize, bool),
    Plus,
}

impl<M: 'static> TabBar<M> {
    pub fn new(
        select: fn(usize) -> M,
        close: fn(usize) -> M,
        context: fn(usize) -> M,
        new: fn() -> M,
        scale: f32,
    ) -> Self {
        Self {
            items: Vec::new(),
            selected: 0,
            hovered: None,
            on_close_button: false,
            on_plus: false,
            select,
            close,
            context,
            new,
            style: TextStyle::built_in(14),
            metrics: Metrics::new(scale),
        }
    }

    pub fn with_style(mut self, style: TextStyle) -> Self {
        self.style = style;
        self
    }

    pub fn set_items(&mut self, items: Vec<TabItem>) {
        self.items = items;
        if self.selected >= self.items.len() {
            self.selected = self.items.len().saturating_sub(1);
        }
    }

    pub fn set_selected(&mut self, index: usize) {
        self.selected = index;
    }

    pub fn items(&self) -> &[TabItem] {
        &self.items
    }

    pub fn style(&self) -> TextStyle {
        self.style
    }

    pub fn metrics(&self) -> Metrics {
        self.metrics
    }

    fn layout(&self, bounds: Rect, text: &mut TextEngine) -> Vec<Rect> {
        layout(&self.items, bounds, self.style, self.metrics, text)
    }

    fn close_rect(&self, tab: Rect) -> Rect {
        let m = self.metrics;
        Rect::new(
            tab.right() - m.pad + m.pad / 4 - m.close,
            tab.y + (tab.height - m.close) / 2,
            m.close,
            m.close,
        )
    }

    fn hit(&self, bounds: Rect, text: &mut TextEngine, p: Point) -> Option<Hit> {
        let rects = self.layout(bounds, text);
        if let Some((i, r)) = rects.iter().enumerate().find(|(_, r)| r.contains(p)) {
            return Some(Hit::Tab(i, self.close_rect(*r).contains(p)));
        }
        plus_rect(&rects, bounds, self.metrics)
            .filter(|r| r.contains(p))
            .map(|_| Hit::Plus)
    }
}

/// Tab rectangles left to right. A free function so painting, hit testing and
/// a caller anchoring a menu can never disagree about where a tab is — and so
/// the last of those can measure text without holding a borrow of the widget.
pub fn layout(
    items: &[TabItem],
    bounds: Rect,
    style: TextStyle,
    m: Metrics,
    text: &mut TextEngine,
) -> Vec<Rect> {
    let mut out = Vec::with_capacity(items.len());
    let mut x = bounds.x + m.lead;
    let y = bounds.y + (bounds.height - m.height) / 2;
    for item in items {
        let label = text.measure_line(style, &item.label);
        let dot = if item.color.is_empty() {
            0
        } else {
            m.dot + m.pad / 2
        };
        let width = (m.pad + dot + label + m.pad / 2 + m.close + m.pad).clamp(m.min_w, m.max_w);
        out.push(Rect::new(x, y, width, m.height));
        x += width + m.gap;
    }
    out
}

/// Where the "+" goes: after the last tab, or at the leading edge when there
/// are none.
fn plus_rect(tabs: &[Rect], bounds: Rect, m: Metrics) -> Option<Rect> {
    let x = tabs
        .last()
        .map(|t| t.right() + m.gap * 2)
        .unwrap_or(bounds.x + m.lead);
    (x + m.plus <= bounds.right())
        .then(|| Rect::new(x, bounds.y + (bounds.height - m.plus) / 2, m.plus, m.plus))
}

impl<M: 'static> Widget<M> for TabBar<M> {
    fn accepts_pointer(&self) -> bool {
        true
    }

    fn paint(&self, ctx: &mut PaintCtx<'_>, canvas: &mut Pen<'_>) {
        let theme = ctx.theme;
        let bounds = ctx.bounds;
        let m = self.metrics;
        canvas.fill_rect(bounds, theme.color(Role::Base200));
        let rects = self.layout(bounds, ctx.text);
        let metrics = ctx.text.metrics(self.style);
        let muted = theme
            .color(Role::BaseContent)
            .mix(theme.color(Role::Base200), 96);

        for (i, tab) in rects.iter().enumerate() {
            let active = i == self.selected;
            let hovered = self.hovered == Some(i);
            let mut pen = canvas.with_clip(*tab);
            // The active tab is the colour of the log surface below it, so the
            // two read as one sheet; the rest sit in the strip.
            if active {
                pen.fill_rounded_rect(*tab, m.radius, theme.color(Role::Base100));
            } else if hovered {
                pen.fill_rounded_rect(*tab, m.radius, theme.color(Role::Base300));
            }
            let baseline = tab.y + (tab.height - metrics.line_height()) / 2 + metrics.ascent;
            let item = &self.items[i];
            let mut x = tab.x + m.pad;
            if !item.color.is_empty() {
                let dot = Rect::new(x, tab.y + (tab.height - m.dot) / 2, m.dot, m.dot);
                pen.fill_rounded_rect(dot, m.dot / 2, crate::theme::hex(&item.color));
                x += m.dot + m.pad / 2;
            }
            let fg = if active {
                theme.color(Role::BaseContent)
            } else {
                muted
            };
            let text_w = (self.close_rect(*tab).x - m.pad / 2 - x).max(0);
            {
                let mut label = pen.with_clip(Rect::new(x, tab.y, text_w, tab.height));
                ctx.text.draw_line(
                    &mut label,
                    self.style,
                    Point::new(x, baseline),
                    &item.label,
                    fg,
                );
            }
            // Every tab carries its cross, as the macOS strip does — the
            // width was reserved for it either way, and a tab that only shows
            // its close button once the pointer is on it is a tab nobody finds
            // the close button on. It takes the label's colour, so an inactive
            // one is as quiet as its name.
            let close = self.close_rect(*tab);
            if self.on_close_button && hovered {
                pen.fill_rounded_rect(close, m.radius, theme.color(Role::Base300));
            }
            cross(&mut pen, close, fg);
        }

        if let Some(plus) = plus_rect(&rects, bounds, m) {
            if self.on_plus {
                canvas.fill_rounded_rect(plus, m.radius, theme.color(Role::Base300));
            }
            let color = if self.on_plus {
                theme.color(Role::BaseContent)
            } else {
                muted
            };
            let arm = plus.width / 5;
            let (cx, cy) = (plus.x + plus.width / 2, plus.y + plus.height / 2);
            // Thickened by repetition: `draw_line` is one pixel wide, and one
            // pixel of "+" disappears on a Retina display.
            for d in 0..(m.plus / 16).max(1) {
                canvas.draw_line(
                    Point::new(cx - arm, cy + d),
                    Point::new(cx + arm, cy + d),
                    color,
                );
                canvas.draw_line(
                    Point::new(cx + d, cy - arm),
                    Point::new(cx + d, cy + arm),
                    color,
                );
            }
        }
    }

    fn on_event(&mut self, event: &Event<'_>, ctx: &mut EventCtx<'_, M>) -> Handled {
        let Event::Input(input) = event else {
            return Handled::No;
        };
        let bounds = ctx.bounds;
        match input {
            InputEvent::PointerMoved { position } => {
                let (hovered, on_close, on_plus) = match self.hit(bounds, ctx.text, *position) {
                    Some(Hit::Tab(i, close)) => (Some(i), close, false),
                    Some(Hit::Plus) => (None, false, true),
                    None => (None, false, false),
                };
                if hovered != self.hovered
                    || on_close != self.on_close_button
                    || on_plus != self.on_plus
                {
                    self.hovered = hovered;
                    self.on_close_button = on_close;
                    self.on_plus = on_plus;
                    return Handled::Yes;
                }
                Handled::No
            }
            InputEvent::PointerLeft => {
                self.hovered = None;
                self.on_plus = false;
                Handled::Yes
            }
            InputEvent::PointerButton {
                button,
                state: ElementState::Down,
                position,
                ..
            } => {
                let Some(hit) = self.hit(bounds, ctx.text, *position) else {
                    return Handled::No;
                };
                let Hit::Tab(index, on_close) = hit else {
                    if matches!(button, PointerButton::Left) {
                        ctx.emit((self.new)());
                    }
                    return Handled::Yes;
                };
                match button {
                    // A middle click closes, as it does in every tabbed thing.
                    PointerButton::Middle => ctx.emit((self.close)(index)),
                    PointerButton::Right => ctx.emit((self.context)(index)),
                    _ if on_close => ctx.emit((self.close)(index)),
                    _ => {
                        self.selected = index;
                        ctx.emit((self.select)(index));
                    }
                }
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}

/// The close cross, drawn rather than set in type: the UI face may not have a
/// multiplication sign, and a missing glyph shows as a box.
fn cross(canvas: &mut Pen<'_>, rect: Rect, color: Color) {
    let inset = rect.width / 3;
    let (a, b) = (rect.x + inset, rect.right() - inset);
    let (t, u) = (rect.y + inset, rect.bottom() - inset);
    canvas.draw_line(Point::new(a, t), Point::new(b, u), color);
    canvas.draw_line(Point::new(a, u), Point::new(b, t), color);
}
