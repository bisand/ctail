//! The Profiles & Rules window: choose a profile, edit its highlighting rules,
//! and see each one previewed against a sample line before saving.
//!
//! Its own window for the same reason the Settings one is: the platform's
//! title bar, close button and window management, rather than an imitation of
//! them drawn inside the log. Profile names are asked for in a modal child
//! window, and deleting asks through the platform's own message dialog.

use crate::prompt::PromptWindow;
use crate::theme;
use crate::widgets::{Preview, Swatch};
use ctail_core::{resolve_palette, ConfigStore, Profile, Rule};
use denise::{DamageTracker, ElementState, Frame, InputEvent, KeyCode, Rect, Role, Size};
use denise_text::TextStyle;
use denise_ui::widgets::{Align, Button, Checkbox, Label, List, ListItem, Select, TextInput};
use denise_ui::{NodeId, Ui};
use denise_winit::{DeniseApp, Modality, WindowConfig, WindowRequest};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

pub const SIZE: Size = Size::new(800, 540);

const SAMPLE: &str = "2026-09-03T12:00:00.000 ERROR worker-7 request id=42 failed";

/// The palette rule colours are picked from — the theme's own accent family,
/// which is what the built-in profile uses.
const SWATCHES: [&str; 8] = [
    "#ff6b6b", "#ffd93d", "#6bcbff", "#a6e3a1", "#cba6f7", "#f38ba8", "#ffffff", "#888888",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Foreground,
    Background,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Msg {
    OpenProfiles,
    ChoseProfile(usize),
    OpenTypes,
    ChoseType(usize),
    NewProfile,
    RenameProfile,
    DeleteProfile,
    SetActive,
    SelectRule(usize),
    AddRule,
    RemoveRule,
    MoveUp,
    MoveDown,
    Swatch(Target, usize),
    Bold(bool),
    Italic(bool),
    Enabled(bool),
    Save,
    Close,
}

/// Which name the prompt window is collecting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pending {
    New,
    Rename,
}

pub struct ProfilesWindow {
    ui: Ui<Msg>,
    config: ConfigStore,
    names: Vec<String>,
    profile: Profile,
    selected: Option<usize>,
    /// Set while a dropdown is open, so a chosen row reaches the right control.
    open_list: Option<Msg>,
    profile_sel: NodeId,
    rules_list: NodeId,
    name: NodeId,
    pattern: NodeId,
    pattern_error: NodeId,
    type_sel: NodeId,
    fg: NodeId,
    bg: NodeId,
    bold: NodeId,
    italic: NodeId,
    enabled: NodeId,
    preview: NodeId,
    fg_swatches: Vec<NodeId>,
    bg_swatches: Vec<NodeId>,
    /// Name prompts run in a modal child window.
    pending: Option<Pending>,
    prompt_tx: Sender<Option<String>>,
    prompt_rx: Receiver<Option<String>>,
    windows: Vec<WindowRequest>,
    dirty: bool,
    exit: bool,
    /// Told whenever the rules on disk changed, so the log restyles at once.
    changed: Sender<()>,
}

impl ProfilesWindow {
    pub fn config_window() -> WindowConfig {
        WindowConfig {
            title: "ctail — Profiles & Rules".into(),
            size: SIZE,
            resizable: false,
            frame_interval: Duration::from_nanos(1_000_000_000 / 60),
        }
    }

