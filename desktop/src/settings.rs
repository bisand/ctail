//! The Settings window.
//!
//! A window of its own rather than a panel inside the main one, so it gets the
//! platform's title bar, close button, placement and window management — the
//! same shape the macOS app's settings window has. What is *inside* it is drawn
//! with the same widgets and theme as everything else.
//!
//! It writes nothing on its own: pressing Save sends the edited settings back
//! to the main window, which persists them and applies what it can live.

use crate::theme;
use ctail_core::{all_themes, resolve_palette, AppSettings, ConfigStore};
use denise::{DamageTracker, ElementState, Frame, InputEvent, KeyCode, Rect, Role, Size};
use denise_text::TextStyle;
use denise_ui::widgets::{Align, Button, Checkbox, Divider, Label, Select, TextInput};
use denise_ui::{Anchors, NodeId, Ui};
use denise_winit::{DeniseApp, WindowConfig};
use std::sync::mpsc::Sender;
use std::time::Duration;

/// Logical size; the window is not resizable, so this is the whole form.
pub const SIZE: Size = Size::new(460, 600);

/// Which dropdown a message is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Theme,
    Mode,
    NewTabPosition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Msg {
    OpenList(Field),
    Chose(usize),
    LineNumbers(bool),
    WordWrap(bool),
    RestoreTabs(bool),
    DisableUpdates(bool),
    Save,
    Cancel,
}

/// A labelled row: caption on the left, control on the right.
struct Form {
    y: i32,
    scale: f32,
}

impl Form {
    fn row(&mut self, height: i32) -> (Rect, Rect) {
        let s = |v: i32| (v as f32 * self.scale + 0.5) as i32;
        let label = Rect::new(s(20), s(self.y), s(170), s(height));
        let control = Rect::new(s(196), s(self.y), s(224), s(height));
        self.y += height + 10;
        (label, control)
    }

    fn gap(&mut self, height: i32) -> Rect {
        let s = |v: i32| (v as f32 * self.scale + 0.5) as i32;
        let r = Rect::new(s(20), s(self.y), s(400), s(height));
        self.y += height + 10;
        r
    }
}

pub struct SettingsWindow {
    ui: Ui<Msg>,
    settings: AppSettings,
    themes: Vec<String>,
    /// Which list is open, so a chosen row goes to the right control.
    open_list: Option<Field>,
    theme_sel: NodeId,
    mode_sel: NodeId,
    new_tab_sel: NodeId,
    font: NodeId,
    poll: NodeId,
    buffer: NodeId,
    scrollback: NodeId,
    timeout: NodeId,
    update_hours: NodeId,
    tx: Sender<Option<AppSettings>>,
    exit: bool,
}

impl SettingsWindow {
    pub fn config() -> WindowConfig {
        WindowConfig {
            title: "ctail — Settings".into(),
            size: SIZE,
            resizable: false,
            frame_interval: Duration::from_nanos(1_000_000_000 / 60),
        }
    }

