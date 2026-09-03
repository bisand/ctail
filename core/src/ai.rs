//! The AI assistant's portable half: prompts, providers, the HTTP shapes of
//! the OpenAI-compatible and Anthropic APIs, GitHub Copilot's device-flow
//! sign-in, model listing, and the two local CLI tools as a backend.
//!
//! Ported from the Go app's `internal/ai` by way of the Swift port, and kept
//! here because none of it is about a window: a front end supplies the
//! settings, the Copilot token it has stored, and the log text, and gets back
//! the model's answer or a sentence saying why not. Every call here blocks;
//! a front end runs it on a thread of its own.
//!
//! What stays in a front end is small and rightly so: where the Copilot
//! token is kept, and how an answer is shown.

use crate::models::{AppSettings, Rule};
use crate::net;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Duration;

/// A chat turn, matching `ai.Message` in the Go app.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct AiMessage {
    pub role: String,
    pub content: String,
}

impl AiMessage {
    pub fn new(role: &str, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
        }
    }
}

/// Why an answer did not come. `NeedsCopilotAuth` is its own case because a
/// front end answers it by starting a sign-in rather than by showing it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "ffi", derive(uniffi::Error))]
#[cfg_attr(feature = "ffi", uniffi(flat_error))]
pub enum AiError {
    Message(String),
    NeedsCopilotAuth,
}

impl fmt::Display for AiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AiError::Message(m) => f.write_str(m),
            AiError::NeedsCopilotAuth => f.write_str("Sign in to GitHub Copilot first."),
        }
    }
}

impl std::error::Error for AiError {}

fn err<T>(message: impl Into<String>) -> Result<T, AiError> {
    Err(AiError::Message(message.into()))
}

const CHAT_TIMEOUT: Duration = Duration::from_secs(60);
const LIST_TIMEOUT: Duration = Duration::from_secs(30);

// --- providers --------------------------------------------------------------

/// Providers reached over HTTP.
pub const API_PROVIDERS: [&str; 5] = ["openai", "github", "copilot", "anthropic", "custom"];
/// Providers that run a locally installed command. Refused in a sandbox.
pub const CLI_PROVIDERS: [&str; 2] = ["claude-cli", "codex-cli"];

pub fn default_endpoint(provider: &str) -> String {
    match provider {
        "openai" => "https://api.openai.com",
        "github" => "https://models.inference.ai.azure.com",
        "copilot" => "https://api.githubcopilot.com",
        "anthropic" => "https://api.anthropic.com",
        _ => "",
    }
    .into()
}

pub fn default_model(provider: &str) -> String {
    match provider {
        "anthropic" => "claude-sonnet-4-6",
        // The CLI picks its own default.
        "claude-cli" | "codex-cli" => "",
        _ => "gpt-4o-mini",
    }
    .into()
}

/// Inside the macOS App Sandbox (the App Store build), where spawning an
/// executable is not allowed, so the CLI providers are refused and hidden.
pub fn is_sandboxed() -> bool {
    std::env::var_os("APP_SANDBOX_CONTAINER_ID").is_some()
}

fn trimmed(endpoint: &str) -> &str {
    endpoint.trim_end_matches('/')
}

fn is_github_style(base: &str) -> bool {
    // Copilot and GitHub Models expose their routes without a /v1.
    base.contains("githubcopilot")
        || base.contains("models.inference")
        || base.contains("models.github")
}

/// The chat-completions URL for an OpenAI-compatible base (mirrors
/// `client.go`'s `completionsURL`). A URL that is already the full route is
/// left alone, so a user who pasted one is not second-guessed.
pub fn completions_url(endpoint: &str) -> String {
    let base = trimmed(endpoint);
    if base.ends_with("/chat/completions") {
        return base.into();
    }
    if base.ends_with("/v1") || is_github_style(base) {
        return format!("{base}/chat/completions");
    }
    format!("{base}/v1/chat/completions")
}

/// The Messages URL for an Anthropic base.
pub fn messages_url(endpoint: &str) -> String {
    let base = trimmed(endpoint);
    if base.ends_with("/v1/messages") {
        return base.into();
    }
    if base.ends_with("/v1") {
        return format!("{base}/messages");
    }
    format!("{base}/v1/messages")
}