    pub fn new(size: Size, scale: f32, changed: Sender<()>) -> Self {
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
        let mut ui: Ui<Msg> = Ui::new(size, theme);
        if let Some((_, source)) = crate::fonts::load(crate::fonts::UI) {
            let id = ui.add_font(source);
            ui.set_default_font(id);
        }
        let mono = match crate::fonts::load(crate::fonts::MONO) {
            Some((_, source)) => TextStyle {
                font: ui.add_font(source),
                size_px: (12.0 * scale + 0.5) as u16,
            },
            None => TextStyle::built_in((12.0 * scale + 0.5) as u16),
        };
        let s = |v: i32| (v as f32 * scale + 0.5) as i32;
        let px = |v: f32| (v * scale + 0.5) as u16;
        let body = px(13.0);
        let root = ui.root();

        let names = config.list_profiles();
        let profile = config
            .load_profile(&settings.active_profile)
            .or_else(|| names.first().and_then(|n| config.load_profile(n)))
            .unwrap_or_default();

        let label = |ui: &mut Ui<Msg>, rect: Rect, text: &str| {
            ui.add(
                root,
                Label::new(text)
                    .with_size(body)
                    .with_align(Align::Start, Align::Center),
                rect,
            );
        };

        // --- top bar ---
        label(&mut ui, Rect::new(s(16), s(14), s(52), s(28)), "Profile");
        let profile_sel = ui
            .add(
                root,
                Select::new(names.clone(), Msg::OpenProfiles)
                    .with_selected(names.iter().position(|n| *n == profile.name))
                    .with_style(TextStyle::built_in(body)),
                Rect::new(s(72), s(14), s(220), s(28)),
            )
            .expect("profile select");
        let mut x = s(300);
        for (text, msg) in [
            ("New", Msg::NewProfile),
            ("Rename", Msg::RenameProfile),
            ("Delete", Msg::DeleteProfile),
            ("Set Active", Msg::SetActive),
        ] {
            let w = s(if text == "Set Active" { 90 } else { 70 });
            ui.add(
                root,
                Button::new(text, msg)
                    .with_size(body)
                    .with_role(Role::Neutral),
                Rect::new(x, s(14), w, s(28)),
            );
            x += w + s(6);
        }

        // --- left: the rule list and its buttons ---
        let rules_list = ui
            .add(
                root,
                List::new(rule_items(&profile.rules), Msg::SelectRule)
                    .with_style(TextStyle::built_in(body)),
                Rect::new(s(16), s(56), s(220), s(390)),
            )
            .expect("rules list");
        let mut x = s(16);
        for (text, msg) in [
            ("+", Msg::AddRule),
            ("\u{2212}", Msg::RemoveRule),
            ("\u{2191}", Msg::MoveUp),
            ("\u{2193}", Msg::MoveDown),
        ] {
            ui.add(
                root,
                Button::new(text, msg)
                    .with_size(body)
                    .with_role(Role::Neutral),
                Rect::new(x, s(454), s(52), s(28)),
            );
            x += s(56);
        }

        // --- right: the editor ---
        let ex = s(256);
        let ew = s(528);
        let mut y = 56;
        let mut row = |height: i32| {
            let r = (
                Rect::new(ex, s(y), s(96), s(height)),
                Rect::new(ex + s(100), s(y), ew - s(100), s(height)),
            );
            y += height + 8;
            r
        };
        let (l, c) = row(28);
        label(&mut ui, l, "Name");
        let name = ui
            .add(
                root,
                TextInput::new().with_size(body).with_submit(Msg::Save),
                c,
            )
            .expect("name");
        let (l, c) = row(28);
        label(&mut ui, l, "Pattern");
        let pattern = ui
            .add(
                root,
                TextInput::new().with_size(body).with_submit(Msg::Save),
                c,
            )
            .expect("pattern");
        let (_, c) = row(16);
        let pattern_error = ui
            .add(
                root,
                Label::new("")
                    .with_size(px(11.0))
                    .with_role(Role::Error)
                    .with_align(Align::Start, Align::Center),
                c,
            )
            .expect("pattern error");
        let (l, c) = row(28);
        label(&mut ui, l, "Type");
        let type_sel = ui
            .add(
                root,
                Select::new(["match", "line"], Msg::OpenTypes)
                    .with_selected(Some(0))
                    .with_style(TextStyle::built_in(body)),
                Rect::new(c.x, c.y, s(140), c.height),
            )
            .expect("type");

        let (l, c) = row(28);
        label(&mut ui, l, "Foreground");
        let fg = ui
            .add(
                root,
                TextInput::new().with_max_chars(9).with_size(body),
                Rect::new(c.x, c.y, s(96), c.height),
            )
            .expect("fg");
        let fg_swatches = swatch_row(&mut ui, root, c.x + s(104), c.y, scale, Target::Foreground);
        let (l, c) = row(28);
        label(&mut ui, l, "Background");
        let bg = ui
            .add(
                root,
                TextInput::new().with_max_chars(9).with_size(body),
                Rect::new(c.x, c.y, s(96), c.height),
            )
            .expect("bg");
        let bg_swatches = swatch_row(&mut ui, root, c.x + s(104), c.y, scale, Target::Background);

        let (l, _) = row(24);
        let mut cx = l.x;
        let mut check = |ui: &mut Ui<Msg>, text: &str, msg: fn(bool) -> Msg, w: i32| {
            let id = ui
                .add(
                    root,
                    Checkbox::new(text, msg).with_size(body),
                    Rect::new(cx, l.y, s(w), l.height),
                )
                .expect("check");
            cx += s(w + 8);
            id
        };
        let bold = check(&mut ui, "Bold", Msg::Bold, 84);
        let italic = check(&mut ui, "Italic", Msg::Italic, 90);
        let enabled = check(&mut ui, "Enabled", Msg::Enabled, 110);

        let (l, _) = row(18);
        label(&mut ui, l, "Preview");
        let (l, _) = row(30);
        let preview = ui
            .add(
                root,
                Preview::new(SAMPLE, mono),
                Rect::new(l.x, l.y, ew, l.height),
            )
            .expect("preview");

        // --- bottom buttons ---
        let h = size.height as i32;
        ui.add(
            root,
            Button::new("Close", Msg::Close)
                .with_role(Role::Neutral)
                .with_size(body),
            Rect::new(ex + ew - s(212), h - s(44), s(100), s(30)),
        );
        ui.add(
            root,
            Button::new("Save", Msg::Save)
                .with_role(Role::Primary)
                .with_size(body),
            Rect::new(ex + ew - s(100), h - s(44), s(100), s(30)),
        );

        let (prompt_tx, prompt_rx) = mpsc::channel();
        let mut window = Self {
            ui,
            config,
            names,
            profile,
            selected: None,
            open_list: None,
            profile_sel,
            rules_list,
            name,
            pattern,
            pattern_error,
            type_sel,
            fg,
            bg,
            bold,
            italic,
            enabled,
            preview,
            fg_swatches,
            bg_swatches,
            pending: None,
            prompt_tx,
            prompt_rx,
            windows: Vec::new(),
            dirty: false,
            exit: false,
            changed,
        };
        window.select_rule(0);
        window
    }

