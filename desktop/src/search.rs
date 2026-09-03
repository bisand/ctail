//! The find bar: a query field, the three VS Code toggles, a filter toggle, a
//! match counter and prev/next/close. It owns its nodes and its toggle state;
//! the app owns what a query *means* and hands the result back through
//! [`SearchBar::set_counter`].

use denise::{Radius, Rect, Role};
use denise_ui::widgets::{Align, Button, Label, Panel, TextInput};
use denise_ui::{Anchors, NodeId, Ui};

/// The four sticky buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Toggle {
    /// Aa — match case.
    Case,
    /// W — whole word.
    Word,
    /// .* — the query is a regular expression.
    Regex,
    /// ≡ — show only matching lines.
    Filter,
}

/// What the app asks the bar to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchMsg {
    Toggle(Toggle),
    Next,
    Prev,
    Close,
}

/// What the match counter is showing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Counter {
    /// No query: the counter says nothing rather than "no results".
    Empty,
    /// A regex that does not compile — an honest zero would be useless.
    BadRegex,
    /// The file is still being scanned; the number is what the window holds.
    Scanning(usize),
    NoResults,
    /// 1-based position and the total. A position of 0 means the reader has
    /// not stepped to a match yet.
    At(usize, usize),
}

impl Counter {
    /// A count, reading as "no results" when there are none.
    pub fn at(current: usize, total: usize) -> Self {
        if total == 0 {
            Self::NoResults
        } else {
            Self::At(current, total)
        }
    }
}

/// Unscaled geometry; everything is multiplied by the display scale.
const HEIGHT: i32 = 36;
const PAD: i32 = 8;
const GAP: i32 = 4;
const FIELD_W: i32 = 210;
const COUNTER_W: i32 = 78;
const BTN_W: i32 = 30;
const INNER_H: i32 = 26;
const BUTTONS: usize = 7;

pub const WIDTH: i32 =
    PAD * 2 + FIELD_W + COUNTER_W + BTN_W * BUTTONS as i32 + GAP * (BUTTONS as i32 + 1);

pub struct SearchBar {
    panel: NodeId,
    field: NodeId,
    counter: NodeId,
    case_btn: NodeId,
    word_btn: NodeId,
    regex_btn: NodeId,
    filter_btn: NodeId,
    open: bool,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub is_regex: bool,
    pub filter: bool,
    /// Last text seen in the field, so typing is noticed without the widget
    /// having to report every keystroke.
    last_text: String,
    /// Last role written to the counter; the label reports its own text
    /// changes, this catches the colour ones.
    counter_role: Role,
}