/// The model-list URL for an OpenAI-compatible base.
pub fn models_url(endpoint: &str) -> String {
    let base = trimmed(endpoint);
    if base.ends_with("/models") {
        return base.into();
    }
    if base.ends_with("/v1") || is_github_style(base) {
        return format!("{base}/models");
    }
    format!("{base}/v1/models")
}

// --- prompts ----------------------------------------------------------------

pub const SYSTEM_LOG_ANALYSIS: &str =
    "You are a log analysis assistant integrated into ctail, a log file viewer.
The user will provide log file content and ask questions about it.

Guidelines:
- Be concise and direct in your answers.
- When identifying issues, reference specific line numbers or patterns.
- If you see errors, warnings, or anomalies, highlight them.
- Suggest possible root causes when appropriate.
- Format your response in plain text (no markdown), keeping it readable in a desktop app.";

pub const SYSTEM_RULE_GENERATION: &str = "You are a highlighting rule generator for ctail, a log file viewer.
Analyze the provided log content and generate highlighting rules that help the user visually parse the logs.

Each rule is a JSON object with these fields:
  id          - unique kebab-case identifier (e.g. \"error-line\", \"http-status\")
  name        - short human-readable name (e.g. \"Error\", \"HTTP Status\")
  pattern     - regex pattern (RE2/ICU compatible)
  matchType   - \"line\" (highlight entire line) or \"match\" (highlight matched text only)
  foreground  - hex color for text (e.g. \"#ff6b6b\"), or \"\" for default
  background  - hex color for background (e.g. \"#3d1f1f\"), or \"\" for transparent
  bold        - boolean
  italic      - boolean
  enabled     - always true
  priority    - integer, higher = takes precedence (use 0-200 range)

Important:
- Use (?i) for case-insensitive matching where appropriate.
- Choose contrasting, readable colors. Use background sparingly (for important items like errors).
- Generate 5-15 rules covering the main patterns visible in the logs.
- Return ONLY a JSON array of rule objects. No explanation, no wrapping, no markdown fences.";

/// The conversation that asks `question` about `log_content`.
pub fn log_messages(log_content: &str, question: &str) -> Vec<AiMessage> {
    vec![
        AiMessage::new("system", SYSTEM_LOG_ANALYSIS),
        AiMessage::new(
            "user",
            format!("Here are the log contents:\n\n{log_content}\n\nQuestion: {question}"),
        ),
    ]
}

/// The conversation that asks for highlighting rules for `log_content`.
pub fn rule_gen_messages(log_content: &str) -> Vec<AiMessage> {
    vec![
        AiMessage::new("system", SYSTEM_RULE_GENERATION),
        AiMessage::new(
            "user",
            format!("Analyze these logs and generate highlighting rules:\n\n{log_content}"),
        ),
    ]
}

/// The rules in a model's answer to [`rule_gen_messages`], tolerating the
/// prose and code fences models wrap a JSON array in despite being asked
/// not to. `None` when there is no array to be found.
pub fn extract_rules(text: &str) -> Option<Vec<Rule>> {
    let start = text.find('[')?;
    let end = text.rfind(']')?;
    if end < start {
        return None;
    }
    serde_json::from_str(&text[start..=end]).ok()
}

// --- chat -------------------------------------------------------------------

/// Asks the provider chosen in `settings`. `copilot_oauth` is the GitHub
/// token a front end stored after a device-flow sign-in; without one, the
/// Copilot provider answers [`AiError::NeedsCopilotAuth`].
pub fn chat(
    settings: &AppSettings,
    copilot_oauth: Option<&str>,
    messages: &[AiMessage],
) -> Result<String, AiError> {
    let provider = settings.ai_provider.as_str();
    let model = if settings.ai_model.is_empty() {
        default_model(provider)
    } else {
        settings.ai_model.clone()
    };
    let endpoint = || {
        if settings.ai_endpoint.is_empty() {
            default_endpoint(provider)
        } else {
            settings.ai_endpoint.clone()
        }
    };
    match provider {
        "anthropic" => chat_anthropic(&endpoint(), &settings.ai_key, &model, messages),
        "claude-cli" | "codex-cli" => {
            if is_sandboxed() {
                return err("CLI tools aren't available in the App Store build. Choose an API provider, or use the direct-download build of ctail.");
            }
            chat_cli(provider, &model, messages)
        }
        "copilot" => {
            let oauth = copilot_oauth.ok_or(AiError::NeedsCopilotAuth)?;
            let token = copilot::exchange_token(oauth)?;
            let headers = copilot::editor_headers();
            let extra: Vec<(&str, &str)> = headers.iter().map(|(k, v)| (*k, *v)).collect();
            chat_openai(&endpoint(), &token, &model, &extra, messages)
        }
        _ => chat_openai(&endpoint(), &settings.ai_key, &model, &[], messages),
    }
}

/// One turn against an OpenAI-compatible server: OpenAI, GitHub Models,
/// Copilot (with its editor headers in `extra_headers`), Ollama, LM Studio…
pub fn chat_openai(
    endpoint: &str,
    api_key: &str,
    model: &str,
    extra_headers: &[(&str, &str)],
    messages: &[AiMessage],
) -> Result<String, AiError> {
    #[derive(Serialize)]
    struct Request<'a> {
        model: &'a str,
        messages: &'a [AiMessage],
    }
    let body = serde_json::to_string(&Request { model, messages })
        .map_err(|e| AiError::Message(e.to_string()))?;
    let auth = format!("Bearer {api_key}");
    let mut headers: Vec<(&str, &str)> = Vec::new();
    if !api_key.is_empty() {
        headers.push(("Authorization", &auth));
    }
    headers.extend_from_slice(extra_headers);
    let reply = net::post_json(&completions_url(endpoint), &headers, &body, CHAT_TIMEOUT)
        .map_err(AiError::Message)?;
    parse_openai_reply(reply.status, &reply.body)
}

fn parse_openai_reply(status: u16, body: &str) -> Result<String, AiError> {
    #[derive(Deserialize)]
    struct Message {
        content: String,
    }
    #[derive(Deserialize)]
    struct Choice {
        message: Message,
    }
    #[derive(Deserialize)]
    struct ErrorBody {
        message: String,
    }
    #[derive(Deserialize)]
    struct Response {
        choices: Option<Vec<Choice>>,
        error: Option<ErrorBody>,
    }
    let parsed: Option<Response> = serde_json::from_str(body).ok();
    // An API's own error message beats an HTTP status.
    if let Some(message) = parsed
        .as_ref()
        .and_then(|p| p.error.as_ref())
        .map(|e| &e.message)
    {
        return err(message.clone());
    }
    if status != 200 {
        return err(format!(
            "AI request failed (HTTP {status}): {}",
            excerpt(body, 300)
        ));
    }
    parsed
        .and_then(|p| p.choices)
        .and_then(|c| c.into_iter().next())
        .map(|c| c.message.content)
        .ok_or_else(|| AiError::Message("Empty AI response".into()))
}

/// One turn against the Anthropic Messages API, where the system prompt is a
/// field of its own, the key travels as `x-api-key`, and `max_tokens` is
/// required.
pub fn chat_anthropic(
    endpoint: &str,
    api_key: &str,
    model: &str,
    messages: &[AiMessage],
) -> Result<String, AiError> {
    #[derive(Serialize)]
    struct Request<'a> {
        model: &'a str,
        max_tokens: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        system: Option<String>,
        messages: Vec<&'a AiMessage>,
    }
    let (system, turns) = split_system(messages);
    let body = serde_json::to_string(&Request {
        model,
        max_tokens: 2048,
        system: (!system.is_empty()).then_some(system),
        messages: turns,
    })
    .map_err(|e| AiError::Message(e.to_string()))?;
    let reply = net::post_json(
        &messages_url(endpoint),
        &[("x-api-key", api_key), ("anthropic-version", "2023-06-01")],
        &body,
        CHAT_TIMEOUT,
    )
    .map_err(AiError::Message)?;
    parse_anthropic_reply(reply.status, &reply.body)
}

fn parse_anthropic_reply(status: u16, body: &str) -> Result<String, AiError> {
    #[derive(Deserialize)]
    struct Block {
        text: Option<String>,
    }
    #[derive(Deserialize)]
    struct ErrorBody {
        message: String,
    }
    #[derive(Deserialize)]
    struct Response {
        content: Option<Vec<Block>>,
        error: Option<ErrorBody>,
    }
    let parsed: Option<Response> = serde_json::from_str(body).ok();
    if let Some(message) = parsed
        .as_ref()
        .and_then(|p| p.error.as_ref())
        .map(|e| &e.message)
    {
        return err(message.clone());
    }
    if status != 200 {
        return err(format!(
            "Anthropic request failed (HTTP {status}): {}",
            excerpt(body, 300)
        ));
    }
    let text: String = parsed
        .and_then(|p| p.content)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|b| b.text)
        .collect();
    if text.is_empty() {
        return err("Empty AI response");
    }
    Ok(text)
}

/// The system prompt(s) joined, and the turns that are not system prompts.
fn split_system(messages: &[AiMessage]) -> (String, Vec<&AiMessage>) {
    let system = messages
        .iter()
        .filter(|m| m.role == "system")
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    let turns = messages.iter().filter(|m| m.role != "system").collect();
    (system, turns)
}

/// The first `max` characters, for an error message that quotes a body.
fn excerpt(body: &str, max: usize) -> String {
    body.chars().take(max).collect()
}

// --- the CLI tools ------------------------------------------------------------

/// The chat as one prompt for a tool that takes its input on stdin: system
/// guidance first, then the turns.
pub fn combined_prompt(messages: &[AiMessage]) -> String {
    let (system, turns) = split_system(messages);
    let rest = turns
        .iter()
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    if system.is_empty() {
        rest
    } else {
        format!("{system}\n\n{rest}")
    }
}

/// The command a CLI provider runs, and its non-interactive arguments.
pub fn cli_command(provider: &str, model: &str) -> (String, Vec<String>) {
    let (binary, mut args) = match provider {
        "codex-cli" => ("codex", vec!["exec".to_string()]),
        _ => ("claude", vec!["-p".to_string()]),
    };
    if !model.is_empty() {
        args.push("--model".into());
        args.push(model.into());
    }
    (binary.into(), args)
}

/// Runs the provider's command with the prompt on stdin and answers with its
/// stdout.
pub fn chat_cli(provider: &str, model: &str, messages: &[AiMessage]) -> Result<String, AiError> {
    use std::io::Write as _;
    use std::process::{Command, Stdio};

    let (binary, args) = cli_command(provider, model);
    let Some(path) = resolve_binary(&binary) else {
        return err(format!(
            "`{binary}` not found. Install it and make sure it's on your PATH."
        ));
    };
    let mut child = Command::new(path)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AiError::Message(format!("Failed to launch `{binary}`: {e}")))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(combined_prompt(messages).as_bytes());
    }
    let output = child
        .wait_with_output()
        .map_err(|e| AiError::Message(format!("`{binary}` failed: {e}")))?;
    let out = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = excerpt(
            if stderr.trim().is_empty() {
                &out
            } else {
                &stderr
            },
            300,
        );
        let code = output.status.code().unwrap_or(-1);
        return err(format!("`{binary}` failed (exit {code}): {detail}"));
    }
    if out.is_empty() {
        return err(format!("`{binary}` returned no output"));
    }
    Ok(out)
}

