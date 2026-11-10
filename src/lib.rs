//! Formal Notes: a folder of Markdown files as a library of notes.

pub mod actions;
pub mod ai;
pub mod demo;
pub mod library;
pub mod nav;
mod pump;
pub mod secrets;
pub mod settings;
pub mod sync;
mod theme;
pub mod views;

use std::path::PathBuf;

use nd::prelude::*;

use settings::NotesSettings;
use views::FormalNotes;
use views::metrics::{WINDOW, WINDOW_MIN};

pub fn app() -> Application {
    Application::new(nd::manifest!()).settings::<NotesSettings>()
}

/// The main window over the library at `root`. A `fixed` root ignores the library setting.
pub fn open_at(cx: &mut App, root: PathBuf, fixed: bool) -> WindowHandle<FormalNotes> {
    let options = WindowOptions::main("main")
        .title("Formal Notes")
        .size(size(px(WINDOW.0), px(WINDOW.1)))
        .min_size(size(px(WINDOW_MIN), px(360.)));
    cx.open_window(options, move |window, cx| cx.new(|cx| FormalNotes::new(root, fixed, window, cx)))
}

/// The main window over the user's library, or the demo library under `FORMALNOTES_DEMO`.
pub fn open(cx: &mut App) -> WindowHandle<FormalNotes> {
    if demo::enabled() {
        return open_demo(cx);
    }
    let root = cx.settings::<NotesSettings>().root();
    open_at(cx, root, false)
}

/// The main window over a freshly seeded demo library.
pub fn open_demo(cx: &mut App) -> WindowHandle<FormalNotes> {
    let root = demo::seed().unwrap_or_else(|_| std::env::temp_dir().join("formalnotes-demo"));
    open_at(cx, root, true)
}

struct LiveKey(String);

impl ai::Connect for LiveKey {
    fn connect(
        &self,
        settings: &ai::AssistantSettings,
    ) -> Result<std::sync::Arc<dyn ai::chat::Transport>, ai::chat::AiError> {
        let key = std::fs::read_to_string(&self.0).map_err(|e| ai::chat::AiError::Keychain(e.to_string()))?;
        let endpoint = ai::chat::Endpoint {
            provider: settings.provider,
            model: settings.model(),
            base_url: settings.base_url.trim().to_owned(),
            key: key.trim().to_owned(),
        };
        Ok(std::sync::Arc::new(ai::chat::Http { endpoint }))
    }
}

pub fn run() -> nd::Result<()> {
    pump::set_live();
    app().run(|cx| {
        if let Ok(path) = std::env::var("FORMALNOTES_LIVE_KEY_FILE") {
            cx.set_global(ai::Connector(std::sync::Arc::new(LiveKey(path))));
        }
        actions::init(cx);
        sync::init(cx);
        open(cx);
    })
}
