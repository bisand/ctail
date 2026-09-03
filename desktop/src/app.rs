//! The application: tabs of tailed files over one `Ui` tree, the engine's
//! callbacks funnelled through channels into the log views, and the few
//! messages the chrome produces.

use crate::fonts;
use crate::logview::{LogRequest, LogView};
use crate::profiles::ProfilesWindow;
use crate::prompt::PromptWindow;
use crate::search::{Counter, SearchBar, SearchMsg};
use crate::settings::SettingsWindow;
use crate::tabbar::{TabBar, TabItem};
use crate::theme;
use ctail_core::{
    check_for_update, resolve_palette, AppSettings, ConfigStore, Counters, FileSearch,
    FileSearchEvents, FileSearchQuery, FileSearchStatus, LogLine, Rule, SearchMatcher, TabState,
    Tailer, TailerEvents, TailerOptions, UpdateCheck,
};
use denise::{DamageTracker, ElementState, Frame, InputEvent, KeyCode, Modifiers, Rect, Size};
use denise_text::TextStyle;
use denise_ui::widgets::{open_menu, open_menu_at, Checkbox, Label, MenuBar, MenuItem};
use denise_ui::Anchors;
use denise_ui::{NodeId, Ui};
use denise_winit::{DeniseApp, Modality, WindowRequest};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Msg {
    SelectTab(usize),
    CloseTab(usize),
    TabContext(usize),
    Follow(bool),
    Log(LogRequest),
    Search(SearchMsg),
    /// A menu-bar title was pressed.
    Menu(usize),
    /// A row of whichever menu is open was chosen.
    MenuPick(usize),
}

/// What a menu row does. Kept beside the entries the popup was built from, so
/// a row is identified by position and the popup itself stays a plain list.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Action {
    /// A heading, or a command that is not available.
    None,
    OpenFile,
    OpenRecent(String),
    ClearRecent,
    CloseTab,
    ReopenTab,
    Quit,
    Copy,
    SelectAll,
    Find,
    ToggleLineNumbers,
    ToggleWordWrap,
    ToggleTheme,
    Profiles,
    Settings,
    CheckUpdates,
    About,
    TabRename,
    TabRefresh,
    TabChangePath,
    TabCopyPath,
    TabReveal,
    TabClose,
    TabColor(String),
}

/// Shows a file in the platform's file manager. Each of the three has its own
/// spelling of "select this one", and none of them is a library call.
fn reveal(path: &str) {
    let _ = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
            .args(["-R", path])
            .spawn()
    } else if cfg!(target_os = "windows") {
        std::process::Command::new("explorer")
            .arg(format!("/select,{path}"))
            .spawn()
    } else {
        // No portable "select the file", so the folder it is in is the answer.
        let dir = std::path::Path::new(path)
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default();
        std::process::Command::new("xdg-open").arg(dir).spawn()
    };
}

/// Opens a web page in the default browser.
fn open_url(url: &str) {
    let _ = if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).spawn()
    } else if cfg!(target_os = "windows") {
        std::process::Command::new("cmd")
            .args(["/c", "start", "", url])
            .spawn()
    } else {
        std::process::Command::new("xdg-open").arg(url).spawn()
    };
}

/// The version this binary is, for the update check.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The colours a tab can be marked with, matching the macOS app's set.
const TAB_COLORS: [(&str, &str); 6] = [
    ("Red", "#f38ba8"),
    ("Orange", "#fab387"),
    ("Yellow", "#f9e2af"),
    ("Green", "#a6e3a1"),
    ("Blue", "#89b4fa"),
    ("Purple", "#cba6f7"),
];

enum TabEvent {
    Lines(Vec<LogLine>, bool),
    /// A range fetched from elsewhere in the file, and the line to land on.
    Jump(Vec<LogLine>, i64),
    Reset,
    Error(String),
    Ready,
    Base(i64, i64),
    Older(Vec<LogLine>),
}

/// A finished file scan, from its own thread to the UI's. The count itself
/// stays in the engine; this only says that it moved.
struct ScanNotice(Mutex<Sender<()>>);

