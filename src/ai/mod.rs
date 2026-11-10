//! AI on the user's own account: Ask (questions answered from the notes) and automations
//! (brain dumps filed into the library). The model is whatever the user points the settings
//! at, reached straight from this app with their own key.

pub mod agent;
mod anthropic;
pub mod ask;
pub mod automate;
pub mod chat;
mod openai;
pub mod organize;
pub mod search;
pub mod settings;
pub mod sse;
pub mod tools;

pub use automate::{Automations, Outcome};
pub use settings::{AssistantSettings, BrainDumpSettings, Connect, Connector};