impl SearchBar {
    /// Builds the bar, hidden, pinned to the top-right of `area`.
    pub fn install<M: Clone + 'static>(
        ui: &mut Ui<M>,
        parent: NodeId,
        area: Rect,
        scale: f32,
        to_msg: fn(SearchMsg) -> M,
        submit: M,
    ) -> Self {
        let s = |v: i32| (v as f32 * scale + 0.5) as i32;
        let px = |v: f32| (v * scale + 0.5) as u16;
        let width = s(WIDTH);
        let height = s(HEIGHT);
        let bar = Rect::new(
            area.x + area.width - width - s(12),
            area.y + s(8),
            width,
            height,
        );
        let panel = ui
            .add(
                parent,
                Panel {
                    fill: Some(Role::Base200),
                    border: Some(Role::Base300),
                    border_width: 1,
                    radius: Radius::Box,
                    backdrop: true, // clicks land on the bar, not the log behind it
                },
                bar,
            )
            .expect("search bar");
        // Above the log views, which are added as files are opened.
        ui.set_z(panel, 10);
        ui.set_anchors(
            panel,
            Anchors {
                left: false,
                top: true,
                right: true,
                bottom: false,
            },
        );

        let y = s((HEIGHT - INNER_H) / 2);
        let mut x = s(PAD);
        let field = ui
            .add(
                panel,
                TextInput::new()
                    .with_placeholder("Find")
                    .with_submit(submit)
                    .with_size(px(13.0)),
                Rect::new(x, y, s(FIELD_W), s(INNER_H)),
            )
            .expect("search field");
        x += s(FIELD_W + GAP);
        let counter = ui
            .add(
                panel,
                Label::new("")
                    .with_size(px(11.0))
                    .with_align(Align::End, Align::Center),
                Rect::new(x, y, s(COUNTER_W), s(INNER_H)),
            )
            .expect("counter");
        x += s(COUNTER_W + GAP);

        let button = |ui: &mut Ui<M>, label: &str, msg: M, x: i32| {
            ui.add(
                panel,
                // The field keeps focus so typing survives a click here.
                Button::new(label, msg)
                    .no_focus()
                    .with_role(Role::Neutral)
                    .with_size(px(11.0)),
                Rect::new(x, y, s(BTN_W), s(INNER_H)),
            )
            .expect("search button")
        };
        let case_btn = button(ui, "Aa", to_msg(SearchMsg::Toggle(Toggle::Case)), x);
        x += s(BTN_W + GAP);
        let word_btn = button(ui, "W", to_msg(SearchMsg::Toggle(Toggle::Word)), x);
        x += s(BTN_W + GAP);
        let regex_btn = button(ui, ".*", to_msg(SearchMsg::Toggle(Toggle::Regex)), x);
        x += s(BTN_W + GAP);
        let filter_btn = button(ui, "≡", to_msg(SearchMsg::Toggle(Toggle::Filter)), x);
        x += s(BTN_W + GAP);
        button(ui, "↑", to_msg(SearchMsg::Prev), x);
        x += s(BTN_W + GAP);
        button(ui, "↓", to_msg(SearchMsg::Next), x);
        x += s(BTN_W + GAP);
        button(ui, "\u{00d7}", to_msg(SearchMsg::Close), x);

        ui.set_visible(panel, false);
        Self {
            panel,
            field,
            counter,
            case_btn,
            word_btn,
            regex_btn,
            filter_btn,
            open: false,
            case_sensitive: false,
            whole_word: false,
            is_regex: false,
            filter: false,
            last_text: String::new(),
            counter_role: Role::BaseContent,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Puts a query in the field, as if it had been typed.
    pub fn set_query<M: Clone + 'static>(&mut self, ui: &mut Ui<M>, text: &str) {
        if let Some(field) = ui.widget_mut::<TextInput<M>>(self.field) {
            field.set_text(text);
        }
        self.last_text = text.to_string();
        ui.invalidate(self.field);
    }

    /// Shows the bar and puts the caret in the field, keeping whatever query
    /// was there so ⌘F twice in a row is not a reset.
    pub fn open<M: Clone + 'static>(&mut self, ui: &mut Ui<M>) {
        self.open = true;
        ui.set_visible(self.panel, true);
        ui.focus(Some(self.field));
        ui.invalidate(self.panel);
    }

    pub fn close<M: Clone + 'static>(&mut self, ui: &mut Ui<M>) {
        self.open = false;
        ui.set_visible(self.panel, false);
        ui.invalidate(self.panel);
    }

    /// The query as typed.
    pub fn query<M: Clone + 'static>(&self, ui: &Ui<M>) -> String {
        ui.widget::<TextInput<M>>(self.field)
            .map(|f| f.text().to_string())
            .unwrap_or_default()
    }

    /// Whether the field changed since the last call — the field reports on
    /// submit, not per keystroke, so typing is noticed by comparison.
    pub fn take_text_change<M: Clone + 'static>(&mut self, ui: &Ui<M>) -> Option<String> {
        let text = self.query(ui);
        (text != self.last_text).then(|| {
            self.last_text.clone_from(&text);
            text
        })
    }

    pub fn toggle<M: Clone + 'static>(&mut self, ui: &mut Ui<M>, which: Toggle) {
        let (flag, id) = match which {
            Toggle::Case => (&mut self.case_sensitive, self.case_btn),
            Toggle::Word => (&mut self.whole_word, self.word_btn),
            Toggle::Regex => (&mut self.is_regex, self.regex_btn),
            Toggle::Filter => (&mut self.filter, self.filter_btn),
        };
        *flag = !*flag;
        let role = if *flag { Role::Primary } else { Role::Neutral };
        if let Some(b) = ui.widget_mut::<Button<M>>(id) {
            b.set_role(role);
        }
        ui.invalidate(id);
    }

    /// What the counter says.
    pub fn set_counter<M: Clone + 'static>(&mut self, ui: &mut Ui<M>, counter: Counter) {
        let (text, role) = match counter {
            Counter::BadRegex => ("bad regex".to_string(), Role::Error),
            Counter::Empty => (String::new(), Role::BaseContent),
            // A file being scanned still has the window's matches to offer,
            // and the ellipsis says the number is not the whole story yet.
            Counter::Scanning(0) => ("scanning…".to_string(), Role::BaseContent),
            Counter::Scanning(window) => (format!("{window}+…"), Role::BaseContent),
            Counter::NoResults => ("No results".to_string(), Role::BaseContent),
            Counter::At(current, total) => (format!("{current}/{total}"), Role::BaseContent),
        };
        // Called every frame while the bar is open, so it must cost nothing
        // when nothing moved: only a real change earns a repaint.
        let role_changed = role != self.counter_role;
        self.counter_role = role;
        let mut changed = role_changed;
        if let Some(label) = ui.widget_mut::<Label>(self.counter) {
            changed |= label.update(&text);
            if role_changed {
                label.set_role(role);
            }
        }
        if changed {
            ui.invalidate(self.counter);
        }
    }
}