    // --- profile handling -------------------------------------------------

    fn reload_profiles(&mut self, select: Option<String>) {
        self.names = self.config.list_profiles();
        let wanted = select.unwrap_or_else(|| self.profile.name.clone());
        self.profile = self
            .config
            .load_profile(&wanted)
            .or_else(|| self.names.first().and_then(|n| self.config.load_profile(n)))
            .unwrap_or_default();
        let index = self.names.iter().position(|n| *n == self.profile.name);
        if let Some(sel) = self.ui.widget_mut::<Select<Msg>>(self.profile_sel) {
            sel.set_options(self.names.clone());
            sel.set_selected(index);
        }
        self.ui.invalidate(self.profile_sel);
        self.dirty = false;
        self.refresh_rules();
        self.select_rule(0);
    }

    fn refresh_rules(&mut self) {
        let items = rule_items(&self.profile.rules);
        let selected = self.selected;
        if let Some(list) = self.ui.widget_mut::<List<Msg>>(self.rules_list) {
            list.set_items(items);
            list.set_selected(selected);
        }
        self.ui.invalidate(self.rules_list);
    }

    /// Loads a rule into the editor. An empty profile clears it instead.
    fn select_rule(&mut self, index: usize) {
        self.selected = (index < self.profile.rules.len()).then_some(index);
        let rule = self
            .selected
            .and_then(|i| self.profile.rules.get(i))
            .cloned()
            .unwrap_or_default();
        set_text(&mut self.ui, self.name, &rule.name);
        set_text(&mut self.ui, self.pattern, &rule.pattern);
        set_text(&mut self.ui, self.fg, &rule.foreground);
        set_text(&mut self.ui, self.bg, &rule.background);
        if let Some(sel) = self.ui.widget_mut::<Select<Msg>>(self.type_sel) {
            sel.set_selected(Some(usize::from(rule.is_line_level())));
        }
        for (id, on) in [
            (self.bold, rule.bold),
            (self.italic, rule.italic),
            (self.enabled, rule.enabled),
        ] {
            if let Some(c) = self.ui.widget_mut::<Checkbox<Msg>>(id) {
                c.set_checked(on);
            }
            self.ui.invalidate(id);
        }
        if let Some(list) = self.ui.widget_mut::<List<Msg>>(self.rules_list) {
            list.set_selected(self.selected);
        }
        self.ui.invalidate(self.type_sel);
        self.ui.invalidate(self.rules_list);
        self.sync_editor();
    }

