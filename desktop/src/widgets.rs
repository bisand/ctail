//! The two widgets the rules editor needs and the toolkit does not have,
//! because both are about *arbitrary* colours: Denise's palette is semantic
//! roles, and a highlighting rule is a hex value the user chose.

use ctail_core::{Highlighter, Rule};
use denise::{Color, ElementState, InputEvent, PointerButton, Radius, Rect, Role};
use denise_render::Canvas;
use denise_text::TextStyle;
use denise_ui::widget::{Event, EventCtx, Handled, PaintCtx, Widget};

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
