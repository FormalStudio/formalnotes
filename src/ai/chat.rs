//! What the app says to a model and hears back, whatever wire format carries it, and the HTTP
//! transport that carries it. Requests go from this machine straight to the provider with the
//! user's own key; nothing is logged or sent anywhere else.

use std::io::{BufRead, BufReader};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use nd::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::sse;
use super::{anthropic, openai};

/// The wire format the provider speaks.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[schemars(crate = "nd::schemars")]
pub enum Provider {
    /// Anthropic Messages (`/v1/messages`).
    #[default]
    #[serde(rename = "anthropic")]
    Anthropic,
    /// OpenAI chat completions, or any endpoint that speaks them.
    #[serde(rename = "OpenAI")]
    OpenAi,
}

impl Provider {
    pub fn name(self) -> &'static str {
        match self {
            Provider::Anthropic => "Anthropic",
            Provider::OpenAi => "OpenAI",
        }
    }

    pub fn default_model(self) -> &'static str {
        match self {
            Provider::Anthropic => "claude-sonnet-5-5",
            Provider::OpenAi => "gpt-5.5",
        }
    }

    /// The provider's own address, which the base URL setting replaces.
    pub fn default_base(self) -> &'static str {
        match self {
            Provider::Anthropic => "https://api.anthropic.com",
            Provider::OpenAi => "https://api.openai.com/v1",
        }
    }

    /// The keychain account the provider's key is stored under.
    pub fn key_account(self) -> &'static str {
        match self {
            Provider::Anthropic => "anthropic-api-key",
            Provider::OpenAi => "openai-api-key",
        }
    }
}

/// Where requests go and with which key.
#[derive(Clone, PartialEq)]
pub struct Endpoint {
    pub provider: Provider,
    pub model: String,
    pub base_url: String,
    pub key: String,
}

impl std::fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Endpoint")
            .field("provider", &self.provider)
            .field("model", &self.model)
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Text(String),
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        id: String,
        content: String,
        is_error: bool,
    },
    /// A block the app does not read (Anthropic's thinking blocks), sent back exactly as it
    /// came on the next request, as the API requires. Other wire formats leave it out.
    Opaque(Value),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    pub role: Role,
    pub content: Vec<Block>,
}

impl Message {
    pub fn user(text: impl Into<String>) -> Self {
        Self { role: Role::User, content: vec![Block::Text(text.into())] }
    }

