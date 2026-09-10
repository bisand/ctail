//! The strip along the bottom of the window: what the active tab is showing,
//! what the process costs, and whether the view is following the tail.
//!
//! One widget rather than a label and a toolkit `Checkbox` side by side,
//! because the macOS app's bar is a single 26pt band with three things
//! measured against each other — the memory figure sits a fixed gap from the
//! Follow box, whose width depends on its label — and because the toolkit's
//! checkbox is sized for a form (a 20pt selector), which beside 11pt text
//! reads as a button rather than a status indicator.

use denise::{Color, ElementState, InputEvent, Pen, Point, PointerButton, Rect, Role};
use denise_text::{TextEngine, TextStyle};
use denise_ui::widget::{Event, EventCtx, Handled, PaintCtx, Widget};

/// The bar's proportions, in logical pixels, taken from the macOS window.
const PAD: f32 = 10.0;
/// Between the memory figure and the Follow box.
const GROUP_GAP: f32 = 14.0;
/// Edge of the tick box.
const BOX: f32 = 13.0;
/// Between the tick box and its label.
const BOX_GAP: f32 = 5.0;

pub struct StatusBar<M> {
    /// What the active tab is: "name · 690 lines", or an error.
    text: String,
    /// The process's footprint, already formatted; empty hides it.
    memory: String,
    following: bool,
    /// A window with no tab has nothing to follow, so the box goes away.
    show_follow: bool,
    hovered: bool,
    toggle: fn(bool) -> M,
    /// The status and memory figures, in the log's face: they are numbers read
    /// at a glance, and they change under the eye.
    mono: TextStyle,
    /// The Follow label, in the UI face.
    label: TextStyle,
    scale: f32,
}

impl<M: 'static> StatusBar<M> {
    pub fn new(toggle: fn(bool) -> M, mono: TextStyle, label: TextStyle, scale: f32) -> Self {
        Self {
            text: String::new(),
            memory: String::new(),
            following: true,
            show_follow: false,
            hovered: false,
            toggle,
            mono,
            label,
            scale,
        }
    }

    /// Sets the left-hand text. Reports whether it changed, so a caller that
    /// runs every frame can skip the repaint when it did not.
    pub fn set_text(&mut self, text: String) -> bool {
        let changed = self.text != text;
        self.text = text;
        changed
    }

    pub fn set_memory(&mut self, memory: String) -> bool {
        let changed = self.memory != memory;
        self.memory = memory;
        changed
    }

    pub fn set_following(&mut self, following: bool) -> bool {
        let changed = self.following != following;
        self.following = following;
        changed
    }

    pub fn set_show_follow(&mut self, show: bool) -> bool {
        let changed = self.show_follow != show;
        self.show_follow = show;
        changed
    }

    fn px(&self, v: f32) -> i32 {
        (v * self.scale + 0.5) as i32
    }

    /// The Follow control: tick box and label together, at the trailing edge.
    fn follow_rect(&self, bounds: Rect, text: &mut TextEngine) -> Option<Rect> {
        if !self.show_follow {
            return None;
        }
        let side = self.px(BOX);
        let width = side + self.px(BOX_GAP) + text.measure_line(self.label, "Follow");
        let height = side.max(text.metrics(self.label).line_height());
        Some(Rect::new(
            bounds.right() - self.px(PAD) - width,
            bounds.y + (bounds.height - height) / 2,
            width,
            height,
        ))
    }

    fn box_rect(&self, follow: Rect) -> Rect {
        let side = self.px(BOX);
        Rect::new(follow.x, follow.y + (follow.height - side) / 2, side, side)
    }
}