impl FileSearchEvents for ScanNotice {
    fn on_result(&self, _query: FileSearchQuery, _total: u32) {
        let _ = self.0.lock().unwrap().send(());
    }
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
    /// The user's name for this tab; empty means the file name is used.
    label: String,
    /// Hex colour the user marked it with, or empty.
    color: String,
    tailer: Tailer,
    tx: Sender<TabEvent>,
    rx: Receiver<TabEvent>,
    view: NodeId,
    error: Option<String>,
}

impl Tab {
    fn file_name(&self) -> String {
        std::path::Path::new(&self.path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.clone())
    }

    fn name(&self) -> String {
        if self.label.is_empty() {
            self.file_name()
        } else {
            self.label.clone()
        }
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
    menubar: NodeId,
    /// What each row of the open menu does.
    menu_actions: Vec<Action>,
    /// The tab a context menu was opened on.
    context_tab: Option<usize>,
    /// Set while a rename prompt is open, with the tab it will rename.
    renaming: Option<usize>,
    prompt_tx: Sender<Option<String>>,
    prompt_rx: Receiver<Option<String>>,
    status: NodeId,
    follow: NodeId,
    search: SearchBar,
    /// Validity and emptiness of the query the views are showing, so stepping
    /// through matches keeps reporting "bad regex" rather than a technically
    /// true "No results".
    search_valid: bool,
    search_empty: bool,
    /// The scan of the file on disk behind the bar's counter and its ↑/↓.
    file_search: FileSearch,
    /// A tap on the shoulder when a scan finishes; the count is read back from
    /// `file_search`, which holds it.
    scans: Receiver<()>,
    /// Answers from update checks, with whether the reader asked for one:
    /// a quiet launch-time check only speaks up when there is an update.
    updates_tx: Sender<(UpdateCheck, bool)>,
    updates_rx: Receiver<(UpdateCheck, bool)>,
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
        let menu_h = s(28);
        let strip_h = s(34);
        let status_h = s(30);
        let menu_style = TextStyle::built_in(px(13.0));
        let menubar = ui
            .add(
                root,
                MenuBar::new(["File", "Edit", "View", "Help"], Msg::Menu).with_style(menu_style),
                Rect::new(0, 0, w, menu_h),
            )
            .expect("menu bar");
        ui.set_anchors(
            menubar,
            Anchors {
                left: true,
                top: true,
                right: true,
                bottom: false,
            },
        );
        let strip = ui
            .add(
                root,
                TabBar::new(Msg::SelectTab, Msg::CloseTab, Msg::TabContext)
                    .with_style(TextStyle::built_in(px(13.0))),
                Rect::new(0, menu_h, w, strip_h),
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

        let content = Rect::new(0, menu_h + strip_h, w, h - menu_h - strip_h - status_h);
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
        let (prompt_tx, prompt_rx) = mpsc::channel();
        let (scan_tx, scans) = mpsc::channel();
        let (updates_tx, updates_rx) = mpsc::channel();
        let file_search = FileSearch::new(Arc::new(ScanNotice(Mutex::new(scan_tx))));
        let mut app = Self {
            ui,
            config,
            rules,
            mono,
            tabs: Vec::new(),
            active: 0,
            strip,
            menubar,
            menu_actions: Vec::new(),
            context_tab: None,
            renaming: None,
            prompt_tx,
            prompt_rx,
            status,
            follow,
            search,
            search_valid: true,
            search_empty: true,
            file_search,
            scans,
            updates_tx,
            updates_rx,
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
        app.maybe_check_for_updates();
        app
    }

    /// Development affordances, driven by the environment because this window
    /// cannot be scripted from outside without accessibility permission:
    /// `CTAIL_DEBUG_SEARCH` opens the find bar on that query,
    /// `CTAIL_DEBUG_SEARCH_STEP` presses ↓ that many times (negative for ↑),
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
        let settings = self.config.load_settings();
        let cap = settings.buffer_size.clamp(200, 1_000_000) as usize;
        let mut log = LogView::new(Msg::Log, self.mono, &self.rules, cap);
        log.set_show_line_numbers(settings.show_line_numbers);
        log.set_word_wrap(settings.word_wrap);
        let view = self
            .ui
            .add(self.ui.root(), log, self.content)
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
        let saved = self
            .config
            .load_settings()
            .tabs
            .into_iter()
            .find(|t| t.file_path == path);
        self.tabs.push(Tab {
            label: saved.as_ref().map(|t| t.label.clone()).unwrap_or_default(),
            color: saved.as_ref().map(|t| t.color.clone()).unwrap_or_default(),
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
        let items: Vec<TabItem> = self
            .tabs
            .iter()
            .map(|t| TabItem {
                label: t.name(),
                color: t.color.clone(),
            })
            .collect();
        let active = self.active;
        if let Some(strip) = self.ui.widget_mut::<TabBar<Msg>>(self.strip) {
            strip.set_items(items);
            strip.set_selected(active);
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
        if let Some(strip) = self.ui.widget_mut::<TabBar<Msg>>(self.strip) {
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
        self.file_search.clear();
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

    /// What the whole-file search should be answering, if anything.
    ///
    /// Nothing, in filter mode: filtering shows the lines the window holds,
    /// and counting matches the view cannot show would be answering a question
    /// nobody asked.
    fn file_query(&self) -> Option<FileSearchQuery> {
        if !self.search.is_open() || self.search.filter || self.search_empty || !self.search_valid {
            return None;
        }
        Some(FileSearchQuery {
            path: self.tabs.get(self.active)?.path.clone(),
            text: self.search.query(&self.ui),
            case_sensitive: self.search.case_sensitive,
            whole_word: self.search.whole_word,
            is_regex: self.search.is_regex,
        })
    }

    /// Reads the match count out of the file scan, or out of the active view
    /// while the scan is still running. Lines arriving while the bar is open
    /// change the second of those without anyone pressing anything.
    fn refresh_counter(&mut self) {
        let window = self
            .tabs
            .get(self.active)
            .and_then(|t| self.ui.widget::<LogView<Msg>>(t.view))
            .map(|v| v.search_status())
            .unwrap_or_default();
        let counter = match self.file_query() {
            None if !self.search_valid => Counter::BadRegex,
            None if self.search_empty => Counter::Empty,
            None => Counter::at(window.current, window.total),
            Some(query) => {
                self.file_search.request(query.clone());
                match self.file_search.status(&query) {
                    FileSearchStatus::Ready { current, total } => {
                        Counter::at(current as usize, total as usize)
                    }
                    // The window's count is a true count of something while
                    // the file's is still being worked out, and ↑/↓ step it.
                    FileSearchStatus::Scanning | FileSearchStatus::Idle => {
                        Counter::Scanning(window.total)
                    }
                }
            }
        };
        self.search.set_counter(&mut self.ui, counter);
    }

    /// Presses ↓ (or ↑) for a snapshot, after the scan the window cannot wait
    /// for interactively.
    pub fn debug_step_search(&mut self, times: usize, forward: bool) {
        for _ in 0..times {
            self.step_search(forward);
        }
    }

    /// Next/previous match. Over the whole file once it has been scanned, and
    /// over the window in memory until then, so ↓ answers straight away on a
    /// file too big to have finished scanning.
    fn step_search(&mut self, forward: bool) {
        if let Some(query) = self.file_query() {
            let from = self
                .tabs
                .get(self.active)
                .and_then(|t| self.ui.widget::<LogView<Msg>>(t.view))
                .and_then(|v| v.first_visible_number());
            if let Some(number) = self.file_search.step(&query, forward, from) {
                self.go_to_line(number);
                self.refresh_counter();
                self.sync_chrome();
                return;
            }
        }
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

    /// Puts the line numbered `number` in the middle of the view, fetching the
    /// part of the file around it when the window does not reach that far.
    fn go_to_line(&mut self, number: i64) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let view = tab.view;
        let rows = self
            .ui
            .widget::<LogView<Msg>>(view)
            .map_or(50, |v| v.visible_rows());
        if let Some(v) = self.ui.widget_mut::<LogView<Msg>>(view) {
            if v.reveal_number(number) {
                self.ui.invalidate(view);
                return;
            }
        }
        let count = (rows * 6).max(300);
        let start = (number - count as i64 / 2).max(1);
        let tx = tab.tx.clone();
        tab.tailer.fetch_range(start, count, move |lines| {
            let _ = tx.send(TabEvent::Jump(lines, number));
        });
    }

    /// The window jumped somewhere else in the file and the reader wants the
    /// end back: the tail has to be read again, there being nothing to append
    /// live lines to.
    fn reattach_active(&mut self) {
        let Some(view) = self.tabs.get(self.active).map(|t| t.view) else {
            return;
        };
        if let Some(v) = self.ui.widget_mut::<LogView<Msg>>(view) {
            if !v.is_detached() {
                return;
            }
            v.reset();
        }
        self.ui.invalidate(view);
        self.tabs[self.active].tailer.refresh();
    }

    // --- menus -----------------------------------------------------------

    /// The rows of one menu-bar menu, and what each of them does.
    fn menu_entries(&self, which: usize) -> (Vec<MenuItem>, Vec<Action>) {
        let mut rows: Vec<(MenuItem, Action)> = Vec::new();
        match which {
            0 => {
                rows.push((
                    MenuItem::new("Open…").with_shortcut("Cmd+O"),
                    Action::OpenFile,
                ));
                let has_tabs = !self.tabs.is_empty();
                rows.push((
                    MenuItem::new("Close Tab")
                        .with_shortcut("Cmd+W")
                        .enabled(has_tabs),
                    Action::CloseTab,
                ));
                rows.push((
                    MenuItem::new("Reopen Closed Tab")
                        .with_shortcut("Cmd+Shift+T")
                        .enabled(!self.closed.is_empty()),
                    Action::ReopenTab,
                ));
                let recent = self.config.recent_files();
                rows.push((MenuItem::heading("Recent"), Action::None));
                if recent.is_empty() {
                    rows.push((MenuItem::new("(empty)").disabled(), Action::None));
                } else {
                    for path in recent.iter().take(8) {
                        let name = std::path::Path::new(path)
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| path.clone());
                        rows.push((MenuItem::new(name), Action::OpenRecent(path.clone())));
                    }
                    rows.push((MenuItem::new("Clear Recent"), Action::ClearRecent));
                }
                rows.push((MenuItem::new("Quit").with_shortcut("Cmd+Q"), Action::Quit));
            }
            1 => {
                rows.push((MenuItem::new("Copy").with_shortcut("Cmd+C"), Action::Copy));
                rows.push((
                    MenuItem::new("Select All").with_shortcut("Cmd+A"),
                    Action::SelectAll,
                ));
                rows.push((MenuItem::new("Find…").with_shortcut("Cmd+F"), Action::Find));
            }
            2 => {
                let settings = self.config.load_settings();
                rows.push((
                    MenuItem::new("Show Line Numbers")
                        .with_shortcut("Cmd+Shift+L")
                        .checked(settings.show_line_numbers),
                    Action::ToggleLineNumbers,
                ));
                rows.push((
                    MenuItem::new("Word Wrap")
                        .with_shortcut("Cmd+Alt+W")
                        .checked(settings.word_wrap),
                    Action::ToggleWordWrap,
                ));
                rows.push((
                    MenuItem::new("Light Theme").checked(settings.theme_mode == "light"),
                    Action::ToggleTheme,
                ));
                rows.push((
                    MenuItem::new("Profiles & Rules…").with_shortcut("Cmd+R"),
                    Action::Profiles,
                ));
                rows.push((
                    MenuItem::new("Settings…").with_shortcut("Cmd+,"),
                    Action::Settings,
                ));
            }
            _ => {
                rows.push((MenuItem::new("Check for Updates…"), Action::CheckUpdates));
                rows.push((MenuItem::new("About ctail"), Action::About));
            }
        }
        rows.into_iter().unzip()
    }

    /// The rows of a tab's right-click menu.
    fn tab_menu_entries(&self) -> (Vec<MenuItem>, Vec<Action>) {
        let reveal = if cfg!(target_os = "macos") {
            "Reveal in Finder"
        } else if cfg!(target_os = "windows") {
            "Show in Explorer"
        } else {
            "Show in File Manager"
        };
        let mut rows = vec![
            (MenuItem::new("Rename…"), Action::TabRename),
            (MenuItem::new("Refresh"), Action::TabRefresh),
            (MenuItem::new("Change File Path…"), Action::TabChangePath),
            (MenuItem::new("Copy Path"), Action::TabCopyPath),
            (MenuItem::new(reveal), Action::TabReveal),
            (MenuItem::new("Close Tab"), Action::TabClose),
            (MenuItem::heading("Colour"), Action::None),
        ];
        let current = self
            .context_tab
            .and_then(|i| self.tabs.get(i))
            .map(|t| t.color.clone())
            .unwrap_or_default();
        for (name, hex) in TAB_COLORS {
            rows.push((
                MenuItem::new(name).checked(current == hex),
                Action::TabColor(hex.to_string()),
            ));
        }
        rows.push((
            MenuItem::new("None").checked(current.is_empty()),
            Action::TabColor(String::new()),
        ));
        rows.into_iter().unzip()
    }

    pub(crate) fn open_menu(&mut self, which: usize) {
        self.context_tab = None;
        let (entries, actions) = self.menu_entries(which);
        self.menu_actions = actions;
        if open_menu(&mut self.ui, self.menubar, which, &entries, Msg::MenuPick).is_some() {
            if let Some(bar) = self.ui.widget_mut::<MenuBar<Msg>>(self.menubar) {
                bar.set_open(Some(which));
            }
            self.ui.invalidate(self.menubar);
        }
    }

    /// Drops the menu bar's highlight when a menu closes, however it closed.
    fn close_menu(&mut self) {
        self.ui.close_popup();
        if let Some(bar) = self.ui.widget_mut::<MenuBar<Msg>>(self.menubar) {
            if bar.open().is_some() {
                bar.set_open(None);
                self.ui.invalidate(self.menubar);
            }
        }
    }

    pub(crate) fn open_tab_menu(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        self.context_tab = Some(index);
        // Anchor the popup to the tab that was clicked. The widget's items are
        // copied out first so measuring can borrow the text engine.
        let strip = self.ui.bounds(self.strip);
        let bar = self
            .ui
            .widget::<TabBar<Msg>>(self.strip)
            .map(|b| (b.items().to_vec(), b.style()));
        let at = match (bar, strip) {
            (Some((items, style)), Some(bounds)) => {
                crate::tabbar::layout(&items, bounds, style, self.ui.text_mut())
                    .get(index)
                    .copied()
            }
            _ => None,
        };
        let Some(at) = at else { return };
        let (entries, actions) = self.tab_menu_entries();
        self.menu_actions = actions;
        let style = TextStyle::built_in(self.menu_px());
        open_menu_at(&mut self.ui, self.strip, at, &entries, style, Msg::MenuPick);
    }

    fn menu_px(&self) -> u16 {
        (13.0 * self.scale + 0.5) as u16
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
                label: tab.label.clone(),
                color: tab.color.clone(),
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
                v.set_word_wrap(new.word_wrap);
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

    // --- the update check ----------------------------------------------

    /// Asks GitHub on a thread of its own; the answer comes back through
    /// `updates_rx` on a later frame. `manual` is whether the reader asked.
    fn check_for_updates(&self, manual: bool) {
        let tx = self.updates_tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send((check_for_update(VERSION), manual));
        });
    }

    /// The launch-time check, when the setting allows and the interval has
    /// passed. Quiet unless there is something to say.
    fn maybe_check_for_updates(&self) {
        let settings = self.config.load_settings();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        if self.config.update_check_due(&settings, now) {
            self.config.set_last_update_check(now);
            self.check_for_updates(false);
        }
    }

    fn show_update_check(&self, check: UpdateCheck, manual: bool) {
        if let Some(error) = check.error {
            if manual {
                rfd::MessageDialog::new()
                    .set_level(rfd::MessageLevel::Warning)
                    .set_title("Update check failed")
                    .set_description(error)
                    .show();
            }
            return;
        }
        if check.update_available {
            let notes: String = check.notes.chars().take(500).collect();
            let answer = rfd::MessageDialog::new()
                .set_title(format!("Update available: {}", check.latest))
                .set_description(format!("You have {}.\n\n{notes}", check.current))
                .set_buttons(rfd::MessageButtons::OkCancelCustom(
                    "Download".into(),
                    "Later".into(),
                ))
                .show();
            if answer == rfd::MessageDialogResult::Custom("Download".into()) {
                open_url(&check.url);
            }
        } else if manual {
            rfd::MessageDialog::new()
                .set_title("You're up to date")
                .set_description(format!("ctail {} is the latest version.", check.current))
                .show();
        }
    }

    /// Carries out a chosen menu row.
    fn run(&mut self, action: Action) {
        match action {
            Action::None => {}
            Action::OpenFile => self.open_dialog(),
            Action::OpenRecent(path) => self.open(path),
            Action::ClearRecent => self.config.clear_recent_files(),
            Action::CloseTab => self.close_active(),
            Action::ReopenTab => self.reopen_closed(),
            Action::Quit => {
                self.persist();
                self.exit = true;
            }
            Action::Copy => self.copy_selection(),
            Action::SelectAll => {
                if let Some(view) = self.active_view() {
                    if let Some(v) = self.ui.widget_mut::<LogView<Msg>>(view) {
                        v.select_all();
                    }
                    self.ui.invalidate(view);
                }
            }
            Action::Find => self.open_search(),
            Action::ToggleLineNumbers => {
                let mut s = self.config.load_settings();
                s.show_line_numbers = !s.show_line_numbers;
                self.apply_settings(s);
            }
            Action::ToggleWordWrap => {
                let mut s = self.config.load_settings();
                s.word_wrap = !s.word_wrap;
                self.apply_settings(s);
            }
            Action::ToggleTheme => {
                let mut s = self.config.load_settings();
                s.theme_mode = if s.theme_mode == "light" {
                    "dark"
                } else {
                    "light"
                }
                .into();
                self.apply_settings(s);
            }
            Action::Profiles => self.open_profiles(),
            Action::Settings => self.open_settings(),
            Action::CheckUpdates => self.check_for_updates(true),
            Action::About => {
                rfd::MessageDialog::new()
                    .set_title("About ctail")
                    .set_description(format!(
                        "ctail {}\n\nLog viewer with real-time tailing and regex highlighting.",
                        env!("CARGO_PKG_VERSION")
                    ))
                    .show();
            }
            Action::TabRename => {
                let Some(index) = self.context_tab else {
                    return;
                };
                let Some(tab) = self.tabs.get(index) else {
                    return;
                };
                let initial = tab.name();
                self.renaming = Some(index);
                let tx = self.prompt_tx.clone();
                self.pending_windows.push(
                    WindowRequest::new(PromptWindow::config("Rename Tab"), move |size, scale| {
                        PromptWindow::new(size, scale, "Name for this tab".into(), initial, tx)
                    })
                    .with_modality(Modality::Modal),
                );
            }
            Action::TabRefresh => {
                if let Some(index) = self.context_tab {
                    if let Some(view) = self.tabs.get(index).map(|t| t.view) {
                        if let Some(v) = self.ui.widget_mut::<LogView<Msg>>(view) {
                            v.reset();
                        }
                        self.ui.invalidate(view);
                    }
                    if let Some(tab) = self.tabs.get(index) {
                        tab.tailer.refresh();
                    }
                }
            }
            Action::TabChangePath => self.change_tab_path(),
            Action::TabCopyPath => {
                let path = self
                    .context_tab
                    .and_then(|i| self.tabs.get(i))
                    .map(|t| t.path.clone());
                if let (Some(path), Some(cb)) = (path, self.clipboard.as_mut()) {
                    let _ = cb.set_text(path);
                }
            }
            Action::TabReveal => {
                if let Some(path) = self
                    .context_tab
                    .and_then(|i| self.tabs.get(i))
                    .map(|t| t.path.clone())
                {
                    reveal(&path);
                }
            }
            Action::TabClose => {
                if let Some(index) = self.context_tab {
                    self.activate(index);
                    self.close_active();
                }
            }
            Action::TabColor(hex) => {
                if let Some(tab) = self.context_tab.and_then(|i| self.tabs.get_mut(i)) {
                    tab.color = hex;
                }
                self.refresh_strip();
                self.persist();
            }
        }
    }

    fn active_view(&self) -> Option<NodeId> {
        self.tabs.get(self.active).map(|t| t.view)
    }

    fn copy_selection(&mut self) {
        let text = self
            .active_view()
            .and_then(|view| self.ui.widget::<LogView<Msg>>(view))
            .and_then(LogView::selected_text);
        if let (Some(text), Some(cb)) = (text, self.clipboard.as_mut()) {
            let _ = cb.set_text(text);
        }
    }

    /// Points a tab at a different file, keeping its place, label and colour.
    fn change_tab_path(&mut self) {
        let Some(index) = self.context_tab else {
            return;
        };
        let Some(current) = self.tabs.get(index).map(|t| t.path.clone()) else {
            return;
        };
        let mut dialog = rfd::FileDialog::new().set_title("Point this tab at a different file");
        if let Some(dir) = std::path::Path::new(&current).parent() {
            dialog = dialog.set_directory(dir);
        }
        let Some(picked) = dialog.pick_file() else {
            return;
        };
        let path = picked.to_string_lossy().into_owned();
        let (tx, rx) = mpsc::channel();
        let listener = Arc::new(Listener {
            tx: tx.clone(),
            counters: OnceLock::new(),
        });
        let tailer = Tailer::new(&path, self.tailer_options(), listener.clone());
        let _ = listener.counters.set(tailer.counters());
        let view = self.tabs[index].view;
        if let Some(v) = self.ui.widget_mut::<LogView<Msg>>(view) {
            v.reset();
        }
        self.ui.invalidate(view);
        let tab = &mut self.tabs[index];
        tab.tailer.stop();
        tab.path = path.clone();
        tab.tailer = tailer;
        tab.tx = tx;
        tab.rx = rx;
        tab.error = None;
        tab.tailer.start();
        self.config.add_recent_file(&path, 15);
        self.refresh_strip();
        self.activate(index);
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
                    TabEvent::Jump(lines, target) => v.show_range(lines, target),
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
                if on {
                    self.reattach_active();
                }
                if let Some(tab) = self.tabs.get(self.active) {
                    let view = tab.view;
                    if let Some(v) = self.ui.widget_mut::<LogView<Msg>>(view) {
                        v.set_follow(on);
                    }
                    self.ui.invalidate(view);
                }
            }
            Msg::Log(LogRequest::Reattach) => self.reattach_active(),
            Msg::Log(LogRequest::Follow(on)) => {
                if let Some(cb) = self.ui.widget_mut::<Checkbox<Msg>>(self.follow) {
                    cb.set_checked(on);
                }
                self.ui.invalidate(self.follow);
            }
            Msg::Log(LogRequest::Copy) => self.copy_selection(),
            Msg::CloseTab(i) => {
                if i < self.tabs.len() {
                    self.activate(i);
                    self.close_active();
                }
            }
            Msg::TabContext(i) => self.open_tab_menu(i),
            Msg::Menu(which) => self.open_menu(which),
            Msg::MenuPick(row) => {
                self.close_menu();
                let action = self.menu_actions.get(row).cloned().unwrap_or(Action::None);
                self.run(action);
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
                            // ⌥⌘W / Ctrl+Alt+W: the view toggle sits on the
                            // same letter as Close Tab, so it has to be
                            // matched first.
                            KeyCode::W if modifiers.contains(Modifiers::ALT) => {
                                self.run(Action::ToggleWordWrap);
                                continue;
                            }
                            KeyCode::W => {
                                self.close_active();
                                continue;
                            }
                            KeyCode::L if modifiers.contains(Modifiers::SHIFT) => {
                                self.run(Action::ToggleLineNumbers);
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
        while let Ok(answer) = self.prompt_rx.try_recv() {
            let index = self.renaming.take();
            if let (Some(index), Some(name)) = (index, answer) {
                if let Some(tab) = self.tabs.get_mut(index) {
                    // A name matching the file is no name at all, so the tab
                    // goes back to following the file if it is renamed later.
                    tab.label = if name == tab.file_name() {
                        String::new()
                    } else {
                        name
                    };
                }
                self.refresh_strip();
                self.persist();
                self.sync_chrome();
            }
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
        // A scan answers on its own thread; the counter reads the new count
        // back rather than being handed it.
        if self.scans.try_iter().count() > 0 {
            self.refresh_counter();
        }
        let answers: Vec<(UpdateCheck, bool)> = self.updates_rx.try_iter().collect();
        for (check, manual) in answers {
            self.show_update_check(check, manual);
        }
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