    /// The text blocks, joined.
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|b| match b {
                Block::Text(t) => Some(t.as_str()),
                _ => None,
            })
            .collect()
    }

    pub fn tool_uses(&self) -> impl Iterator<Item = (&str, &str, &Value)> {
        self.content.iter().filter_map(|b| match b {
            Block::ToolUse { id, name, input } => Some((id.as_str(), name.as_str(), input)),
            _ => None,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    /// JSON Schema of the input object.
    pub schema: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Auto,
    /// The reply must be a call to this tool.
    Required(&'static str),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub system: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolSpec>,
    pub choice: Choice,
    pub max_tokens: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stop {
    EndTurn,
    ToolUse,
    MaxTokens,
    Refusal,
    Other(String),
}

/// One finished model turn.
#[derive(Clone, Debug, PartialEq)]
pub struct Reply {
    pub message: Message,
    pub stop: Stop,
}

/// Why a request did not come back with an answer, in words a person reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AiError {
    NoKey(Provider),
    Keychain(String),
    Unauthorized,
    RateLimited,
    Network(String),
    Server { status: u16, message: String },
    Refused,
    BadReply(String),
    Cancelled,
}

impl AiError {
    pub fn line(&self) -> String {
        match self {
            AiError::NoKey(provider) => format!("Add your {} API key in Settings to use AI.", provider.name()),
            AiError::Keychain(_) => "Could not read your API key from the keychain.".to_owned(),
            AiError::Unauthorized => "Your provider did not accept the API key. Check it in Settings.".to_owned(),
            AiError::RateLimited => "Your provider is limiting requests right now. Try again in a minute.".to_owned(),
            AiError::Network(_) => "Could not reach your provider. Check your connection and the base URL.".to_owned(),
            AiError::Server { status: 404, .. } => {
                "Your provider does not know this model or address. Check them in Settings.".to_owned()
            }
            AiError::Server { status, message } if *status < 500 => {
                format!("Your provider turned the request down: {}", short(message))
            }
            AiError::Server { .. } => "Your provider had a problem answering. Try again.".to_owned(),
            AiError::Refused => "The model declined to answer this.".to_owned(),
            AiError::BadReply(_) => "The model's reply could not be read. Try again.".to_owned(),
            AiError::Cancelled => "Stopped.".to_owned(),
        }
    }

    /// Whether the fix is in Settings (the key, the model, the address).
    pub fn wants_settings(&self) -> bool {
        matches!(
            self,
            AiError::NoKey(_)
                | AiError::Keychain(_)
                | AiError::Unauthorized
                | AiError::Network(_)
                | AiError::Server { status: 400..=404, .. }
        )
    }
}

fn short(message: &str) -> String {
    let line = message.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() > 160 { format!("{}…", line.chars().take(160).collect::<String>()) } else { line }
}

/// Set to stop a request; the transport checks it between stream events.
pub type Cancel = Arc<AtomicBool>;

pub fn cancelled(cancel: &Cancel) -> bool {
    cancel.load(Ordering::Relaxed)
}

/// Sends one request and streams the reply's text to `on_text` as it arrives.
pub trait Transport: Send + Sync {
    fn send(&self, request: &Request, on_text: &mut dyn FnMut(&str), cancel: &Cancel) -> Result<Reply, AiError>;
}

/// A reply being read off a stream, one server-sent event at a time.
pub trait StreamParser {
    fn event(&mut self, event: &sse::Event, on_text: &mut dyn FnMut(&str)) -> Result<(), AiError>;
    fn finish(self: Box<Self>) -> Result<Reply, AiError>;
}

/// Reads a whole recorded or live stream through the provider's parser.
pub fn read_stream(
    provider: Provider,
    reader: impl BufRead,
    on_text: &mut dyn FnMut(&str),
    cancel: &Cancel,
) -> Result<Reply, AiError> {
    let mut parser: Box<dyn StreamParser> = match provider {
        Provider::Anthropic => Box::new(anthropic::Parser::default()),
        Provider::OpenAi => Box::new(openai::Parser::default()),
    };
    let mut lines = sse::Lines::default();
    for line in reader.lines() {
        if cancelled(cancel) {
            return Err(AiError::Cancelled);
        }
        let line = line.map_err(|e| AiError::Network(e.to_string()))?;
        if let Some(event) = lines.push(&line) {
            parser.event(&event, on_text)?;
        }
    }
    if let Some(event) = lines.finish() {
        parser.event(&event, on_text)?;
    }
    parser.finish()
}

/// The real transport: HTTPS to the endpoint, the reply streamed.
pub struct Http {
    pub endpoint: Endpoint,
}

impl Http {
    fn url(&self) -> String {
        let base = self.endpoint.base_url.trim().trim_end_matches('/');
        let base = if base.is_empty() { self.endpoint.provider.default_base() } else { base };
        match self.endpoint.provider {
            Provider::Anthropic => format!("{base}/v1/messages"),
            Provider::OpenAi => format!("{base}/chat/completions"),
        }
    }
}

/// The JSON body `request` goes out as.
pub fn body(provider: Provider, request: &Request, model: &str) -> Value {
    match provider {
        Provider::Anthropic => anthropic::body(request, model),
        Provider::OpenAi => openai::body(request, model),
    }
}

impl Transport for Http {
    fn send(&self, request: &Request, on_text: &mut dyn FnMut(&str), cancel: &Cancel) -> Result<Reply, AiError> {
        let endpoint = &self.endpoint;
        let body = body(endpoint.provider, request, &endpoint.model);
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(20)))
            .timeout_recv_response(Some(Duration::from_secs(180)))
            .build()
            .into();
        let call = agent.post(self.url()).header("content-type", "application/json");
        let call = match endpoint.provider {
            Provider::Anthropic => call.header("x-api-key", &endpoint.key).header("anthropic-version", "2026-10-21"),
            Provider::OpenAi => call.header("authorization", &format!("Bearer {}", endpoint.key)),
        };
        let response = call.send(body.to_string()).map_err(|e| AiError::Network(e.to_string()))?;
        let status = response.status().as_u16();
        let mut body = response.into_body();
        if status >= 400 {
            let text = body.read_to_string().unwrap_or_default();
            return Err(status_error(status, &text));
        }
        let reader = BufReader::new(body.into_reader());
        read_stream(endpoint.provider, reader, on_text, cancel)
    }
}

/// The error an HTTP status and its body stand for. Providers and gateways shape error bodies
/// differently; the message is taken from wherever it is.
pub fn status_error(status: u16, body: &str) -> AiError {
    match status {
        401 | 403 => AiError::Unauthorized,
        429 => AiError::RateLimited,
        _ => AiError::Server { status, message: error_message(body) },
    }
}

pub fn error_message(body: &str) -> String {
    let Ok(json) = serde_json::from_str::<Value>(body) else { return body.trim().to_owned() };
    let found = [
        json.pointer("/error/message"),
        json.get("error").filter(|e| e.is_string()),
        json.get("message"),
        json.get("detail"),
    ];
    found.into_iter().flatten().find_map(Value::as_str).map_or_else(|| body.trim().to_owned(), str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_messages_come_from_any_shape() {
        assert_eq!(error_message(r#"{"type":"error","error":{"type":"x","message":"bad model"}}"#), "bad model");
        assert_eq!(error_message(r#"{"success":false,"error":"The key is invalid"}"#), "The key is invalid");
        assert_eq!(error_message("plain"), "plain");
        assert_eq!(status_error(401, ""), AiError::Unauthorized);
        assert!(status_error(404, "{}").wants_settings());
    }

    #[test]
    fn urls_follow_the_base() {
        let http = |provider, base: &str| Http {
            endpoint: Endpoint { provider, model: String::new(), base_url: base.into(), key: String::new() },
        };
        assert_eq!(http(Provider::Anthropic, "").url(), "https://api.anthropic.com/v1/messages");
        assert_eq!(http(Provider::Anthropic, "https://gw.example/").url(), "https://gw.example/v1/messages");
        assert_eq!(http(Provider::OpenAi, "").url(), "https://api.openai.com/v1/chat/completions");
        assert_eq!(
            http(Provider::OpenAi, "http://localhost:1234/v1").url(),
            "http://localhost:1234/v1/chat/completions"
        );
    }

    #[test]
    fn the_key_never_prints() {
        let endpoint = Endpoint {
            provider: Provider::Anthropic,
            model: "m".into(),
            base_url: String::new(),
            key: "sk-secret".into(),
        };
        assert!(!format!("{endpoint:?}").contains("sk-secret"));
    }
}
