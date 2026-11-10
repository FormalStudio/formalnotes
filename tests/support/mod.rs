//! A provider that answers from a script, for tests that must not touch the network. Each
//! test crate uses its own part of it.
#![allow(dead_code, clippy::unwrap_used)]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use formalnotes::ai::chat::{AiError, Block, Cancel, Message, Reply, Request, Role, Stop, Transport};
use formalnotes::ai::{AssistantSettings, Connect};
use serde_json::Value;

/// A provider that answers from a script and keeps every request it got.
#[derive(Default)]
pub struct Script {
    replies: Mutex<VecDeque<Result<Reply, AiError>>>,
    seen: Mutex<Vec<Request>>,
}

impl Script {
    pub fn new(replies: impl IntoIterator<Item = Result<Reply, AiError>>) -> Arc<Self> {
        Arc::new(Self { replies: Mutex::new(replies.into_iter().collect()), seen: Mutex::default() })
    }

    pub fn seen(&self) -> Vec<Request> {
        self.seen.lock().unwrap().clone()
    }
}

pub struct Fake(pub Arc<Script>);

impl Transport for Fake {
    fn send(&self, request: &Request, on_text: &mut dyn FnMut(&str), _: &Cancel) -> Result<Reply, AiError> {
        self.0.seen.lock().unwrap().push(request.clone());
        let reply =
            self.0.replies.lock().unwrap().pop_front().unwrap_or(Err(AiError::BadReply("script ran out".into())));
        if let Ok(reply) = &reply {
            on_text(&reply.message.text());
        }
        reply
    }
}

impl Connect for Fake {
    fn connect(&self, _: &AssistantSettings) -> Result<Arc<dyn Transport>, AiError> {
        Ok(Arc::new(Fake(self.0.clone())))
    }
}

pub fn call(id: &str, name: &str, input: Value) -> Result<Reply, AiError> {
    Ok(Reply {
        message: Message {
            role: Role::Assistant,
            content: vec![Block::ToolUse { id: id.into(), name: name.into(), input }],
        },
        stop: Stop::ToolUse,
    })
}

pub fn answer(text: &str) -> Result<Reply, AiError> {
    Ok(Reply {
        message: Message { role: Role::Assistant, content: vec![Block::Text(text.into())] },
        stop: Stop::EndTurn,
    })
}

/// A provider the app cannot reach: no key, a refused key.
pub struct Failing(pub AiError);

impl Connect for Failing {
    fn connect(&self, _: &AssistantSettings) -> Result<Arc<dyn Transport>, AiError> {
        Err(self.0.clone())
    }
}