/// Where a CLI tool is. A GUI app started from a launcher inherits a bare
/// PATH, so the usual install locations are tried first, and on Unix the
/// login shell is asked last — it has the PATH the user actually set up.
fn resolve_binary(binary: &str) -> Option<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    let dirs = [
        "/opt/homebrew/bin".to_string(),
        "/usr/local/bin".into(),
        "/usr/bin".into(),
        format!("{home}/.local/bin"),
        format!("{home}/.npm-global/bin"),
        format!("{home}/.bun/bin"),
    ];
    for dir in &dirs {
        let candidate = std::path::Path::new(dir).join(binary);
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().into_owned());
        }
    }
    if cfg!(windows) {
        // CreateProcess searches PATH itself, and there is no login shell.
        return Some(binary.to_string());
    }
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    let output = std::process::Command::new(shell)
        .args(["-lc", &format!("command -v {binary}")])
        .output()
        .ok()?;
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!path.is_empty()).then_some(path)
}

// --- model listing ------------------------------------------------------------

/// The Claude model IDs for the `claude` CLI, which has no list endpoint.
pub const CLAUDE_CLI_MODELS: [&str; 3] = [
    "claude-opus-4-8",
    "claude-sonnet-4-6",
    "claude-haiku-4-5-20251001",
];

