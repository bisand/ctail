//! The widgets this app needs and the toolkit does not have. Two are about
//! *arbitrary* colours — Denise's palette is semantic roles, and a
//! highlighting rule is a hex value the user chose — and one is a block of
//! wrapped text long enough to need scrolling, which is what an answer from a
//! model is.

use ctail_core::{Highlighter, Rule};
use denise::{Color, ElementState, InputEvent, Point, PointerButton, Radius, Rect, Role};
use denise_render::Canvas;
use denise_text::TextStyle;
use denise_ui::widget::{Event, EventCtx, Handled, PaintCtx, Widget};
use std::cell::Cell;

/// A clickable square of one colour: the palette a rule's foreground and
/// background are picked from.
pub struct Swatch<M> {
    color: Color,
    message: M,
    /// Drawn with a ring when it is the rule's current colour.
    selected: bool,
}

impl<M: Clone + 'static> Swatch<M> {
    pub fn new(hex: &str, message: M) -> Self {
        Self {
            color: crate::theme::hex(hex),
            message,
            selected: false,
        }
    }

    pub fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }
}

impl<M: Clone + 'static> Widget<M> for Swatch<M> {
    fn accepts_pointer(&self) -> bool {
        true
    }

    fn paint(&self, ctx: &mut PaintCtx<'_>, canvas: &mut Canvas<'_>) {
        let r = ctx.theme.radius(Radius::Selector);
        canvas.fill_rounded_rect(ctx.bounds, r, self.color);
        if self.selected {
            canvas.stroke_rounded_rect(ctx.bounds, r, 2, ctx.theme.color(Role::BaseContent));
        } else {
            canvas.stroke_rounded_rect(ctx.bounds, r, 1, ctx.theme.color(Role::Base300));
        }
    }

    fn on_event(&mut self, event: &Event<'_>, ctx: &mut EventCtx<'_, M>) -> Handled {
        if let Event::Input(InputEvent::PointerButton {
            button: PointerButton::Left,
            state: ElementState::Down,
            ..
        }) = event
        {
            ctx.emit(self.message.clone());
            return Handled::Yes;
        }
        Handled::No
    }
}

/// A sample log line with one rule applied, so the effect of an edit is
/// visible before it is saved. Uses the engine's highlighter, so what is shown
/// here is what the log will do.
pub struct Preview {
    sample: String,
    rule: Rule,
    /// Compiled once per edit rather than once per paint: a regex compile is
    /// not something to do at frame rate.
    highlighter: Highlighter,
    style: TextStyle,
}

impl Preview {
    pub fn new(sample: impl Into<String>, style: TextStyle) -> Self {
        let rule = Rule::default();
        Self {
            sample: sample.into(),
            highlighter: Highlighter::new(std::slice::from_ref(&rule)),
            rule,
            style,
        }
    }

    pub fn set_rule(&mut self, rule: Rule) {
        if rule != self.rule {
            self.highlighter = Highlighter::new(std::slice::from_ref(&rule));
            self.rule = rule;
        }
    }
}

impl<M: 'static> Widget<M> for Preview {
    fn paint(&self, ctx: &mut PaintCtx<'_>, canvas: &mut Canvas<'_>) {
        let bounds = ctx.bounds;
        canvas.fill_rect(bounds, ctx.theme.color(Role::Base100));
        let fg =
            (!self.rule.foreground.is_empty()).then(|| crate::theme::hex(&self.rule.foreground));
        let bg =
            (!self.rule.background.is_empty()).then(|| crate::theme::hex(&self.rule.background));
        let base = ctx.theme.color(Role::BaseContent);
        let metrics = ctx.text.metrics(self.style);
        let baseline = bounds.y + (bounds.height - metrics.line_height()) / 2 + metrics.ascent;
        let x = bounds.x + 6;

        let styled = self.highlighter.apply(&self.sample);
        if styled.line_rule >= 0 {
            if let Some(bg) = bg {
                canvas.fill_rect(bounds, bg);
            }
            ctx.text.draw_line(
                canvas,
                self.style,
                denise::Point::new(x, baseline),
                &self.sample,
                fg.unwrap_or(base),
            );
            return;
        }
        // Match-level: paint the whole line plainly, then the matched spans.
        ctx.text.draw_line(
            canvas,
            self.style,
            denise::Point::new(x, baseline),
            &self.sample,
            base,
        );
        for span in &styled.spans {
            let (a, b) = (span.start as usize, span.end as usize);
            let Some(text) = self.sample.get(a..b) else {
                continue;
            };
            let prefix = ctx.text.measure_line(self.style, &self.sample[..a]);
            let width = ctx.text.measure_line(self.style, text);
            let rect = Rect::new(x + prefix, bounds.y, width, bounds.height);
            if let Some(bg) = bg {
                canvas.fill_rect(rect, bg);
            }
            ctx.text.draw_line(
                canvas,
                self.style,
                denise::Point::new(x + prefix, baseline),
                text,
                fg.unwrap_or(base),
            );
        }
    }
}

