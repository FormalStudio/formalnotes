//! The tool loop: send, run the tools the model calls, send their results back, until the model
//! answers.

use serde_json::Value;

use super::chat::{AiError, Block, Cancel, Message, Reply, Request, Role, Stop, ToolSpec, Transport, cancelled};

/// Tool calls a conversation may make before it has to answer.
pub const MAX_STEPS: usize = 8;

/// What the model can call.
pub trait Tools {
    fn specs(&self) -> Vec<ToolSpec>;
    /// The tool's result as text, or an error the model reads and can recover from.
    fn run(&mut self, name: &str, input: &Value) -> Result<String, String>;
}

/// Progress while the loop runs.
#[derive(Clone, Debug, PartialEq)]
pub enum Progress {
    Text(String),
    /// A tool is about to run, with the input it was given.
    Tool {
        name: String,
        input: Value,
    },
    /// The text so far was a preamble to a tool call; the answer starts over.
    Restart,
}

/// Runs the loop over `request` and returns every message it added (the model's turns and the
/// tool results), the last being the answer.
pub fn run(
    transport: &dyn Transport,
    mut request: Request,
    tools: &mut dyn Tools,
    cancel: &Cancel,
    progress: &mut dyn FnMut(Progress),
) -> Result<Vec<Message>, AiError> {
    request.tools = tools.specs();
    let start = request.messages.len();
    for _ in 0..MAX_STEPS {
        let Reply { message, stop } =
            transport.send(&request, &mut |text| progress(Progress::Text(text.to_owned())), cancel)?;
        if cancelled(cancel) {
            return Err(AiError::Cancelled);
        }
        let calls: Vec<(String, String, Value)> =
            message.tool_uses().map(|(id, name, input)| (id.to_owned(), name.to_owned(), input.clone())).collect();
        request.messages.push(message);
        if stop == Stop::Refusal {
            return Err(AiError::Refused);
        }
        if calls.is_empty() {
            return Ok(request.messages.split_off(start));
        }
        progress(Progress::Restart);
        let mut results = Vec::new();
        for (id, name, input) in calls {
            progress(Progress::Tool { name: name.clone(), input: input.clone() });
            let (content, is_error) = match tools.run(&name, &input) {
                Ok(content) => (content, false),
                Err(error) => (error, true),
            };
            results.push(Block::ToolResult { id, content, is_error });
        }
        request.messages.push(Message { role: Role::User, content: results });
    }
    Err(AiError::BadReply(format!("no answer after {MAX_STEPS} tool calls")))
}
