//! The AI assistant window: ask a question about the log, or have a
//! highlighting profile written for it.
//!
//! A window of its own, like Settings and Profiles, so the platform places it
//! and gives it a title bar. Everything the assistant *is* — prompts,
//! providers, Copilot's sign-in — is the engine's (`ctail_core::ai`); this
//! window supplies the log text, keeps the Copilot token in the config
//! directory, and shows what comes back. Every engine call blocks on the
//! network, so each runs on a thread and answers through a channel the
//! window drains each frame.

use crate::theme;
use crate::widgets::TextBlock;
use ctail_core::ai::{self, copilot, AiError, AiMessage};
use ctail_core::{resolve_palette, AppSettings, ConfigStore, Profile};
use denise::{DamageTracker, ElementState, Frame, InputEvent, KeyCode, Rect, Role, Size};
use denise_text::TextStyle;
use denise_ui::widgets::{Align, Button, Label, TextInput};
use denise_ui::{NodeId, Ui};
use denise_winit::{DeniseApp, WindowConfig};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

pub const SIZE: Size = Size::new(640, 540);

/// How many lines of the log the model is shown when nothing is selected —
/// the macOS app's figure.
pub const CONTEXT_LINES: usize = 500;

/// What the window tells the main window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssistantEvent {
    /// A profile of that name was saved and should become the active one.
    ProfileGenerated(String),
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Msg {
    Ask,
    Generate,
    Copy,
}

/// What the reader asked for, kept so a Copilot sign-in can finish it.
#[derive(Clone, Debug)]
enum Task {
    Ask(String),
    Generate,
}

/// What a worker thread sends back.
enum Reply {
    Answer(Task, Result<String, AiError>),
    DeviceCode(Result<copilot::CopilotDeviceCode, AiError>),
    Token(Result<String, AiError>),
}

pub struct AssistantWindow {
    ui: Ui<Msg>,
    config: ConfigStore,
    settings: AppSettings,
    /// The log as it was when the window opened.
    log: String,
    question: NodeId,
    answer: NodeId,
    status: NodeId,
    ask_btn: NodeId,
    gen_btn: NodeId,
    tx: Sender<AssistantEvent>,
    reply_tx: Sender<Reply>,
    reply_rx: Receiver<Reply>,
    busy: bool,
    /// What to do once a Copilot sign-in has finished.
    after_sign_in: Option<Task>,
    clipboard: Option<arboard::Clipboard>,
    exit: bool,
}

impl AssistantWindow {
    pub fn config() -> WindowConfig {
        WindowConfig {
            title: "ctail — AI Assistant".into(),
            size: SIZE,
            resizable: false,
            frame_interval: Duration::from_nanos(1_000_000_000 / 60),
        }
    }