/// The models the provider in `settings` offers, sorted, for a picker.
pub fn list_models(
    settings: &AppSettings,
    copilot_oauth: Option<&str>,
) -> Result<Vec<String>, AiError> {
    let provider = settings.ai_provider.as_str();
    match provider {
        "claude-cli" => return Ok(CLAUDE_CLI_MODELS.iter().map(|m| m.to_string()).collect()),
        "codex-cli" => return err("The codex CLI has no model list — type the model name."),
        _ => {}
    }
    let base = if settings.ai_endpoint.is_empty() {
        default_endpoint(provider)
    } else {
        settings.ai_endpoint.clone()
    };
    let reply = match provider {
        "anthropic" => net::get(
            &format!("{}/v1/models", trimmed(&base)),
            &[
                ("x-api-key", &settings.ai_key),
                ("anthropic-version", "2023-06-01"),
            ],
            LIST_TIMEOUT,
        ),
        "copilot" => {
            let oauth = copilot_oauth.ok_or(AiError::NeedsCopilotAuth)?;
            let token = copilot::exchange_token(oauth)?;
            let auth = format!("Bearer {token}");
            let mut headers = copilot::editor_headers();
            headers.push(("Authorization", &auth));
            net::get(
                "https://api.githubcopilot.com/models",
                &headers,
                LIST_TIMEOUT,
            )
        }
        _ => {
            if base.is_empty() {
                return err("Set an endpoint first.");
            }
            let auth = format!("Bearer {}", settings.ai_key);
            let headers: Vec<(&str, &str)> = if settings.ai_key.is_empty() {
                Vec::new()
            } else {
                vec![("Authorization", &auth)]
            };
            net::get(&models_url(&base), &headers, LIST_TIMEOUT)
        }
    }
    .map_err(AiError::Message)?;
    if reply.status != 200 {
        return err(format!(
            "Couldn't list models (HTTP {}): {}",
            reply.status,
            excerpt(&reply.body, 200)
        ));
    }
    let mut ids = parse_models(&reply.body);
    if ids.is_empty() {
        return err("No models returned");
    }
    ids.sort();
    Ok(ids)
}

