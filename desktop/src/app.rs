//! The application: tabs of tailed files over one `Ui` tree, the engine's
//! callbacks funnelled through channels into the log views, and the few
//! messages the chrome produces.

use crate::fonts;
use crate::logview::{LogRequest, LogView};
use crate::theme;
use ctail_core::{
    resolve_palette, ConfigStore, Counters, LogLine, Rule, Tailer, TailerEvents, TailerOptions,
};
use denise::{DamageTracker, ElementState, Frame, InputEvent, KeyCode, Modifiers, Rect, Size};
use denise_text::TextStyle;
use denise_ui::widgets::{Checkbox, Label, Tabs};
use denise_ui::Anchors;
use denise_ui::{NodeId, Ui};
use denise_winit::DeniseApp;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Msg {
    SelectTab(usize),
    Follow(bool),
    Log(LogRequest),
}

enum TabEvent {
    Lines(Vec<LogLine>, bool),
    Reset,
    Error(String),
    Ready,
    Base(i64, i64),
    Older(Vec<LogLine>),
}

/// Engine thread -> channel; the UI thread drains it every frame.
struct Listener {
    tx: Sender<TabEvent>,
    counters: OnceLock<Arc<Counters>>,
}

impl Listener {
    fn provisional(&self) -> bool {
        self.counters.get().is_some_and(|c| !c.indexing_complete())
    }
}

impl TailerEvents for Listener {
    fn on_lines(&self, lines: Vec<LogLine>) {
        let _ = self.tx.send(TabEvent::Lines(lines, self.provisional()));
    }
    fn on_reset(&self) {
        let _ = self.tx.send(TabEvent::Reset);
    }
    fn on_error(&self, message: String) {
        let _ = self.tx.send(TabEvent::Error(message));
    }
    fn on_ready(&self) {
        let _ = self.tx.send(TabEvent::Ready);
    }
    fn on_base_resolved(&self, base: i64) {
        let total = self.counters.get().map_or(0, |c| c.total_lines());
        let _ = self.tx.send(TabEvent::Base(base, total));
    }
}

struct Tab {
    path: String,
    tailer: Tailer,
    tx: Sender<TabEvent>,
    rx: Receiver<TabEvent>,
    view: NodeId,
    error: Option<String>,
}

impl Tab {
    fn name(&self) -> String {
        std::path::Path::new(&self.path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.clone())
    }
}

pub struct App {
    ui: Ui<Msg>,
    config: ConfigStore,
    rules: Vec<Rule>,
    mono: TextStyle,
    tabs: Vec<Tab>,
    active: usize,
    strip: NodeId,
    status: NodeId,
    follow: NodeId,
    content: Rect,
    title: String,
    started: Instant,
    clipboard: Option<arboard::Clipboard>,
    exit: bool,
}

impl App {
    pub fn new(size: Size, scale: f32, files: Vec<String>) -> Self {
        let config = ConfigStore::new(None);
        config.ensure_default_profile();
        let settings = config.load_settings();
        let palette = resolve_palette(
            &settings.theme,
            &settings.theme_mode,
            Some(config.themes_dir()),
        );
        let theme =
            theme::from_palette(&settings.theme, &settings.theme_mode, &palette).scaled(scale);
        let px = |v: f32| (v * scale + 0.5) as u16;
        let s = |v: i32| (v as f32 * scale + 0.5) as i32;

        let mut ui: Ui<Msg> = Ui::new(size, theme);
        if let Some((_, source)) = fonts::load(fonts::UI) {
            let id = ui.add_font(source);
            ui.set_default_font(id);
        }
        let mono = match fonts::load(fonts::MONO) {
            Some((_, source)) => TextStyle {
                font: ui.add_font(source),
                size_px: px(settings.font_size.max(6) as f32),
            },
            None => TextStyle::built_in(px(settings.font_size.max(6) as f32)),
        };
        let rules = config
            .load_profile(&settings.active_profile)
            .map(|p| p.rules)
            .unwrap_or_default();

        let root = ui.root();
        let (w, h) = (size.width as i32, size.height as i32);
        let strip_h = s(36);
        let status_h = s(30);
        let strip = ui
            .add(
                root,
                Tabs::new(Vec::<String>::new(), Msg::SelectTab)
                    .with_style(TextStyle::built_in(px(14.0))),
                Rect::new(0, 0, w, strip_h),
            )
            .expect("tab strip");
        ui.set_anchors(
            strip,
            Anchors {
                left: true,
                top: true,
                right: true,
                bottom: false,
            },
        );
        let status = ui
            .add(
                root,
                Label::new("Open a file: ⌘O / Ctrl+O").with_size(px(13.0)),
                Rect::new(s(12), h - status_h + s(6), w - s(160), status_h - s(8)),
            )
            .expect("status");
        ui.set_anchors(
            status,
            Anchors {
                left: true,
                top: false,
                right: true,
                bottom: true,
            },
        );
        let follow = ui
            .add(
                root,
                Checkbox::new("Follow", Msg::Follow)
                    .with_checked(true)
                    .with_size(px(13.0)),
                Rect::new(w - s(120), h - status_h + s(4), s(110), status_h - s(8)),
            )
            .expect("follow");
        ui.set_anchors(
            follow,
            Anchors {
                left: false,
                top: false,
                right: true,
                bottom: true,
            },
        );

        let mut app = Self {
            ui,
            config,
            rules,
            mono,
            tabs: Vec::new(),
            active: 0,
            strip,
            status,
            follow,
            content: Rect::new(0, strip_h, w, h - strip_h - status_h),
            title: "ctail".into(),
            started: Instant::now(),
            clipboard: arboard::Clipboard::new().ok(),
            exit: false,
        };
        for f in files {
            app.open(f);
        }
        app
    }