    pub fn new(size: Size, scale: f32, tx: Sender<Option<AppSettings>>) -> Self {
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
        let px = |v: f32| (v * scale + 0.5) as u16;
        let s = |v: i32| (v as f32 * scale + 0.5) as i32;
        let root = ui.root();
        let body = px(13.0);

        let themes: Vec<String> = all_themes(Some(config.themes_dir()))
            .into_iter()
            .map(|t| t.display_name)
            .collect();
        let theme_names: Vec<String> = all_themes(Some(config.themes_dir()))
            .into_iter()
            .map(|t| t.name)
            .collect();

        let mut form = Form { y: 18, scale };
        let label = |ui: &mut Ui<Msg>, rect: Rect, text: &str| {
            ui.add(
                root,
                Label::new(text)
                    .with_size(body)
                    .with_align(Align::Start, Align::Center),
                rect,
            );
        };

        // --- appearance ---
        let (l, c) = form.row(28);
        label(&mut ui, l, "Theme");
        let theme_sel = ui
            .add(
                root,
                Select::new(themes.clone(), Msg::OpenList(Field::Theme))
                    .with_selected(theme_names.iter().position(|n| *n == settings.theme))
                    .with_style(TextStyle::built_in(body)),
                c,
            )
            .expect("theme");
        let (l, c) = form.row(28);
        label(&mut ui, l, "Mode");
        let mode_sel = ui
            .add(
                root,
                Select::new(["dark", "light"], Msg::OpenList(Field::Mode))
                    .with_selected(Some(usize::from(settings.theme_mode == "light")))
                    .with_style(TextStyle::built_in(body)),
                c,
            )
            .expect("mode");
        let (l, c) = form.row(28);
        label(&mut ui, l, "Font size");
        let font = number(&mut ui, root, c, settings.font_size, body);
        let (l, c) = form.row(24);
        label(&mut ui, l, "Show line numbers");
        let line_numbers = ui
            .add(
                root,
                Checkbox::new("", Msg::LineNumbers)
                    .with_checked(settings.show_line_numbers)
                    .with_size(body),
                c,
            )
            .expect("line numbers");
        let (l, c) = form.row(24);
        label(&mut ui, l, "Word wrap");
        let word_wrap = ui
            .add(
                root,
                Checkbox::new("", Msg::WordWrap)
                    .with_checked(settings.word_wrap)
                    .with_size(body),
                c,
            )
            .expect("word wrap");
        let _ = (line_numbers, word_wrap);
        ui.add(root, Divider::new(), form.gap(8));

        // --- reading ---
        let (l, c) = form.row(28);
        label(&mut ui, l, "Poll interval (ms)");
        let poll = number(&mut ui, root, c, settings.poll_interval_ms, body);
        let (l, c) = form.row(28);
        label(&mut ui, l, "Buffer size (lines)");
        let buffer = number(&mut ui, root, c, settings.buffer_size, body);
        let (l, c) = form.row(28);
        label(&mut ui, l, "Scrollback (lines)");
        let scrollback = number(&mut ui, root, c, settings.scroll_buffer, body);
        let (l, c) = form.row(28);
        label(&mut ui, l, "Read timeout (s)");
        let timeout = number(&mut ui, root, c, settings.read_timeout_sec, body);
        ui.add(root, Divider::new(), form.gap(8));

        // --- session ---
        let (l, c) = form.row(24);
        label(&mut ui, l, "Restore tabs on launch");
        ui.add(
            root,
            Checkbox::new("", Msg::RestoreTabs)
                .with_checked(settings.restore_tabs)
                .with_size(body),
            c,
        );
        let (l, c) = form.row(28);
        label(&mut ui, l, "New tab position");
        let new_tab_sel = ui
            .add(
                root,
                Select::new(["end", "afterActive"], Msg::OpenList(Field::NewTabPosition))
                    .with_selected(Some(usize::from(
                        settings.new_tab_position == "afterActive",
                    )))
                    .with_style(TextStyle::built_in(body)),
                c,
            )
            .expect("new tab");
        let (l, c) = form.row(24);
        label(&mut ui, l, "Disable update check");
        ui.add(
            root,
            Checkbox::new("", Msg::DisableUpdates)
                .with_checked(settings.disable_update_check)
                .with_size(body),
            c,
        );
        let (l, c) = form.row(28);
        label(&mut ui, l, "Update check (hours)");
        let update_hours = number(&mut ui, root, c, settings.update_check_interval_hours, body);

        // --- buttons, pinned to the bottom right ---
        let h = size.height as i32;
        let save = ui
            .add(
                root,
                Button::new("Save", Msg::Save)
                    .with_role(Role::Primary)
                    .with_size(body),
                Rect::new(s(320), h - s(46), s(100), s(30)),
            )
            .expect("save");
        let cancel = ui
            .add(
                root,
                Button::new("Cancel", Msg::Cancel)
                    .with_role(Role::Neutral)
                    .with_size(body),
                Rect::new(s(208), h - s(46), s(100), s(30)),
            )
            .expect("cancel");
        for id in [save, cancel] {
            ui.set_anchors(
                id,
                Anchors {
                    left: true,
                    top: false,
                    right: false,
                    bottom: true,
                },
            );
        }

        Self {
            ui,
            settings,
            themes: theme_names,
            open_list: None,
            theme_sel,
            mode_sel,
            new_tab_sel,
            font,
            poll,
            buffer,
            scrollback,
            timeout,
            update_hours,
            tx,
            exit: false,
        }
    }

