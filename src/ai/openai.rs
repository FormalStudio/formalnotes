//! OpenAI chat completions, which most other providers and local servers speak too: the
//! request body and the streamed reply.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::chat::{AiError, Block, Choice, Message, Reply, Request, Role, Stop, StreamParser};
use super::sse;

/// No token limit is sent: OpenAI's newer models refuse `max_tokens` and many compatible
/// servers do not know `max_completion_tokens`.
pub fn body(request: &Request, model: &str) -> Value {
    let mut messages = vec![json!({ "role": "system", "content": request.system })];
    for message in &request.messages {
        push(&mut messages, message);
    }
    let mut body = json!({ "model": model, "stream": true, "messages": messages });
    if !request.tools.is_empty() {
        body["tools"] = request
            .tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": { "name": t.name, "description": t.description, "parameters": t.schema },
                })
            })
            .collect();
        body["tool_choice"] = match request.choice {
            Choice::Auto => json!("auto"),
            Choice::Required(name) => json!({ "type": "function", "function": { "name": name } }),
        };
    }
    body
}

fn push(out: &mut Vec<Value>, message: &Message) {
    match message.role {
        Role::User => {
            for block in &message.content {
                if let Block::ToolResult { id, content, .. } = block {
                    out.push(json!({ "role": "tool", "tool_call_id": id, "content": content }));
                }
            }
            let text = message.text();
            if !text.is_empty() {
                out.push(json!({ "role": "user", "content": text }));
            }
        }
        Role::Assistant => {
            let calls: Vec<Value> = message
                .tool_uses()
                .map(|(id, name, input)| {
                    json!({ "id": id, "type": "function", "function": { "name": name, "arguments": input.to_string() } })
                })
                .collect();
            let text = message.text();
            let mut entry =
                json!({ "role": "assistant", "content": if text.is_empty() { Value::Null } else { json!(text) } });
            if !calls.is_empty() {
                entry["tool_calls"] = Value::Array(calls);
            }
            out.push(entry);
        }
    }
}

#[derive(Default)]
struct Call {
    id: String,
    name: String,
    arguments: String,
}

#[derive(Default)]
pub struct Parser {
    text: String,
    calls: BTreeMap<u64, Call>,
    stop: Option<Stop>,
    done: bool,
}

impl StreamParser for Parser {
    fn event(&mut self, event: &sse::Event, on_text: &mut dyn FnMut(&str)) -> Result<(), AiError> {
        if event.data.trim() == "[DONE]" {
            self.done = true;
            return Ok(());
        }
        let data: Value = serde_json::from_str(&event.data).map_err(|e| AiError::BadReply(e.to_string()))?;
        if let Some(error) = data.get("error") {
            let message = error.get("message").and_then(Value::as_str).unwrap_or_default().to_owned();
            return Err(AiError::Server { status: 500, message });
        }
        let Some(choice) = data.pointer("/choices/0") else { return Ok(()) };
        if let Some(text) = choice.pointer("/delta/content").and_then(Value::as_str).filter(|t| !t.is_empty()) {
            on_text(text);
            self.text.push_str(text);
        }
        for call in choice.pointer("/delta/tool_calls").and_then(Value::as_array).into_iter().flatten() {
            let index = call.get("index").and_then(Value::as_u64).unwrap_or(0);
            let entry = self.calls.entry(index).or_default();
            let field = |pointer: &str| call.pointer(pointer).and_then(Value::as_str).unwrap_or_default();
            if !field("/id").is_empty() {
                entry.id = field("/id").to_owned();
            }
            entry.name.push_str(field("/function/name"));
            entry.arguments.push_str(field("/function/arguments"));
        }
        if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
            self.stop = Some(match reason {
                "stop" => Stop::EndTurn,
                "tool_calls" | "function_call" => Stop::ToolUse,
                "length" => Stop::MaxTokens,
                "content_filter" => Stop::Refusal,
                other => Stop::Other(other.to_owned()),
            });
        }
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<Reply, AiError> {
        let stop = match self.stop {
            Some(stop) => stop,
            None if self.done && !self.calls.is_empty() => Stop::ToolUse,
            None if self.done => Stop::EndTurn,
            None => return Err(AiError::BadReply("the stream ended early".into())),
        };
        let mut content = Vec::new();
        if !self.text.is_empty() {
            content.push(Block::Text(self.text));
        }
        for (index, call) in self.calls {
            let arguments = if call.arguments.trim().is_empty() { "{}" } else { call.arguments.as_str() };
            let input = serde_json::from_str(arguments).map_err(|e| AiError::BadReply(format!("tool input: {e}")))?;
            let id = if call.id.is_empty() { format!("call_{index}") } else { call.id };
            content.push(Block::ToolUse { id, name: call.name, input });
        }
        Ok(Reply { message: Message { role: Role::Assistant, content }, stop })
    }
}