    fn tailer_options(&self) -> TailerOptions {
        let s = self.config.load_settings();
        let poll = Duration::from_millis(s.poll_interval_ms.clamp(100, 60_000) as u64);
        TailerOptions {
            poll_interval: poll,
            read_timeout: Duration::from_secs(s.read_timeout_sec.max(1) as u64),
            ..Default::default()
        }
    }

    fn open(&mut self, path: String) {
        if let Some(i) = self.tabs.iter().position(|t| t.path == path) {
            self.activate(i);
            return;
        }
        let (tx, rx) = mpsc::channel();
        let listener = Arc::new(Listener {
            tx: tx.clone(),
            counters: OnceLock::new(),
        });
        let tailer = Tailer::new(&path, self.tailer_options(), listener.clone());
        let _ = listener.counters.set(tailer.counters());
        let cap = self
            .config
            .load_settings()
            .buffer_size
            .clamp(200, 1_000_000) as usize;
        let view = self
            .ui
            .add(
                self.ui.root(),
                LogView::new(Msg::Log, self.mono, &self.rules, cap),
                self.content,
            )
            .expect("log view");
        self.ui.set_anchors(
            view,
            Anchors {
                left: true,
                top: true,
                right: true,
                bottom: true,
            },
        );
        tailer.start();
        self.config.add_recent_file(&path, 15);
        self.tabs.push(Tab {
            path,
            tailer,
            tx,
            rx,
            view,
            error: None,
        });
        self.refresh_strip();
        self.activate(self.tabs.len() - 1);
    }

    fn close_active(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        let tab = self.tabs.remove(self.active);
        tab.tailer.stop();
        self.ui.remove(tab.view);
        self.refresh_strip();
        if !self.tabs.is_empty() {
            self.activate(self.active.min(self.tabs.len() - 1));
        } else {
            self.title = "ctail".into();
            self.set_status("Open a file: ⌘O / Ctrl+O".into());
        }
    }

    fn refresh_strip(&mut self) {
        let labels: Vec<String> = self.tabs.iter().map(Tab::name).collect();
        if let Some(strip) = self.ui.widget_mut::<Tabs<Msg>>(self.strip) {
            strip.set_labels(labels);
        }
        self.ui.invalidate(self.strip);
    }

    fn activate(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        self.active = index;
        for (i, tab) in self.tabs.iter().enumerate() {
            self.ui.set_visible(tab.view, i == index);
        }
        if let Some(strip) = self.ui.widget_mut::<Tabs<Msg>>(self.strip) {
            strip.set_selected(index);
        }
        self.ui.invalidate(self.strip);
        let view = self.tabs[index].view;
        self.ui.focus(Some(view));
        self.title = format!("ctail — {}", self.tabs[index].name());
        self.sync_chrome();
    }

    fn open_dialog(&mut self) {
        let mut dialog = rfd::FileDialog::new().set_title("Open log file");
        if let Some(active) = self.tabs.get(self.active) {
            if let Some(dir) = std::path::Path::new(&active.path).parent() {
                dialog = dialog.set_directory(dir);
            }
        }
        if let Some(path) = dialog.pick_file() {
            self.open(path.to_string_lossy().into_owned());
        }
    }

    fn set_status(&mut self, text: String) {
        if let Some(label) = self.ui.widget_mut::<Label>(self.status) {
            label.set_text(text);
        }
        self.ui.invalidate(self.status);
    }