    /// Every way out goes through here, so the window that opened this one
    /// always learns it is gone — otherwise its menu item would stay disabled
    /// for the rest of the run.
    fn close(&mut self, save: bool) {
        let _ = self.tx.send(save.then(|| self.collect()));
        self.exit = true;
    }

    /// Reads every control back into a settings record.
    fn collect(&self) -> AppSettings {
        let mut s = self.settings.clone();
        let num = |id: NodeId, fallback: i32, lo: i32, hi: i32| -> i32 {
            self.ui
                .widget::<TextInput<Msg>>(id)
                .and_then(|f| f.text().trim().parse::<i32>().ok())
                .unwrap_or(fallback)
                .clamp(lo, hi)
        };
        s.font_size = num(self.font, s.font_size, 6, 48);
        s.poll_interval_ms = num(self.poll, s.poll_interval_ms, 50, 60_000);
        s.buffer_size = num(self.buffer, s.buffer_size, 100, 10_000_000);
        s.scroll_buffer = num(self.scrollback, s.scroll_buffer, 0, 1_000_000);
        s.read_timeout_sec = num(self.timeout, s.read_timeout_sec, 1, 600);
        s.update_check_interval_hours =
            num(self.update_hours, s.update_check_interval_hours, 1, 720);
        s
    }

    fn apply_choice(&mut self, index: usize) {
        let Some(field) = self.open_list.take() else {
            return;
        };
        match field {
            Field::Theme => {
                if let Some(name) = self.themes.get(index) {
                    self.settings.theme = name.clone();
                }
                set_selected(&mut self.ui, self.theme_sel, index);
            }
            Field::Mode => {
                self.settings.theme_mode = if index == 1 { "light" } else { "dark" }.into();
                set_selected(&mut self.ui, self.mode_sel, index);
            }
            Field::NewTabPosition => {
                self.settings.new_tab_position =
                    if index == 1 { "afterActive" } else { "end" }.into();
                set_selected(&mut self.ui, self.new_tab_sel, index);
            }
        }
        self.ui.close_popup();
        self.retheme();
    }

    /// The window shows the theme it is editing, so a choice is visible at once.
    fn retheme(&mut self) {
        let config = ConfigStore::new(None);
        let palette = resolve_palette(
            &self.settings.theme,
            &self.settings.theme_mode,
            Some(config.themes_dir()),
        );
        // Keeping the metrics means keeping the display scale the window was
        // built at; only the colours change.
        let metrics = self.ui.theme().metrics;
        let mut theme =
            theme::from_palette(&self.settings.theme, &self.settings.theme_mode, &palette);
        theme.metrics = metrics;
        self.ui.set_theme(theme);
    }
}

fn number(ui: &mut Ui<Msg>, root: NodeId, rect: Rect, value: i32, size: u16) -> NodeId {
    let id = ui
        .add(
            root,
            TextInput::new()
                .with_max_chars(9)
                .with_size(size)
                .with_submit(Msg::Save),
            rect,
        )
        .expect("number field");
    if let Some(field) = ui.widget_mut::<TextInput<Msg>>(id) {
        field.set_text(value.to_string());
    }
    id
}

fn set_selected(ui: &mut Ui<Msg>, id: NodeId, index: usize) {
    if let Some(sel) = ui.widget_mut::<Select<Msg>>(id) {
        sel.set_selected(Some(index));
    }
    ui.invalidate(id);
}

impl DeniseApp for SettingsWindow {
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
                Msg::OpenList(field) => {
                    let id = match field {
                        Field::Theme => self.theme_sel,
                        Field::Mode => self.mode_sel,
                        Field::NewTabPosition => self.new_tab_sel,
                    };
                    self.open_list = Some(field);
                    denise_ui::widgets::open_select(&mut self.ui, id, Msg::Chose);
                }
                Msg::Chose(index) => self.apply_choice(index),
                Msg::LineNumbers(on) => self.settings.show_line_numbers = on,
                Msg::WordWrap(on) => self.settings.word_wrap = on,
                Msg::RestoreTabs(on) => self.settings.restore_tabs = on,
                Msg::DisableUpdates(on) => self.settings.disable_update_check = on,
                Msg::Save => self.close(true),
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