    /// Reads the editor's text fields back into the selected rule, then
    /// refreshes the preview, the pattern error and the swatch rings.
    fn commit_edits(&mut self) {
        let Some(index) = self.selected else { return };
        let name = text_of(&self.ui, self.name);
        let pattern = text_of(&self.ui, self.pattern);
        let fg = text_of(&self.ui, self.fg);
        let bg = text_of(&self.ui, self.bg);
        let Some(rule) = self.profile.rules.get_mut(index) else {
            return;
        };
        // Called every frame, so it must do nothing when nothing moved:
        // `sync_editor` compiles the pattern to check it, and that is not
        // something to do at frame rate.
        if rule.name == name
            && rule.pattern == pattern
            && rule.foreground == fg
            && rule.background == bg
        {
            return;
        }
        rule.name = name;
        rule.pattern = pattern;
        rule.foreground = fg;
        rule.background = bg;
        self.dirty = true;
        self.refresh_rules();
        self.sync_editor();
    }

    fn sync_editor(&mut self) {
        let rule = self
            .selected
            .and_then(|i| self.profile.rules.get(i))
            .cloned()
            .unwrap_or_default();
        let error = if rule.pattern.is_empty() {
            String::new()
        } else {
            ctail_core::highlight::validate_pattern(&rule.pattern).unwrap_or_default()
        };
        if let Some(label) = self.ui.widget_mut::<Label>(self.pattern_error) {
            if label.update(&error) {
                self.ui.invalidate(self.pattern_error);
            }
        }
        for (ids, chosen) in [
            (self.fg_swatches.clone(), rule.foreground.clone()),
            (self.bg_swatches.clone(), rule.background.clone()),
        ] {
            for (i, id) in ids.iter().enumerate() {
                let on = SWATCHES.get(i).is_some_and(|hex| *hex == chosen);
                if let Some(sw) = self.ui.widget_mut::<Swatch<Msg>>(*id) {
                    sw.set_selected(on);
                }
                self.ui.invalidate(*id);
            }
        }
        if let Some(preview) = self.ui.widget_mut::<Preview>(self.preview) {
            preview.set_rule(rule);
        }
        self.ui.invalidate(self.preview);
    }

    fn ask_name(&mut self, pending: Pending, caption: &str, initial: String) {
        self.pending = Some(pending);
        let tx = self.prompt_tx.clone();
        let caption = caption.to_string();
        let title = match pending {
            Pending::New => "New Profile",
            Pending::Rename => "Rename Profile",
        };
        self.windows.push(
            WindowRequest::new(PromptWindow::config(title), move |size, scale| {
                PromptWindow::new(size, scale, caption, initial, tx)
            })
            .with_modality(Modality::Modal),
        );
    }

    fn named(&mut self, name: String) {
        match self.pending.take() {
            Some(Pending::New) => {
                self.config.save_profile(&Profile {
                    name: name.clone(),
                    rules: Vec::new(),
                });
                self.reload_profiles(Some(name));
            }
            Some(Pending::Rename) => {
                let old = self.profile.name.clone();
                if self.config.rename_profile(&old, &name) {
                    self.reload_profiles(Some(name));
                }
            }
            None => {}
        }
    }

