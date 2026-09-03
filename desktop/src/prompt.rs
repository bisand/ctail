//! A one-field modal window: "name this profile". Its own window rather than a
//! panel so the platform makes it modal and places it, which is what an
//! NSAlert with a text field does on the Mac.

use crate::theme;
use ctail_core::{resolve_palette, ConfigStore};
use denise::{DamageTracker, ElementState, Frame, InputEvent, KeyCode, Rect, Role, Size};
use denise_ui::widgets::{Align, Button, Label, TextInput};
use denise_ui::{NodeId, Ui};
use denise_winit::{DeniseApp, WindowConfig};
use std::sync::mpsc::Sender;
use std::time::Duration;

pub const SIZE: Size = Size::new(380, 150);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Msg {
    Ok,
    Cancel,
}

pub struct PromptWindow {
    ui: Ui<Msg>,
    field: NodeId,
    tx: Sender<Option<String>>,
    exit: bool,
}

impl PromptWindow {
    pub fn config(title: &str) -> WindowConfig {
        WindowConfig {
            title: title.into(),
            size: SIZE,
            resizable: false,
            frame_interval: Duration::from_nanos(1_000_000_000 / 60),
        }
    }

    pub fn new(
        size: Size,
        scale: f32,
        caption: String,
        initial: String,
        tx: Sender<Option<String>>,
    ) -> Self {
        let config = ConfigStore::new(None);
        let settings = config.load_settings();
        let palette = resolve_palette(
            &settings.theme,
            &settings.theme_mode,
            Some(config.themes_dir()),
        );
        let theme =
            theme::from_palette(&settings.theme, &settings.theme_mode, &palette).scaled(scale);
        let mut ui: Ui<Msg> = Ui::new(size, theme);
        if let Some((_, source)) = crate::fonts::load(crate::fonts::UI) {
            let id = ui.add_font(source);
            ui.set_default_font(id);
        }
        let s = |v: i32| (v as f32 * scale + 0.5) as i32;
        let px = |v: f32| (v * scale + 0.5) as u16;
        let root = ui.root();
        ui.add(
            root,
            Label::new(caption)
                .with_size(px(13.0))
                .with_align(Align::Start, Align::Center),
            Rect::new(s(20), s(18), s(340), s(22)),
        );
        let field = ui
            .add(
                root,
                TextInput::new().with_size(px(13.0)).with_submit(Msg::Ok),
                Rect::new(s(20), s(48), s(340), s(30)),
            )
            .expect("field");
        if let Some(f) = ui.widget_mut::<TextInput<Msg>>(field) {
            f.set_text(initial);
        }
        ui.add(
            root,
            Button::new("Cancel", Msg::Cancel)
                .with_role(Role::Neutral)
                .with_size(px(13.0)),
            Rect::new(s(148), s(100), s(100), s(30)),
        );
        ui.add(
            root,
            Button::new("OK", Msg::Ok)
                .with_role(Role::Primary)
                .with_size(px(13.0)),
            Rect::new(s(260), s(100), s(100), s(30)),
        );
        ui.focus(Some(field));
        Self {
            ui,
            field,
            tx,
            exit: false,
        }
    }

    fn close(&mut self, accept: bool) {
        let value = accept
            .then(|| {
                self.ui
                    .widget::<TextInput<Msg>>(self.field)
                    .map(|f| f.text().trim().to_string())
            })
            .flatten()
            .filter(|s| !s.is_empty());
        let _ = self.tx.send(value);
        self.exit = true;
    }
}

impl DeniseApp for PromptWindow {
    fn update(&mut self, events: &[InputEvent], damage: &mut DamageTracker) {
        for event in events {
            match event {
                InputEvent::CloseRequested => self.close(false),
                InputEvent::Key {
                    code: KeyCode::Escape,
                    state: ElementState::Down,
                    ..
                } => self.close(false),
                _ => {}
            }
        }
        self.ui.handle(events);
        self.ui.tick(0);
        let messages: Vec<Msg> = self.ui.drain_messages().collect();
        for msg in messages {
            match msg {
                Msg::Ok => self.close(true),
                Msg::Cancel => self.close(false),
            }
        }
        if self.ui.needs_paint() {
            let pending = self.ui.pending_damage();
            if pending.is_empty() {
                damage.add_full();
            } else {
                for rect in pending {
                    damage.add(*rect);
                }
            }
        }
    }

    fn render(&mut self, frame: &mut Frame<'_>, _damage: &[Rect]) {
        self.ui.paint(frame);
        self.ui.presented();
    }

    fn exit_requested(&self) -> bool {
        self.exit
    }

    fn next_frame_in(&self) -> Option<Duration> {
        self.ui.next_wake_ms().map(Duration::from_millis)
    }
}