/// Model IDs out of either shape a `/models` route answers with: OpenAI's
/// `{"data": [...]}` or a bare array, each entry named by `id` or `name`.
fn parse_models(json: &str) -> Vec<String> {
    #[derive(Deserialize)]
    struct Entry {
        id: Option<String>,
        name: Option<String>,
    }
    #[derive(Deserialize)]
    struct Listing {
        data: Option<Vec<Entry>>,
    }
    let entries = match serde_json::from_str::<Listing>(json) {
        Ok(Listing {
            data: Some(entries),
        }) => entries,
        _ => serde_json::from_str::<Vec<Entry>>(json).unwrap_or_default(),
    };
    entries
        .into_iter()
        .filter_map(|e| e.id.or(e.name))
        .collect()
}

// --- GitHub Copilot -------------------------------------------------------------

/// GitHub Copilot's device-flow OAuth: ask for a code, have the user enter it
/// at github.com, poll for the OAuth token, and exchange that for a
/// short-lived API token before each request. Where the OAuth token is kept
/// between runs is the front end's business.
pub mod copilot {
    use super::{err, net, AiError};
    use std::time::Duration;

    /// The public Copilot editor client id.
    pub const CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";
    pub const DEVICE_CODE_ENDPOINT: &str = "https://github.com/login/device/code";
    pub const POLL_ENDPOINT: &str = "https://github.com/login/oauth/access_token";
    pub const EXCHANGE_ENDPOINT: &str = "https://api.github.com/copilot_internal/v2/token";

    /// The headers Copilot expects an editor to send.
    pub fn editor_headers() -> Vec<(&'static str, &'static str)> {
        vec![
            ("Editor-Version", "vscode/1.100.0"),
            ("Editor-Plugin-Version", "copilot/1.300.0"),
            ("User-Agent", "GithubCopilot/1.300.0"),
            ("Copilot-Integration-Id", "vscode-chat"),
        ]
    }