    fn delete_profile(&mut self) {
        if self.names.len() < 2 {
            return; // the last profile is what the log is highlighted with
        }
        let name = self.profile.name.clone();
        // The platform's own dialog, because a destructive question should look
        // like every other destructive question on the machine.
        let answer = rfd::MessageDialog::new()
            .set_title("Delete profile")
            .set_description(format!(
                "Delete the profile \"{name}\"? This cannot be undone."
            ))
            .set_buttons(rfd::MessageButtons::YesNo)
            .show();
        if answer == rfd::MessageDialogResult::Yes {
            self.config.delete_profile(&name);
            self.reload_profiles(None);
        }
    }

    fn save(&mut self) {
        self.commit_edits();
        self.config.save_profile(&self.profile);
        self.dirty = false;
        let _ = self.changed.send(());
    }
}

fn swatch_row(
    ui: &mut Ui<Msg>,
    root: NodeId,
    x: i32,
    y: i32,
    scale: f32,
    target: Target,
) -> Vec<NodeId> {
    let s = |v: i32| (v as f32 * scale + 0.5) as i32;
    SWATCHES
        .iter()
        .enumerate()
        .filter_map(|(i, hex)| {
            ui.add(
                root,
                Swatch::new(hex, Msg::Swatch(target, i)),
                Rect::new(x + s(i as i32 * 30), y + s(2), s(24), s(24)),
            )
        })
        .collect()
}

fn rule_items(rules: &[Rule]) -> Vec<ListItem> {
    rules
        .iter()
        .map(|r| {
            let name = if r.name.is_empty() {
                "(unnamed)"
            } else {
                &r.name
            };
            let item = ListItem::new(format!(
                "{}{name}",
                if r.enabled { "" } else { "\u{2298} " }
            ))
            .with_trailing(r.match_type.clone());
            if r.enabled {
                item
            } else {
                item.disabled()
            }
        })
        .collect()
}

fn set_text(ui: &mut Ui<Msg>, id: NodeId, text: &str) {
    if let Some(field) = ui.widget_mut::<TextInput<Msg>>(id) {
        field.set_text(text);
    }
    ui.invalidate(id);
}

fn text_of(ui: &Ui<Msg>, id: NodeId) -> String {
    ui.widget::<TextInput<Msg>>(id)
        .map(|f| f.text().to_string())
        .unwrap_or_default()
}

