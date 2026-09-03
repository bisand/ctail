//! The tab strip.
//!
//! Its own widget rather than the toolkit's `Tabs`, because a ctail tab is more
//! than a label: it carries a user colour, a close button, and a right-click
//! that has to report *which* tab was hit. Widths are computed the same way in
//! painting and in hit testing, so a click always lands on what was drawn.

use denise::{Color, ElementState, InputEvent, Point, PointerButton, Radius, Rect, Role};
use denise_render::Canvas;
use denise_text::{TextEngine, TextStyle};
use denise_ui::widget::{Event, EventCtx, Handled, PaintCtx, Widget};

const MIN_W: i32 = 110;
const MAX_W: i32 = 240;
const PAD: i32 = 10;
const CLOSE_W: i32 = 18;

/// One tab, as the strip needs to draw it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TabItem {
    /// What to show: the user's label, or the file name.
    pub label: String,
    /// Hex colour, or empty for none.
    pub color: String,
}

pub struct TabBar<M> {
    items: Vec<TabItem>,
    selected: usize,
    hovered: Option<usize>,
    /// Whether the pointer is over the hovered tab's close button.
    on_close_button: bool,
    select: fn(usize) -> M,
    close: fn(usize) -> M,
    context: fn(usize) -> M,
    style: TextStyle,
}

impl<M: 'static> TabBar<M> {
    pub fn new(select: fn(usize) -> M, close: fn(usize) -> M, context: fn(usize) -> M) -> Self {
        Self {
            items: Vec::new(),
            selected: 0,
            hovered: None,
            on_close_button: false,
            select,
            close,
            context,
            style: TextStyle::built_in(14),
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

    fn layout(&self, bounds: Rect, text: &mut TextEngine) -> Vec<Rect> {
        layout(&self.items, bounds, self.style, text)
    }

    fn close_rect(tab: Rect) -> Rect {
        Rect::new(
            tab.right() - PAD - CLOSE_W,
            tab.y + (tab.height - CLOSE_W) / 2,
            CLOSE_W,
            CLOSE_W,
        )
    }

    fn hit(&self, bounds: Rect, text: &mut TextEngine, p: Point) -> Option<(usize, bool)> {
        self.layout(bounds, text)
            .into_iter()
            .enumerate()
            .find(|(_, r)| r.contains(p))
            .map(|(i, r)| (i, Self::close_rect(r).contains(p)))
    }
}

/// Tab rectangles left to right. A free function so painting, hit testing and
/// a caller anchoring a menu can never disagree about where a tab is — and so
/// the last of those can measure text without holding a borrow of the widget.
pub fn layout(
    items: &[TabItem],
    bounds: Rect,
    style: TextStyle,
    text: &mut TextEngine,
) -> Vec<Rect> {
    let mut out = Vec::with_capacity(items.len());
    let mut x = bounds.x;
    for item in items {
        let label = text.measure_line(style, &item.label);
        let width = (label + PAD * 2 + CLOSE_W).clamp(MIN_W, MAX_W);
        out.push(Rect::new(x, bounds.y, width, bounds.height));
        x += width;
    }
    out
}

impl<M: 'static> Widget<M> for TabBar<M> {
    fn accepts_pointer(&self) -> bool {
        true
    }

    fn paint(&self, ctx: &mut PaintCtx<'_>, canvas: &mut Canvas<'_>) {
        let theme = ctx.theme;
        let bounds = ctx.bounds;
        canvas.fill_rect(bounds, theme.color(Role::Base200));
        let rects = self.layout(bounds, ctx.text);
        let metrics = ctx.text.metrics(self.style);
        let baseline = bounds.y + (bounds.height - metrics.line_height()) / 2 + metrics.ascent;
        let radius = theme.radius(Radius::Selector);

        for (i, tab) in rects.iter().enumerate() {
            let active = i == self.selected;
            let hovered = self.hovered == Some(i);
            let mut pen = canvas.with_clip(*tab);
            if active {
                pen.fill_rect(*tab, theme.color(Role::Base100));
            } else if hovered {
                pen.fill_rect(*tab, theme.color(Role::Base300));
            }
            // The user's colour is a stripe along the top, so it reads even
            // when the tab is not the active one.
            let item = &self.items[i];
            if !item.color.is_empty() {
                pen.fill_rect(
                    Rect::new(tab.x, tab.y, tab.width, 3),
                    crate::theme::hex(&item.color),
                );
            }
            if active {
                pen.fill_rect(
                    Rect::new(tab.x, tab.bottom() - 2, tab.width, 2),
                    theme.color(Role::Primary),
                );
            }
            let fg = if active {
                theme.color(Role::BaseContent)
            } else {
                theme
                    .color(Role::BaseContent)
                    .mix(theme.color(Role::Base200), 90)
            };
            let text_w = tab.width - PAD * 2 - CLOSE_W;
            let mut label = pen.with_clip(Rect::new(tab.x + PAD, tab.y, text_w, tab.height));
            ctx.text.draw_line(
                &mut label,
                self.style,
                Point::new(tab.x + PAD, baseline),
                &item.label,
                fg,
            );
            // The close cross appears on the active tab and under the pointer,
            // which is where a click can plausibly be aimed.
            if active || hovered {
                let close = Self::close_rect(*tab);
                if self.on_close_button && hovered {
                    pen.fill_rounded_rect(close, radius, theme.color(Role::Base300));
                }
                cross(&mut pen, close, fg);
            }
            // Separator, except after the active tab where it would cut the
            // highlight in half.
            if !active && i + 1 < rects.len() {
                pen.fill_rect(
                    Rect::new(tab.right() - 1, tab.y + 6, 1, tab.height - 12),
                    theme.color(Role::Base300),
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
                let hit = self.hit(bounds, ctx.text, *position);
                let (hovered, on_close) = match hit {
                    Some((i, close)) => (Some(i), close),
                    None => (None, false),
                };
                if hovered != self.hovered || on_close != self.on_close_button {
                    self.hovered = hovered;
                    self.on_close_button = on_close;
                    return Handled::Yes;
                }
                Handled::No
            }
            InputEvent::PointerLeft => {
                self.hovered = None;
                Handled::Yes
            }
            InputEvent::PointerButton {
                button,
                state: ElementState::Down,
                position,
                ..
            } => {
                let Some((index, on_close)) = self.hit(bounds, ctx.text, *position) else {
                    return Handled::No;
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
fn cross(canvas: &mut Canvas<'_>, rect: Rect, color: Color) {
    let inset = rect.width / 3;
    let (a, b) = (rect.x + inset, rect.right() - inset);
    let (t, u) = (rect.y + inset, rect.bottom() - inset);
    canvas.draw_line(Point::new(a, t), Point::new(b, u), color);
    canvas.draw_line(Point::new(a, u), Point::new(b, t), color);
}