    pub fn new(size: Size, scale: f32, log: String, tx: Sender<AssistantEvent>) -> Self {
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
        let (w, h) = (size.width as i32, size.height as i32);
        let pad = s(14);

        // The answer takes the room; the controls sit under it.
        let answer = ui
            .add(
                root,
                TextBlock::new(mono),
                Rect::new(pad, pad, w - 2 * pad, h - s(14 + 30 + 10 + 30 + 14 + 10)),
            )
            .expect("answer");
        let row1 = h - s(14 + 30 + 10 + 30);
        let question = ui
            .add(
                root,
                TextInput::new()
                    .with_placeholder("Ask about the current log…")
                    .with_size(body)
                    .with_submit(Msg::Ask),
                Rect::new(pad, row1, w - 2 * pad - s(90), s(30)),
            )
            .expect("question");
        let ask_btn = ui
            .add(
                root,
                Button::new("Ask", Msg::Ask)
                    .with_role(Role::Primary)
                    .with_size(body),
                Rect::new(w - pad - s(80), row1, s(80), s(30)),
            )
            .expect("ask");
        let row2 = h - s(14 + 30);
        let gen_btn = ui
            .add(
                root,
                Button::new("Generate Rules Profile", Msg::Generate)
                    .with_role(Role::Neutral)
                    .with_size(body),
                Rect::new(pad, row2, s(190), s(30)),
            )
            .expect("generate");
        ui.add(
            root,
            Button::new("Copy Answer", Msg::Copy)
                .with_role(Role::Neutral)
                .with_size(body),
            Rect::new(pad + s(200), row2, s(120), s(30)),
        );
        let status = ui
            .add(
                root,
                Label::new("")
                    .with_size(px(11.0))
                    .with_align(Align::End, Align::Center),
                Rect::new(pad + s(330), row2, w - 2 * pad - s(330), s(30)),
            )
            .expect("status");
        ui.focus(Some(question));

        let (reply_tx, reply_rx) = mpsc::channel();
        let mut window = Self {
            ui,
            config,
            settings,
            log,
            question,
            answer,
            status,
            ask_btn,
            gen_btn,
            tx,
            reply_tx,
            reply_rx,
            busy: false,
            after_sign_in: None,
            clipboard: arboard::Clipboard::new().ok(),
            exit: false,
        };
        if window.settings.ai_provider.is_empty() {
            window.set_status("Configure an AI provider in Settings first.");
        }
        window
    }

    /// Puts an answer in the window without asking anyone, for a snapshot.
    pub fn debug_set_answer(&mut self, text: &str) {
        self.show(text);
    }

    fn show(&mut self, text: &str) {
        if let Some(block) = self.ui.widget_mut::<TextBlock>(self.answer) {
            block.set_text(text);
        }
        self.ui.invalidate(self.answer);
    }

    fn set_status(&mut self, text: &str) {
        if let Some(label) = self.ui.widget_mut::<Label>(self.status) {
            label.update(text);
        }
        self.ui.invalidate(self.status);
    }

    fn set_busy(&mut self, busy: bool, status: &str) {
        self.busy = busy;
        for id in [self.ask_btn, self.gen_btn] {
            self.ui.set_enabled(id, !busy);
            self.ui.invalidate(id);
        }
        self.set_status(status);
    }

    fn fail(&mut self, error: &AiError) {
        self.set_busy(false, "");
        self.show(&format!("Error: {error}"));
    }

    /// Runs `task` on a thread of its own; the answer comes back through
    /// `reply_rx` on a later frame.
    fn run(&mut self, task: Task) {
        let status = match task {
            Task::Ask(_) => "Asking…",
            Task::Generate => "Generating rules…",
        };
        self.set_busy(true, status);
        let messages: Vec<AiMessage> = match &task {
            Task::Ask(question) => ai::log_messages(&self.log, question),
            Task::Generate => ai::rule_gen_messages(&self.log),
        };
        let settings = self.settings.clone();
        let token = self.config.copilot_token();
        let tx = self.reply_tx.clone();
        std::thread::spawn(move || {
            let answer = ai::chat(&settings, token.as_deref(), &messages);
            let _ = tx.send(Reply::Answer(task, answer));
        });
    }

    fn answered(&mut self, task: Task, answer: Result<String, AiError>) {
        match answer {
            Err(AiError::NeedsCopilotAuth) => {
                // Not a failure but a detour: sign in, then finish the task.
                self.after_sign_in = Some(task);
                self.start_sign_in();
            }
            Err(e) => self.fail(&e),
            Ok(text) => {
                self.set_busy(false, "");
                match task {
                    Task::Ask(_) => self.show(&text),
                    Task::Generate => self.apply_rules(&text),
                }
            }
        }
    }

