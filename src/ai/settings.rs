//! The AI settings, and how a request finds its transport: the provider from the settings file,
//! the key from the keychain.

use std::sync::Arc;

use nd::prelude::*;
use serde::{Deserialize, Serialize};

use super::chat::{AiError, Endpoint, Http, Provider, Transport};
use super::claude_code::ClaudeCode;

#[derive(Settings, Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[schemars(crate = "nd::schemars")]
#[serde(default, deny_unknown_fields)]
pub struct AssistantSettings {
    /// The wire format your provider speaks; requests go from this computer straight to it,
    /// with your own key. Or Claude Code installed on this computer, which needs no key.
    pub provider: Provider,
    /// Empty uses the provider's current model.
    pub model: String,
    /// Empty uses the provider's own address. Set it to use a gateway or a local server.
    pub base_url: String,
}

impl Default for AssistantSettings {
    fn default() -> Self {
        Self { provider: Provider::Anthropic, model: String::new(), base_url: String::new() }
    }
}

impl AssistantSettings {
    pub fn model(&self) -> String {
        let set = self.model.trim();
        if set.is_empty() { self.provider.default_model().to_owned() } else { set.to_owned() }
    }
}

#[derive(Settings, Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[schemars(crate = "nd::schemars")]
#[serde(default, deny_unknown_fields)]
pub struct BrainDumpSettings {
    /// Notes that rest in the inbox folder are filed into your folders by the model.
    pub organize: bool,
    /// The folder brain dumps land in.
    pub inbox_folder: String,
    /// How long a note rests unedited before it is organized.
    pub idle_minutes: u32,
    /// Added to the organizer's instructions: where things go, how titles read.
    pub extra_instructions: String,
}

impl Default for BrainDumpSettings {
    fn default() -> Self {
        Self { organize: false, inbox_folder: "Inbox".to_owned(), idle_minutes: 5, extra_instructions: String::new() }
    }
}

#[derive(Settings, Clone, PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[schemars(crate = "nd::schemars")]
#[serde(default, deny_unknown_fields)]
pub struct SharingSettings {
    /// Lets your other devices use Claude Code on this computer for Ask. The address and
    /// access code they need are under Assistant.
    pub enabled: bool,
    /// The port your devices connect to.
    pub port: u16,
}

impl Default for SharingSettings {
    fn default() -> Self {
        Self { enabled: false, port: 47600 }
    }
}

/// Opens a transport for the settings in effect. Runs off the main thread: the keychain read
/// can wait on the OS.
pub trait Connect: Send + Sync {
    fn connect(&self, settings: &AssistantSettings) -> Result<Arc<dyn Transport>, AiError>;
}

/// The real one: the key from the keychain and HTTPS to the provider, or Claude Code.
pub struct Keychain;

impl Connect for Keychain {
    fn connect(&self, settings: &AssistantSettings) -> Result<Arc<dyn Transport>, AiError> {
        let provider = settings.provider;
        let Some(account) = provider.key_account() else {
            return Ok(Arc::new(ClaudeCode { model: settings.model() }));
        };
        let key = crate::secrets::get(account)
            .map_err(|e| AiError::Keychain(e.to_string()))?
            .filter(|k| !k.trim().is_empty())
            .ok_or(AiError::NoKey(provider))?;
        let endpoint =
            Endpoint { provider, model: settings.model(), base_url: settings.base_url.trim().to_owned(), key };
        Ok(Arc::new(Http { endpoint }))
    }
}

/// Replaces the keychain and the network for the app (a test's fake provider).
#[derive(Clone)]
pub struct Connector(pub Arc<dyn Connect>);

pub fn connector(cx: &App) -> Arc<dyn Connect> {
    cx.try_global::<Connector>().map_or_else(|| Arc::new(Keychain) as Arc<dyn Connect>, |c| c.0.clone())
}
