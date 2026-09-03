//! The application: tabs of tailed files over one `Ui` tree, the engine's
//! callbacks funnelled through channels into the log views, and the few
//! messages the chrome produces.

use crate::fonts;
use crate::logview::{LogRequest, LogView};
use crate::profiles::ProfilesWindow;
use crate::search::{SearchBar, SearchMsg};
use crate::settings::SettingsWindow;
use crate::theme;
use ctail_core::{
    resolve_palette, AppSettings, ConfigStore, Counters, LogLine, Rule, SearchMatcher, TabState,
    Tailer, TailerEvents, TailerOptions,
};
use denise::{DamageTracker, ElementState, Frame, InputEvent, KeyCode, Modifiers, Rect, Size};
use denise_text::TextStyle;
use denise_ui::widgets::{Checkbox, Label, Tabs};
use denise_ui::Anchors;
use denise_ui::{NodeId, Ui};
use denise_winit::{DeniseApp, Modality, WindowRequest};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Msg {
    SelectTab(usize),
    Follow(bool),
    Log(LogRequest),
    Search(SearchMsg),
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
    search: SearchBar,
    /// Validity and emptiness of the query the views are showing, so stepping
    /// through matches keeps reporting "bad regex" rather than a technically
    /// true "No results".
    search_valid: bool,
    search_empty: bool,
    /// Height of the status strip, so a resize can recompute the log area.
    status_h: i32,
    content: Rect,
    /// The Settings window reports through here: `Some` when it saved,
    /// `None` when it was dismissed. Either way it has closed.
    settings_tx: Sender<Option<AppSettings>>,
    settings_rx: Receiver<Option<AppSettings>>,
    /// Windows asked for this frame; the backend takes them after `update`.
    pending_windows: Vec<WindowRequest>,
    /// One Settings window at a time — a second would edit a stale copy.
    settings_open: bool,
    /// The Profiles window says here whenever the rules on disk changed.
    profiles_tx: Sender<()>,
    profiles_rx: Receiver<()>,
    profiles_open: bool,
    /// Paths of tabs closed this session, newest last.
    closed: Vec<String>,
    /// The window's size, kept current so it can be saved on the way out.
    window: Size,
    scale: f32,
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

        let content = Rect::new(0, strip_h, w, h - strip_h - status_h);
        let search = SearchBar::install(
            &mut ui,
            root,
            content,
            scale,
            Msg::Search,
            Msg::Search(SearchMsg::Next),
        );

        let (settings_tx, settings_rx) = mpsc::channel();
        let (profiles_tx, profiles_rx) = mpsc::channel();
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
            search,
            search_valid: true,
            search_empty: true,
            status_h,
            content,
            settings_tx,
            settings_rx,
            pending_windows: Vec::new(),
            settings_open: false,
            profiles_tx,
            profiles_rx,
            profiles_open: false,
            closed: Vec::new(),
            window: size,
            scale,
            title: "ctail".into(),
            started: Instant::now(),
            clipboard: arboard::Clipboard::new().ok(),
            exit: false,
        };
        if files.is_empty() {
            app.restore_session(&settings);
        } else {
            for f in files {
                app.open(f);
            }
        }
        app.debug_hooks();
        app
    }

    /// Development affordances, driven by the environment because this window
    /// cannot be scripted from outside without accessibility permission:
    /// `CTAIL_DEBUG_SEARCH` opens the find bar on that query,
    /// `CTAIL_DEBUG_SEARCH_FILTER` starts it in filter mode, and
    /// `CTAIL_DEBUG_SETTINGS` / `CTAIL_DEBUG_PROFILES` open those windows.
    fn debug_hooks(&mut self) {
        if std::env::var_os("CTAIL_DEBUG_SETTINGS").is_some() {
            self.open_settings();
        }
        if std::env::var_os("CTAIL_DEBUG_PROFILES").is_some() {
            self.open_profiles();
        }
        let Ok(query) = std::env::var("CTAIL_DEBUG_SEARCH") else {
            return;
        };
        if query.is_empty() {
            return;
        }
        if std::env::var_os("CTAIL_DEBUG_SEARCH_FILTER").is_some() {
            self.search
                .toggle(&mut self.ui, crate::search::Toggle::Filter);
        }
        self.search.open(&mut self.ui);
        self.search.set_query(&mut self.ui, &query);
        self.apply_search();
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
        self.closed.push(tab.path.clone());
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
        if self.search.is_open() {
            self.apply_search();
        }
        self.sync_chrome();
    }

    // --- search ---------------------------------------------------------

    fn open_search(&mut self) {
        self.search.open(&mut self.ui);
        self.apply_search(); // a query left in the field applies again at once
    }

    fn close_search(&mut self) {
        self.search.close(&mut self.ui);
        // Every view, not just the active one: a hidden tab must not come back
        // still filtered by a search the user has closed.
        let views: Vec<NodeId> = self.tabs.iter().map(|t| t.view).collect();
        for view in views {
            if let Some(v) = self.ui.widget_mut::<LogView<Msg>>(view) {
                v.set_search(None, false);
            }
            self.ui.invalidate(view);
        }
        if let Some(tab) = self.tabs.get(self.active) {
            let view = tab.view;
            self.ui.focus(Some(view));
        }
        self.sync_chrome();
    }

    /// Compiles what is in the field and hands it to the active view.
    fn apply_search(&mut self) {
        let text = self.search.query(&self.ui);
        let matcher = SearchMatcher::new(
            &text,
            self.search.case_sensitive,
            self.search.whole_word,
            self.search.is_regex,
        );
        self.search_valid = matcher.is_valid();
        self.search_empty = matcher.is_empty();
        let usable = (!self.search_empty && self.search_valid).then(|| Arc::new(matcher));
        let filter = self.search.filter;
        if let Some(tab) = self.tabs.get(self.active) {
            let view = tab.view;
            if let Some(v) = self.ui.widget_mut::<LogView<Msg>>(view) {
                v.set_search(usable, filter);
            }
            self.ui.invalidate(view);
        }
        self.refresh_counter();
        self.sync_chrome();
    }

    /// Reads the live match count out of the active view. Lines arriving while
    /// the bar is open change it without anyone pressing anything.
    fn refresh_counter(&mut self) {
        let status = self
            .tabs
            .get(self.active)
            .and_then(|t| self.ui.widget::<LogView<Msg>>(t.view))
            .map(|v| v.search_status())
            .unwrap_or_default();
        self.search.set_counter(
            &mut self.ui,
            status.current,
            status.total,
            self.search_valid,
            self.search_empty,
        );
    }

    fn step_search(&mut self, forward: bool) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let view = tab.view;
        let Some(v) = self.ui.widget_mut::<LogView<Msg>>(view) else {
            return;
        };
        if forward {
            v.next_match();
        } else {
            v.prev_match();
        }
        self.ui.invalidate(view);
        self.refresh_counter();
        self.sync_chrome();
    }

    // --- session ---------------------------------------------------------

    /// Reopens what the last session had open, in its saved order, and lands
    /// on the tab that was active. Missing files are skipped rather than
    /// reported: a log that has been rotated away is not an error worth a
    /// dialog on startup.
    fn restore_session(&mut self, settings: &AppSettings) {
        if !settings.restore_tabs {
            return;
        }
        let mut saved = settings.tabs.clone();
        saved.sort_by_key(|t| t.position);
        for tab in &saved {
            if std::path::Path::new(&tab.file_path).is_file() {
                self.open(tab.file_path.clone());
            }
        }
        if let Some(i) = self
            .tabs
            .iter()
            .position(|t| t.path == settings.last_active_tab_path)
        {
            self.activate(i);
        }
    }

    /// Saves the window, the open tabs and their order on the way out.
    fn persist(&mut self) {
        let mut s = self.config.load_settings();
        s.window.width = self.window.width as i32;
        s.window.height = self.window.height as i32;
        s.tabs = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, tab)| TabState {
                file_path: tab.path.clone(),
                profile_id: s.active_profile.clone(),
                auto_scroll: true,
                label: String::new(),
                color: String::new(),
                position: i as i32,
            })
            .collect();
        s.last_active_tab_path = self
            .tabs
            .get(self.active)
            .map(|t| t.path.clone())
            .unwrap_or_default();
        self.config.save_settings(&s);
    }

    // --- tabs ------------------------------------------------------------

    fn cycle_tab(&mut self, forward: bool) {
        if self.tabs.len() < 2 {
            return;
        }
        let n = self.tabs.len();
        let next = if forward {
            (self.active + 1) % n
        } else {
            (self.active + n - 1) % n
        };
        self.activate(next);
    }

    fn reopen_closed(&mut self) {
        while let Some(path) = self.closed.pop() {
            if std::path::Path::new(&path).is_file() {
                self.open(path);
                return;
            }
        }
    }

    // --- settings --------------------------------------------------------

    fn open_settings(&mut self) {
        if self.settings_open {
            return;
        }
        self.settings_open = true;
        let tx = self.settings_tx.clone();
        self.pending_windows.push(
            WindowRequest::new(SettingsWindow::config(), move |size, scale| {
                SettingsWindow::new(size, scale, tx)
            })
            .with_modality(Modality::Owned),
        );
    }

    fn open_profiles(&mut self) {
        if self.profiles_open {
            return;
        }
        self.profiles_open = true;
        let tx = self.profiles_tx.clone();
        self.pending_windows.push(
            WindowRequest::new(ProfilesWindow::config_window(), move |size, scale| {
                ProfilesWindow::new(size, scale, tx)
            })
            .with_modality(Modality::Owned),
        );
    }

    /// Re-reads the active profile and restyles every open tab. Called when the
    /// Profiles window saves, so an edited rule shows up without a restart.
    fn reload_rules(&mut self) {
        let settings = self.config.load_settings();
        self.rules = self
            .config
            .load_profile(&settings.active_profile)
            .map(|p| p.rules)
            .unwrap_or_default();
        let views: Vec<NodeId> = self.tabs.iter().map(|t| t.view).collect();
        for view in views {
            if let Some(v) = self.ui.widget_mut::<LogView<Msg>>(view) {
                v.set_rules(&self.rules);
            }
            self.ui.invalidate(view);
        }
    }

    /// Takes settings back from the Settings window: persists them, then
    /// applies live everything that does not need a restart.
    fn apply_settings(&mut self, new: AppSettings) {
        let old = self.config.load_settings();
        self.config.save_settings(&new);

        if new.theme != old.theme || new.theme_mode != old.theme_mode {
            let palette =
                resolve_palette(&new.theme, &new.theme_mode, Some(self.config.themes_dir()));
            let metrics = self.ui.theme().metrics;
            let mut theme = theme::from_palette(&new.theme, &new.theme_mode, &palette);
            theme.metrics = metrics;
            self.ui.set_theme(theme);
        }
        if new.font_size != old.font_size {
            self.mono.size_px = (new.font_size.max(6) as f32 * self.scale + 0.5) as u16;
        }
        let cap = new.buffer_size.clamp(200, 1_000_000) as usize;
        let views: Vec<NodeId> = self.tabs.iter().map(|t| t.view).collect();
        for view in views {
            if let Some(v) = self.ui.widget_mut::<LogView<Msg>>(view) {
                v.set_style(self.mono);
                v.set_cap(cap);
                v.set_show_line_numbers(new.show_line_numbers);
            }
            self.ui.invalidate(view);
        }
        if new.poll_interval_ms != old.poll_interval_ms {
            let poll = Duration::from_millis(new.poll_interval_ms.clamp(100, 60_000) as u64);
            for tab in &self.tabs {
                tab.tailer.set_poll_interval(poll);
            }
        }
        self.ui.invalidate_all();
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
            if self.search.is_open() {
                self.refresh_counter();
            }
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
            Msg::Search(SearchMsg::Toggle(which)) => {
                self.search.toggle(&mut self.ui, which);
                self.apply_search();
            }
            Msg::Search(SearchMsg::Next) => self.step_search(true),
            Msg::Search(SearchMsg::Prev) => self.step_search(false),
            Msg::Search(SearchMsg::Close) => self.close_search(),
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
        // Shortcuts are taken here rather than in a widget, and the ones that
        // act are not passed on: Escape closing the find bar must not also
        // clear the log's selection underneath it.
        let mut forwarded: Vec<InputEvent> = Vec::with_capacity(events.len());
        for event in events {
            match event {
                InputEvent::CloseRequested => {
                    self.persist();
                    self.exit = true;
                    continue;
                }
                InputEvent::SurfaceResized { size, .. } => {
                    self.window = *size;
                    self.content.width = size.width as i32;
                    self.content.height =
                        (size.height as i32 - self.content.y - self.status_h).max(1);
                }
                InputEvent::Key {
                    code,
                    state: ElementState::Down,
                    modifiers,
                    ..
                } => {
                    let cmd =
                        modifiers.contains(Modifiers::SUPER) || modifiers.contains(Modifiers::CTRL);
                    if cmd {
                        match code {
                            KeyCode::O => {
                                self.open_dialog();
                                continue;
                            }
                            KeyCode::W => {
                                self.close_active();
                                continue;
                            }
                            KeyCode::Q => {
                                self.persist();
                                self.exit = true;
                                continue;
                            }
                            KeyCode::F => {
                                self.open_search();
                                continue;
                            }
                            KeyCode::Comma => {
                                self.open_settings();
                                continue;
                            }
                            KeyCode::R => {
                                self.open_profiles();
                                continue;
                            }
                            // Cmd+Tab belongs to the system on macOS, so tab
                            // cycling is Ctrl+Tab everywhere.
                            KeyCode::Tab if modifiers.contains(Modifiers::CTRL) => {
                                self.cycle_tab(!modifiers.contains(Modifiers::SHIFT));
                                continue;
                            }
                            KeyCode::T if modifiers.contains(Modifiers::SHIFT) => {
                                self.reopen_closed();
                                continue;
                            }
                            _ => {}
                        }
                    }
                    if self.search.is_open() {
                        match code {
                            KeyCode::Escape => {
                                self.close_search();
                                continue;
                            }
                            // Enter alone is the field's own submit message.
                            KeyCode::Enter if modifiers.contains(Modifiers::SHIFT) => {
                                self.step_search(false);
                                continue;
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
            forwarded.push(event.clone());
        }
        // The Settings window hands its result back through a channel; it may
        // have closed itself by the time this runs.
        let mut saved = None;
        while let Ok(result) = self.settings_rx.try_recv() {
            self.settings_open = false;
            if result.is_some() {
                saved = result;
            }
        }
        if let Some(s) = saved {
            self.apply_settings(s);
        }
        let mut rules_changed = false;
        while self.profiles_rx.try_recv().is_ok() {
            self.profiles_open = false;
            rules_changed = true;
        }
        if rules_changed {
            self.reload_rules();
        }
        self.pump_engine();
        self.ui.handle(&forwarded);
        // The field reports on submit, not per keystroke, so typing is noticed
        // by comparing what is in it.
        if self.search.is_open() {
            if self.search.take_text_change(&self.ui).is_some() {
                self.apply_search();
            } else {
                // The count moves on its own — lines arrive, the view scrolls —
                // so the bar reads it back rather than being told once.
                self.refresh_counter();
            }
        }
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

    fn take_windows(&mut self) -> Vec<WindowRequest> {
        // A window that has been handed over is gone from here; the flag is
        // cleared when its settings arrive or when the user is done with it.
        std::mem::take(&mut self.pending_windows)
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