    /// Saves the generated rules as a profile and asks the main window to
    /// make it the active one.
    fn apply_rules(&mut self, text: &str) {
        let rules = ai::extract_rules(text).unwrap_or_default();
        if rules.is_empty() {
            self.show(&format!(
                "Could not parse rules from the AI response:\n\n{text}"
            ));
            return;
        }
        let name = format!("AI Generated {} rules", rules.len());
        let count = rules.len();
        self.config.save_profile(&Profile {
            name: name.clone(),
            rules,
        });
        let _ = self.tx.send(AssistantEvent::ProfileGenerated(name.clone()));
        self.show(&format!(
            "Created profile “{name}” with {count} rules and set it active."
        ));
    }

    fn start_sign_in(&mut self) {
        self.set_busy(true, "Requesting Copilot device code…");
        let tx = self.reply_tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Reply::DeviceCode(copilot::request_device_code()));
        });
    }

    fn got_device_code(&mut self, code: copilot::CopilotDeviceCode) {
        if let Some(clipboard) = self.clipboard.as_mut() {
            let _ = clipboard.set_text(code.user_code.clone());
        }
        self.show(&format!(
            "To use Copilot:\n\n1. Your code (copied to clipboard): {}\n2. A browser is opening {}\n3. Enter the code, then return here.\n\nWaiting for authorization…",
            code.user_code, code.verification_uri
        ));
        self.set_status("Waiting for GitHub…");
        crate::app::open_url(&code.verification_uri);
        let tx = self.reply_tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Reply::Token(copilot::poll_for_token(
                &code.device_code,
                code.interval,
            )));
        });
    }

    fn got_token(&mut self, token: String) {
        self.config.set_copilot_token(&token);
        match self.after_sign_in.take() {
            Some(task) => self.run(task),
            None => self.set_busy(false, "Signed in to Copilot."),
        }
    }

    fn drain_replies(&mut self) {
        let replies: Vec<Reply> = self.reply_rx.try_iter().collect();
        for reply in replies {
            match reply {
                Reply::Answer(task, answer) => self.answered(task, answer),
                Reply::DeviceCode(Ok(code)) => self.got_device_code(code),
                Reply::DeviceCode(Err(e)) => self.fail(&e),
                Reply::Token(Ok(token)) => self.got_token(token),
                Reply::Token(Err(e)) => self.fail(&e),
            }
        }
    }

    fn close(&mut self) {
        let _ = self.tx.send(AssistantEvent::Closed);
        self.exit = true;
    }
}

impl DeniseApp for AssistantWindow {
    fn update(&mut self, events: &[InputEvent], damage: &mut DamageTracker) {
        for event in events {
            match event {
                InputEvent::CloseRequested => self.close(),
                InputEvent::Key {
                    code: KeyCode::Escape,
                    state: ElementState::Down,
                    ..
                } => self.close(),
                _ => {}
            }
        }
        self.drain_replies();
        self.ui.handle(events);
        self.ui.tick(0);
        let messages: Vec<Msg> = self.ui.drain_messages().collect();
        for msg in messages {
            match msg {
                Msg::Ask if !self.busy => {
                    let question = self
                        .ui
                        .widget::<TextInput<Msg>>(self.question)
                        .map(|f| f.text().trim().to_string())
                        .unwrap_or_default();
                    if !question.is_empty() {
                        self.run(Task::Ask(question));
                    }
                }
                Msg::Generate if !self.busy => self.run(Task::Generate),
                Msg::Ask | Msg::Generate => {}
                Msg::Copy => {
                    let text = self
                        .ui
                        .widget::<TextBlock>(self.answer)
                        .map(|b| b.text().to_string())
                        .unwrap_or_default();
                    if let Some(clipboard) = self.clipboard.as_mut() {
                        let _ = clipboard.set_text(text);
                    }
                }
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

    /// While a thread is out asking, the window wakes often enough to notice
    /// the answer; otherwise only when the toolkit has something to animate.
    fn next_frame_in(&self) -> Option<Duration> {
        if self.busy {
            return Some(Duration::from_millis(100));
        }
        self.ui.next_wake_ms().map(Duration::from_millis)
    }
}