impl<M: 'static> Widget<M> for StatusBar<M> {
    fn accepts_pointer(&self) -> bool {
        true
    }

    fn paint(&self, ctx: &mut PaintCtx<'_>, canvas: &mut Pen<'_>) {
        let bounds = ctx.bounds;
        let theme = ctx.theme;
        canvas.fill_rect(bounds, theme.color(Role::Base200));
        let fg = theme.color(Role::BaseContent);
        let muted = fg.mix(theme.color(Role::Base200), 96);

        let follow = self.follow_rect(bounds, ctx.text);
        // Everything to the left of the Follow box, or of the trailing edge
        // when there is none.
        let right = follow.map_or(bounds.right() - self.px(PAD), |r| r.x - self.px(GROUP_GAP));

        let metrics = ctx.text.metrics(self.mono);
        let baseline = bounds.y + (bounds.height - metrics.line_height()) / 2 + metrics.ascent;
        let mem_w = ctx.text.measure_line(self.mono, &self.memory);
        let mem_x = right - mem_w;

        // The status text is clipped rather than shortened: a name long enough
        // to reach the memory figure is still readable up to where it stops.
        let text_x = bounds.x + self.px(PAD);
        let gap = self.px(GROUP_GAP);
        {
            let mut pen = canvas.with_clip(Rect::new(
                text_x,
                bounds.y,
                (mem_x - gap - text_x).max(0),
                bounds.height,
            ));
            ctx.text.draw_line(
                &mut pen,
                self.mono,
                Point::new(text_x, baseline),
                &self.text,
                fg,
            );
        }
        if !self.memory.is_empty() {
            ctx.text.draw_line(
                canvas,
                self.mono,
                Point::new(mem_x, baseline),
                &self.memory,
                muted,
            );
        }

        let Some(follow) = follow else { return };
        let radius = self.px(3.0);
        let tick_box = self.box_rect(follow);
        if self.following {
            // Filled in the accent colour while it is following, which is what
            // the macOS checkbox shows: the state is read at a glance rather
            // than by squinting at a tick.
            canvas.fill_rounded_rect(tick_box, radius, theme.color(Role::Primary));
            tick(
                canvas,
                tick_box,
                theme.color(Role::PrimaryContent),
                self.scale,
            );
        } else {
            canvas.fill_rounded_rect(tick_box, radius, theme.color(Role::Base100));
            let edge = if self.hovered {
                fg.mix(theme.color(Role::Base200), 128)
            } else {
                theme.color(Role::Base300)
            };
            canvas.stroke_rounded_rect(tick_box, radius, self.px(1.0).max(1), edge);
        }
        let label = ctx.text.metrics(self.label);
        let label_baseline = follow.y + (follow.height - label.line_height()) / 2 + label.ascent;
        ctx.text.draw_line(
            canvas,
            self.label,
            Point::new(tick_box.right() + self.px(BOX_GAP), label_baseline),
            "Follow",
            fg,
        );
    }

    fn on_event(&mut self, event: &Event<'_>, ctx: &mut EventCtx<'_, M>) -> Handled {
        let Event::Input(input) = event else {
            return Handled::No;
        };
        let bounds = ctx.bounds;
        match input {
            InputEvent::PointerMoved { position } => {
                let over = self
                    .follow_rect(bounds, ctx.text)
                    .is_some_and(|r| r.contains(*position));
                if over != self.hovered {
                    self.hovered = over;
                    return Handled::Yes;
                }
                Handled::No
            }
            InputEvent::PointerLeft => {
                self.hovered = false;
                Handled::Yes
            }
            InputEvent::PointerButton {
                button: PointerButton::Left,
                state: ElementState::Down,
                position,
                ..
            } => {
                // The label is part of the target: a 13-pixel box on its own is
                // a smaller thing to hit than anything else in this window.
                let Some(rect) = self.follow_rect(bounds, ctx.text) else {
                    return Handled::No;
                };
                if !rect.contains(*position) {
                    return Handled::No;
                }
                self.following = !self.following;
                ctx.emit((self.toggle)(self.following));
                Handled::Yes
            }
            _ => Handled::No,
        }
    }
}

/// A checkmark inside `area`. Drawn rather than set in type, for the reason the
/// tab strip's close cross is: the UI face may not carry the glyph.
fn tick(canvas: &mut Pen<'_>, area: Rect, color: Color, scale: f32) {
    let (w, h) = (area.width, area.height);
    let (ax, ay) = (area.x + w / 4, area.y + h / 2);
    let (bx, by) = (area.x + w * 7 / 16, area.y + h * 11 / 16);
    let (cx, cy) = (area.x + w * 3 / 4, area.y + h * 5 / 16);
    // `draw_line` is a hairline, so the weight comes from repeating it a
    // pixel apart, the way the toolkit's own checkbox draws its tick.
    let weight = (scale + 0.5) as i32 + 1;
    for d in 0..weight.max(2) {
        let dy = d - weight / 2;
        canvas.draw_line(Point::new(ax, ay + dy), Point::new(bx, by + dy), color);
        canvas.draw_line(Point::new(bx, by + dy), Point::new(cx, cy + dy), color);
    }
}