impl DeniseApp for ProfilesWindow {
    fn update(&mut self, events: &[InputEvent], damage: &mut DamageTracker) {
        for event in events {
            match event {
                InputEvent::CloseRequested
                | InputEvent::Key {
                    code: KeyCode::Escape,
                    state: ElementState::Down,
                    ..
                } => {
                    if self.dirty {
                        self.save();
                    }
                    let _ = self.changed.send(());
                    self.exit = true;
                }
                _ => {}
            }
        }
        while let Ok(answer) = self.prompt_rx.try_recv() {
            match answer {
                Some(name) => self.named(name),
                None => self.pending = None,
            }
        }
        self.ui.handle(events);
        self.ui.tick(0);
        let messages: Vec<Msg> = self.ui.drain_messages().collect();
        for msg in messages {
            match msg {
                Msg::OpenProfiles => {
                    self.open_list = Some(Msg::OpenProfiles);
                    denise_ui::widgets::open_select(
                        &mut self.ui,
                        self.profile_sel,
                        Msg::ChoseProfile,
                    );
                }
                Msg::ChoseProfile(i) => {
                    self.ui.close_popup();
                    if let Some(name) = self.names.get(i).cloned() {
                        if self.dirty {
                            self.config.save_profile(&self.profile);
                        }
                        self.reload_profiles(Some(name));
                    }
                }
                Msg::OpenTypes => {
                    self.open_list = Some(Msg::OpenTypes);
                    denise_ui::widgets::open_select(&mut self.ui, self.type_sel, Msg::ChoseType);
                }
                Msg::ChoseType(i) => {
                    self.ui.close_popup();
                    if let Some(rule) = self.selected.and_then(|s| self.profile.rules.get_mut(s)) {
                        rule.match_type = if i == 1 { "line" } else { "match" }.into();
                        self.dirty = true;
                    }
                    if let Some(sel) = self.ui.widget_mut::<Select<Msg>>(self.type_sel) {
                        sel.set_selected(Some(i));
                    }
                    self.ui.invalidate(self.type_sel);
                    self.refresh_rules();
                    self.sync_editor();
                }
                Msg::NewProfile => {
                    self.ask_name(Pending::New, "Name for the new profile", String::new())
                }
                Msg::RenameProfile => {
                    let current = self.profile.name.clone();
                    self.ask_name(Pending::Rename, "New name for this profile", current);
                }
                Msg::DeleteProfile => self.delete_profile(),
                Msg::SetActive => {
                    let mut s = self.config.load_settings();
                    s.active_profile = self.profile.name.clone();
                    self.config.save_settings(&s);
                    let _ = self.changed.send(());
                }
                Msg::SelectRule(i) => {
                    self.commit_edits();
                    self.select_rule(i);
                }
                Msg::AddRule => {
                    self.commit_edits();
                    let priority = self
                        .profile
                        .rules
                        .iter()
                        .map(|r| r.priority)
                        .max()
                        .unwrap_or(0)
                        + 10;
                    self.profile.rules.push(Rule {
                        id: format!("rule-{}", self.profile.rules.len() + 1),
                        name: "New Rule".into(),
                        priority,
                        ..Default::default()
                    });
                    self.dirty = true;
                    self.refresh_rules();
                    self.select_rule(self.profile.rules.len() - 1);
                }
                Msg::RemoveRule => {
                    if let Some(i) = self.selected {
                        self.profile.rules.remove(i);
                        self.dirty = true;
                        self.refresh_rules();
                        self.select_rule(i.saturating_sub(1));
                    }
                }
                Msg::MoveUp | Msg::MoveDown => {
                    self.commit_edits();
                    let up = msg == Msg::MoveUp;
                    if let Some(i) = self.selected {
                        let j = if up { i.wrapping_sub(1) } else { i + 1 };
                        if j < self.profile.rules.len() {
                            self.profile.rules.swap(i, j);
                            self.dirty = true;
                            self.refresh_rules();
                            self.select_rule(j);
                        }
                    }
                }
                Msg::Swatch(target, i) => {
                    let Some(hex) = SWATCHES.get(i) else { continue };
                    let id = match target {
                        Target::Foreground => self.fg,
                        Target::Background => self.bg,
                    };
                    // Clicking the colour that is already set clears it, which
                    // is the only way back to "no colour" without typing.
                    let current = text_of(&self.ui, id);
                    let next = if current == *hex { "" } else { *hex };
                    set_text(&mut self.ui, id, next);
                    self.commit_edits();
                }
                Msg::Bold(on) | Msg::Italic(on) | Msg::Enabled(on) => {
                    if let Some(rule) = self.selected.and_then(|s| self.profile.rules.get_mut(s)) {
                        match msg {
                            Msg::Bold(_) => rule.bold = on,
                            Msg::Italic(_) => rule.italic = on,
                            _ => rule.enabled = on,
                        }
                        self.dirty = true;
                    }
                    self.refresh_rules();
                    self.sync_editor();
                }
                Msg::Save => self.save(),
                Msg::Close => {
                    if self.dirty {
                        self.save();
                    }
                    let _ = self.changed.send(());
                    self.exit = true;
                }
            }
        }
        // Typing in a field reports nothing, so the editor is read back each
        // frame; `commit_edits` only marks anything when a value really moved.
        self.commit_edits();
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
        std::mem::take(&mut self.windows)
    }

    fn exit_requested(&self) -> bool {
        self.exit
    }

    fn next_frame_in(&self) -> Option<Duration> {
        self.ui.next_wake_ms().map(Duration::from_millis)
    }
}