/// Read-only text wrapped to the width and scrolled with the wheel: the
/// assistant's answer. Wrapping is the toolkit's own, done at paint time
/// because only paint knows the width.
pub struct TextBlock {
    text: String,
    style: TextStyle,
    /// First wrapped line shown.
    scroll: Cell<usize>,
    /// Wrapped lines in total and lines that fit, learnt while painting, so
    /// scrolling knows where to stop.
    lines: Cell<usize>,
    visible: Cell<usize>,
}

impl TextBlock {
    pub fn new(style: TextStyle) -> Self {
        Self {
            text: String::new(),
            style,
            scroll: Cell::new(0),
            lines: Cell::new(0),
            visible: Cell::new(1),
        }
    }

    pub fn set_text(&mut self, text: &str) {
        self.text = text.to_string();
        self.scroll.set(0);
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    fn max_scroll(&self) -> usize {
        self.lines.get().saturating_sub(self.visible.get())
    }
}

const TEXT_PAD: i32 = 8;

impl<M: 'static> Widget<M> for TextBlock {
    fn accepts_pointer(&self) -> bool {
        true
    }

    fn paint(&self, ctx: &mut PaintCtx<'_>, canvas: &mut Canvas<'_>) {
        let bounds = ctx.bounds;
        let theme = ctx.theme;
        let r = theme.radius(Radius::Box);
        canvas.fill_rounded_rect(bounds, r, theme.color(Role::Base100));
        canvas.stroke_rounded_rect(bounds, r, 1, theme.color(Role::Base300));
        let inner = Rect::new(
            bounds.x + TEXT_PAD,
            bounds.y + TEXT_PAD,
            (bounds.width - 2 * TEXT_PAD).max(1),
            (bounds.height - 2 * TEXT_PAD).max(1),
        );
        let metrics = ctx.text.metrics(self.style);
        let line_h = metrics.line_height().max(1);
        let wrapped = ctx.text.wrap(self.style, &self.text, inner.width);
        let visible = (inner.height / line_h).max(1) as usize;
        self.lines.set(wrapped.len());
        self.visible.set(visible);
        let scroll = self.scroll.get().min(self.max_scroll());
        self.scroll.set(scroll);
        let fg = theme.color(Role::BaseContent);
        let mut pen = canvas.with_clip(inner);
        for (i, line) in wrapped.iter().skip(scroll).take(visible + 1).enumerate() {
            let baseline = inner.y + i as i32 * line_h + metrics.ascent;
            ctx.text.draw_line(
                &mut pen,
                self.style,
                Point::new(inner.x, baseline),
                line,
                fg,
            );
        }
    }

    fn on_event(&mut self, event: &Event<'_>, _ctx: &mut EventCtx<'_, M>) -> Handled {
        let Event::Input(InputEvent::PointerScroll { delta_y, .. }) = event else {
            return Handled::No;
        };
        // Lines from a mouse wheel, pixels from a trackpad; three lines per
        // wheel notch is the usual feel.
        let lines = if delta_y.abs() > 20.0 {
            *delta_y / 16.0
        } else {
            *delta_y * 3.0
        };
        let delta = lines.round() as i64;
        let next = (self.scroll.get() as i64 + delta).clamp(0, self.max_scroll() as i64);
        self.scroll.set(next as usize);
        Handled::Yes
    }
}
