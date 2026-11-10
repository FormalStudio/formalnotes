//! Anthropic Messages: the request body and the streamed reply.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::chat::{AiError, Block, Choice, Message, Reply, Request, Role, Stop, StreamParser};
use super::sse;

pub fn body(request: &Request, model: &str) -> Value {
    let mut body = json!({
        "model": model,
        "max_tokens": request.max_tokens,
        "stream": true,
        "system": request.system,
        "messages": request.messages.iter().map(message).collect::<Vec<_>>(),
    });
    if !request.tools.is_empty() {
        body["tools"] = request
            .tools
            .iter()
            .map(|t| json!({ "name": t.name, "description": t.description, "input_schema": t.schema }))
            .collect();
        // Current models refuse a forced tool choice with a 400; a required tool is asked for
        // in the prompt instead, and a reply without the call is treated as a failure.
        body["tool_choice"] = match request.choice {
            Choice::Auto | Choice::Required(_) => json!({ "type": "auto" }),
        };
    }
    body
}

fn message(message: &Message) -> Value {
    let role = match message.role {
        Role::User => "user",
        Role::Assistant => "assistant",
    };
    let content: Vec<Value> = message
        .content
        .iter()
        .filter_map(|block| match block {
            Block::Text(text) if text.is_empty() => None,
            Block::Text(text) => Some(json!({ "type": "text", "text": text })),
            Block::ToolUse { id, name, input } => {
                Some(json!({ "type": "tool_use", "id": id, "name": name, "input": input }))
            }
            Block::ToolResult { id, content, is_error } => Some(json!({
                "type": "tool_result",
                "tool_use_id": id,
                "content": content,
                "is_error": is_error,
            })),
            Block::Opaque(raw) => Some(raw.clone()),
        })
        .collect();
    json!({ "role": role, "content": content })
}

/// Rebuilds the reply's content blocks from their start events and deltas. Blocks the app does
/// not read (thinking, with its signature) are rebuilt the same way and kept whole.
#[derive(Default)]
pub struct Parser {
    blocks: BTreeMap<u64, Value>,
    inputs: BTreeMap<u64, String>,
    stop: Option<Stop>,
}

impl StreamParser for Parser {
    fn event(&mut self, event: &sse::Event, on_text: &mut dyn FnMut(&str)) -> Result<(), AiError> {
        let data: Value = serde_json::from_str(&event.data).map_err(|e| AiError::BadReply(e.to_string()))?;
        let index = data.get("index").and_then(Value::as_u64).unwrap_or(0);
        match data.get("type").and_then(Value::as_str).unwrap_or(&event.name) {
            "content_block_start" => {
                let block = data.get("content_block").cloned().unwrap_or(Value::Null);
                if let Some(text) = block.get("text").and_then(Value::as_str).filter(|t| !t.is_empty()) {
                    on_text(text);
                }
                self.blocks.insert(index, block);
            }
            "content_block_delta" => {
                let delta = data.get("delta").cloned().unwrap_or(Value::Null);
                let field = |name: &str| delta.get(name).and_then(Value::as_str).unwrap_or_default().to_owned();
                let Some(block) = self.blocks.get_mut(&index) else { return Ok(()) };
                match delta.get("type").and_then(Value::as_str).unwrap_or_default() {
                    "text_delta" => {
                        let text = field("text");
                        on_text(&text);
                        append(block, "text", &text);
                    }
                    "input_json_delta" => self.inputs.entry(index).or_default().push_str(&field("partial_json")),
                    "thinking_delta" => append(block, "thinking", &field("thinking")),
                    "signature_delta" => block["signature"] = Value::String(field("signature")),
                    _ => {}
                }
            }
            "content_block_stop" => {
                if let (Some(block), Some(input)) = (self.blocks.get_mut(&index), self.inputs.remove(&index)) {
                    let input = if input.trim().is_empty() { "{}" } else { input.as_str() };
                    block["input"] =
                        serde_json::from_str(input).map_err(|e| AiError::BadReply(format!("tool input: {e}")))?;
                }
            }
            "message_delta" => {
                if let Some(reason) = data.pointer("/delta/stop_reason").and_then(Value::as_str) {
                    self.stop = Some(stop(reason));
                }
            }
            "error" => {
                let kind = data.pointer("/error/type").and_then(Value::as_str).unwrap_or_default();
                let message = data.pointer("/error/message").and_then(Value::as_str).unwrap_or_default().to_owned();
                return Err(match kind {
                    "rate_limit_error" => AiError::RateLimited,
                    "authentication_error" | "permission_error" => AiError::Unauthorized,
                    "overloaded_error" => AiError::Server { status: 529, message },
                    _ => AiError::Server { status: 500, message },
                });
            }
            _ => {}
        }
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<Reply, AiError> {
        let stop = self.stop.ok_or_else(|| AiError::BadReply("the stream ended early".into()))?;
        let content = self.blocks.into_values().map(block).collect();
        Ok(Reply { message: Message { role: Role::Assistant, content }, stop })
    }
}

fn append(block: &mut Value, field: &str, text: &str) {
    let now = block.get(field).and_then(Value::as_str).unwrap_or_default();
    block[field] = Value::String(format!("{now}{text}"));
}

fn block(raw: Value) -> Block {
    let text = |name: &str| raw.get(name).and_then(Value::as_str).unwrap_or_default().to_owned();
    match raw.get("type").and_then(Value::as_str) {
        Some("text") => Block::Text(text("text")),
        Some("tool_use") => Block::ToolUse {
            id: text("id"),
            name: text("name"),
            input: raw.get("input").cloned().unwrap_or_else(|| json!({})),
        },
        _ => Block::Opaque(raw),
    }
}

fn stop(reason: &str) -> Stop {
    match reason {
        "end_turn" | "stop_sequence" => Stop::EndTurn,
        "tool_use" => Stop::ToolUse,
        "max_tokens" => Stop::MaxTokens,
        "refusal" => Stop::Refusal,
        other => Stop::Other(other.to_owned()),
    }
}