    /// What GitHub hands out at the start of a sign-in.
    #[derive(Clone, Debug, PartialEq, Eq)]
    #[cfg_attr(feature = "ffi", derive(uniffi::Record))]
    pub struct CopilotDeviceCode {
        pub device_code: String,
        /// What the user types in at `verification_uri`.
        pub user_code: String,
        pub verification_uri: String,
        /// Seconds between polls, as GitHub asks.
        pub interval: u32,
    }

    pub fn request_device_code() -> Result<CopilotDeviceCode, AiError> {
        request_device_code_at(DEVICE_CODE_ENDPOINT)
    }

    pub fn request_device_code_at(endpoint: &str) -> Result<CopilotDeviceCode, AiError> {
        let reply = post_form(
            endpoint,
            &[("client_id", CLIENT_ID), ("scope", "read:user")],
        )?;
        let field = |name: &str| reply.get(name).and_then(|v| v.as_str()).map(str::to_string);
        match (
            field("device_code"),
            field("user_code"),
            field("verification_uri"),
        ) {
            (Some(device_code), Some(user_code), Some(verification_uri)) => Ok(CopilotDeviceCode {
                device_code,
                user_code,
                verification_uri,
                interval: reply.get("interval").and_then(|i| i.as_u64()).unwrap_or(5) as u32,
            }),
            _ => err("Unexpected device-code response"),
        }
    }

    /// Waits for the user to authorise, and answers with the OAuth token.
    /// Honours GitHub's `authorization_pending` and `slow_down`; blocks for
    /// as long as the user takes, so call it on a thread of its own. The
    /// first poll is immediate — the user is still reading the code — and
    /// every later one waits `interval` seconds, as GitHub asks.
    pub fn poll_for_token(device_code: &str, interval: u32) -> Result<String, AiError> {
        poll_for_token_at(POLL_ENDPOINT, device_code, interval)
    }

    pub fn poll_for_token_at(
        endpoint: &str,
        device_code: &str,
        interval: u32,
    ) -> Result<String, AiError> {
        let mut wait = interval.max(5) as u64;
        loop {
            let reply = post_form(
                endpoint,
                &[
                    ("client_id", CLIENT_ID),
                    ("device_code", device_code),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ],
            )?;
            if let Some(token) = reply.get("access_token").and_then(|t| t.as_str()) {
                return Ok(token.to_string());
            }
            match reply.get("error").and_then(|e| e.as_str()) {
                Some("authorization_pending") | None => {}
                Some("slow_down") => wait += 5,
                Some("expired_token") => return err("Code expired — try again"),
                Some(other) => return err(other.to_string()),
            }
            std::thread::sleep(Duration::from_secs(wait));
        }
    }

    /// Trades the OAuth token for the short-lived token the API wants.
    pub fn exchange_token(oauth: &str) -> Result<String, AiError> {
        exchange_token_at(EXCHANGE_ENDPOINT, oauth)
    }

    pub fn exchange_token_at(endpoint: &str, oauth: &str) -> Result<String, AiError> {
        let auth = format!("token {oauth}");
        let mut headers = editor_headers();
        headers.push(("Authorization", &auth));
        headers.push(("Accept", "application/json"));
        let reply =
            net::get(endpoint, &headers, Duration::from_secs(30)).map_err(AiError::Message)?;
        let token = serde_json::from_str::<serde_json::Value>(&reply.body)
            .ok()
            .and_then(|v| v.get("token").and_then(|t| t.as_str()).map(str::to_string))
            .filter(|t| !t.is_empty());
        token.ok_or_else(|| AiError::Message("Copilot token exchange failed".into()))
    }