    /// Status line + follow box reflect the active tab.
    fn sync_chrome(&mut self) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let view_id = tab.view;
        let name = tab.name();
        let error = tab.error.clone();
        let (total, following) = self
            .ui
            .widget::<LogView<Msg>>(view_id)
            .map(|v| (v.total_lines(), v.following()))
            .unwrap_or((0, true));
        let text = match error {
            Some(e) => format!("{name} · ⚠︎ {e}"),
            None => format!("{name} · {total} lines"),
        };
        self.set_status(text);
        if let Some(cb) = self.ui.widget_mut::<Checkbox<Msg>>(self.follow) {
            cb.set_checked(following);
        }
        self.ui.invalidate(self.follow);
    }

    /// Drains every tab's engine events into its view.
    fn pump_engine(&mut self) {
        let mut chrome_dirty = false;
        for i in 0..self.tabs.len() {
            let view = self.tabs[i].view;
            let mut changed = false;
            while let Ok(ev) = self.tabs[i].rx.try_recv() {
                changed = true;
                let Some(v) = self.ui.widget_mut::<LogView<Msg>>(view) else {
                    continue;
                };
                match ev {
                    TabEvent::Lines(lines, provisional) => v.append(lines, provisional),
                    TabEvent::Reset => v.reset(),
                    TabEvent::Older(lines) => v.prepend(lines),
                    TabEvent::Base(base, total) => v.apply_base(base, total),
                    TabEvent::Ready => self.tabs[i].error = None,
                    TabEvent::Error(e) => self.tabs[i].error = Some(e),
                }
            }
            if changed {
                self.ui.invalidate(view);
                if i == self.active {
                    chrome_dirty = true;
                }
            }
        }
        if chrome_dirty {
            self.sync_chrome();
        }
    }

    fn handle_message(&mut self, msg: Msg) {
        match msg {
            Msg::SelectTab(i) => self.activate(i),
            Msg::Follow(on) => {
                if let Some(tab) = self.tabs.get(self.active) {
                    let view = tab.view;
                    if let Some(v) = self.ui.widget_mut::<LogView<Msg>>(view) {
                        v.set_follow(on);
                    }
                    self.ui.invalidate(view);
                }
            }
            Msg::Log(LogRequest::Follow(on)) => {
                if let Some(cb) = self.ui.widget_mut::<Checkbox<Msg>>(self.follow) {
                    cb.set_checked(on);
                }
                self.ui.invalidate(self.follow);
            }
            Msg::Log(LogRequest::Copy) => {
                let text = self
                    .tabs
                    .get(self.active)
                    .and_then(|t| self.ui.widget::<LogView<Msg>>(t.view))
                    .and_then(LogView::selected_text);
                if let (Some(text), Some(cb)) = (text, self.clipboard.as_mut()) {
                    let _ = cb.set_text(text);
                }
            }
            Msg::Log(LogRequest::Older) => {
                let Some(tab) = self.tabs.get(self.active) else {
                    return;
                };
                let Some(v) = self.ui.widget::<LogView<Msg>>(tab.view) else {
                    return;
                };
                let first = v.first_number().unwrap_or(1);
                let count = (v.visible_rows() * 4).max(200) as i64;
                let start = (first - count).max(1);
                let want = (first - start) as usize;
                let tx = tab.tx.clone();
                if want == 0 {
                    let _ = tx.send(TabEvent::Older(Vec::new()));
                    return;
                }
                tab.tailer.fetch_range(start, want, move |lines| {
                    let _ = tx.send(TabEvent::Older(lines));
                });
            }
        }
    }
}

impl DeniseApp for App {
    fn update(&mut self, events: &[InputEvent], damage: &mut DamageTracker) {
        for event in events {
            match event {
                InputEvent::CloseRequested => self.exit = true,
                InputEvent::Key {
                    code,
                    state: ElementState::Down,
                    modifiers,
                    ..
                } if modifiers.contains(Modifiers::SUPER)
                    || modifiers.contains(Modifiers::CTRL) =>
                {
                    match code {
                        KeyCode::O => self.open_dialog(),
                        KeyCode::W => self.close_active(),
                        KeyCode::Q => self.exit = true,
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        self.pump_engine();
        self.ui.handle(events);
        self.ui.tick(self.started.elapsed().as_millis() as u64);
        let messages: Vec<Msg> = self.ui.drain_messages().collect();
        for m in messages {
            self.handle_message(m);
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

    fn title(&self) -> Option<&str> {
        Some(&self.title)
    }

    fn next_frame_in(&self) -> Option<Duration> {
        let now = self.started.elapsed().as_millis() as u64;
        let ui = self
            .ui
            .next_wake_ms()
            .map(|w| Duration::from_millis(w.saturating_sub(now)));
        // Engine events arrive on their own threads and cannot wake the loop
        // yet, so poll them at the tail cadence while a file is open.
        let poll = (!self.tabs.is_empty()).then_some(Duration::from_millis(100));
        match (ui, poll) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }
}