    /// GitHub's OAuth endpoints take a JSON object and answer with one.
    fn post_form(endpoint: &str, params: &[(&str, &str)]) -> Result<serde_json::Value, AiError> {
        let body: serde_json::Map<String, serde_json::Value> = params
            .iter()
            .map(|(k, v)| (k.to_string(), serde_json::Value::String(v.to_string())))
            .collect();
        let reply = net::post_json(
            endpoint,
            &[("Accept", "application/json")],
            &serde_json::Value::Object(body).to_string(),
            Duration::from_secs(30),
        )
        .map_err(AiError::Message)?;
        serde_json::from_str(&reply.body).map_err(|_| AiError::Message("Invalid response".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_are_completed_but_never_second_guessed() {
        assert_eq!(
            completions_url("https://api.openai.com"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            completions_url("https://api.githubcopilot.com"),
            "https://api.githubcopilot.com/chat/completions"
        );
        assert_eq!(
            completions_url("http://localhost:11434/v1"),
            "http://localhost:11434/v1/chat/completions"
        );
        assert_eq!(
            completions_url("https://x/v1/chat/completions"),
            "https://x/v1/chat/completions"
        );
        assert_eq!(
            completions_url("https://x/v1/chat/completions/"),
            "https://x/v1/chat/completions"
        );

        assert_eq!(
            messages_url("https://api.anthropic.com"),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(
            messages_url("https://proxy/v1"),
            "https://proxy/v1/messages"
        );
        assert_eq!(
            messages_url("https://proxy/v1/messages"),
            "https://proxy/v1/messages"
        );

        assert_eq!(
            models_url("https://api.openai.com"),
            "https://api.openai.com/v1/models"
        );
        assert_eq!(
            models_url("http://localhost:11434/v1"),
            "http://localhost:11434/v1/models"
        );
        assert_eq!(
            models_url("https://models.inference.ai.azure.com"),
            "https://models.inference.ai.azure.com/models"
        );
        assert_eq!(models_url("https://x/v1/models"), "https://x/v1/models");
    }

    #[test]
    fn providers_have_the_go_apps_defaults() {
        assert_eq!(default_endpoint("openai"), "https://api.openai.com");
        assert_eq!(default_endpoint("anthropic"), "https://api.anthropic.com");
        assert_eq!(default_endpoint("custom"), "");
        assert_eq!(default_model("anthropic"), "claude-sonnet-4-6");
        assert_eq!(default_model("claude-cli"), "");
        assert_eq!(default_model("openai"), "gpt-4o-mini");
    }

    #[test]
    fn a_cli_prompt_puts_the_system_guidance_first() {
        let messages = [
            AiMessage::new("system", "SYS"),
            AiMessage::new("user", "USER"),
        ];
        assert_eq!(combined_prompt(&messages), "SYS\n\nUSER");
        assert_eq!(combined_prompt(&messages[1..]), "USER");
        assert_eq!(
            cli_command("claude-cli", "claude-x"),
            (
                "claude".into(),
                vec!["-p".into(), "--model".into(), "claude-x".into()]
            )
        );
        assert_eq!(
            cli_command("codex-cli", ""),
            ("codex".into(), vec!["exec".into()])
        );
    }

    #[test]
    fn rules_are_found_inside_whatever_the_model_wrapped_them_in() {
        let answer = "Here you go:\n```json\n[{\"id\":\"err\",\"name\":\"Error\",\"pattern\":\"(?i)ERROR\",\"matchType\":\"line\",\"foreground\":\"#ff0000\",\"background\":\"\",\"bold\":true,\"italic\":false,\"enabled\":true,\"priority\":100}]\n```\nEnjoy.";
        let rules = extract_rules(answer).expect("an array in there");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].match_type, "line");
        assert_eq!(rules[0].priority, 100);
        assert_eq!(extract_rules("no rules here"), None);
        assert_eq!(extract_rules("] backwards ["), None);
        assert_eq!(extract_rules("[not json]"), None);
    }

    #[test]
    fn replies_are_read_and_errors_quoted() {
        let ok = r#"{"choices":[{"message":{"role":"assistant","content":"Fine."}}]}"#;
        assert_eq!(parse_openai_reply(200, ok).unwrap(), "Fine.");
        let api_error = r#"{"error":{"message":"Bad key"}}"#;
        assert_eq!(
            parse_openai_reply(401, api_error).unwrap_err(),
            AiError::Message("Bad key".into())
        );
        assert_eq!(
            parse_openai_reply(502, "gateway"),
            Err(AiError::Message(
                "AI request failed (HTTP 502): gateway".into()
            ))
        );
        assert_eq!(
            parse_openai_reply(200, r#"{"choices":[]}"#),
            Err(AiError::Message("Empty AI response".into()))
        );

        let claude = r#"{"content":[{"type":"text","text":"Hel"},{"type":"text","text":"lo"}]}"#;
        assert_eq!(parse_anthropic_reply(200, claude).unwrap(), "Hello");
        assert_eq!(
            parse_anthropic_reply(400, r#"{"error":{"type":"x","message":"No"}}"#),
            Err(AiError::Message("No".into()))
        );
        assert_eq!(
            parse_anthropic_reply(200, r#"{"content":[]}"#),
            Err(AiError::Message("Empty AI response".into()))
        );

        assert_eq!(
            parse_models(r#"{"data":[{"id":"b"},{"id":"a"},{"name":"c"}]}"#),
            ["b", "a", "c"]
        );
        assert_eq!(parse_models(r#"[{"id":"x"},{"name":"y"}]"#), ["x", "y"]);
        assert!(parse_models("nope").is_empty());
    }

    #[test]
    fn a_chat_goes_over_http_with_the_right_headers() {
        let (url, server) =
            net::testing::serve_once(200, r#"{"choices":[{"message":{"content":"Hi"}}]}"#);
        let answer = chat_openai(
            &url,
            "sk-test",
            "m",
            &[("X-Extra", "1")],
            &[AiMessage::new("user", "hey")],
        )
        .unwrap();
        assert_eq!(answer, "Hi");
        let request = server.join().unwrap();
        assert!(
            request.starts_with("POST /v1/chat/completions HTTP/1.1"),
            "{request}"
        );
        let lower = request.to_ascii_lowercase();
        assert!(lower.contains("authorization: bearer sk-test"), "{request}");
        assert!(lower.contains("x-extra: 1"), "{request}");
        assert!(
            request.ends_with(r#"{"model":"m","messages":[{"role":"user","content":"hey"}]}"#),
            "{request}"
        );

        let (url, server) =
            net::testing::serve_once(200, r#"{"content":[{"type":"text","text":"Yo"}]}"#);
        let messages = [AiMessage::new("system", "S"), AiMessage::new("user", "U")];
        assert_eq!(
            chat_anthropic(&url, "ak", "claude", &messages).unwrap(),
            "Yo"
        );
        let request = server.join().unwrap();
        assert!(
            request.starts_with("POST /v1/messages HTTP/1.1"),
            "{request}"
        );
        assert!(
            request.to_ascii_lowercase().contains("x-api-key: ak"),
            "{request}"
        );
        assert!(
            request.ends_with(r#"{"model":"claude","max_tokens":2048,"system":"S","messages":[{"role":"user","content":"U"}]}"#),
            "the system prompt travels as its own field: {request}"
        );
    }

    #[test]
    fn copilot_sign_in_reads_githubs_answers() {
        let (url, server) = net::testing::serve_once(
            200,
            r#"{"device_code":"dc","user_code":"ABCD-1234","verification_uri":"https://github.com/login/device","interval":7}"#,
        );
        let code = copilot::request_device_code_at(&url).unwrap();
        assert_eq!(code.user_code, "ABCD-1234");
        assert_eq!(code.interval, 7);
        let request = server.join().unwrap();
        assert!(
            request.ends_with(r#"{"client_id":"Iv1.b507a08c87ecfe98","scope":"read:user"}"#),
            "{request}"
        );

        let (url, server) = net::testing::serve_once(200, r#"{"token":"short-lived"}"#);
        assert_eq!(
            copilot::exchange_token_at(&url, "oauth").unwrap(),
            "short-lived"
        );
        let request = server.join().unwrap();
        let lower = request.to_ascii_lowercase();
        assert!(lower.contains("authorization: token oauth"), "{request}");
        assert!(
            lower.contains("copilot-integration-id: vscode-chat"),
            "{request}"
        );

        let (url, server) = net::testing::serve_once(200, r#"{"error":"expired_token"}"#);
        assert_eq!(
            copilot::poll_for_token_at(&url, "dc", 0),
            Err(AiError::Message("Code expired — try again".into()))
        );
        server.join().unwrap();
    }
}
