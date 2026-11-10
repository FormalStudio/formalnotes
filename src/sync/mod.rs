//! The account and end-to-end encrypted sync (`docs/protocol.md`). The server holds sealed
//! objects; keys come from the password on this device and the vault key never leaves it
//! unwrapped. The engine runs on its own thread between the library folder and the server;
//! [`SyncModel`] is what the window and the Settings window read.

pub mod account;
pub mod crypto;
pub mod engine;
pub mod local;
pub mod object;
pub mod remote;
pub mod service;
pub mod state;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use nd::prelude::*;

pub use service::{Status, Task};

use remote::Usage;
use service::{Command, Event, Handle};

/// What the UI knows of the account and the sync, delivered from the sync thread.
pub struct SyncModel {
    handle: Option<Handle>,
    pub email: Option<String>,
    pub usage: Option<Usage>,
    pub status: Status,
    /// The account request running now.
    pub task: Option<Task>,
    /// The last account request that failed, and why.
    pub failure: Option<(Task, String)>,
}

struct Global(Entity<SyncModel>);

impl SyncModel {
    /// A model with no sync thread behind it: tests and the appearance audit.
    pub fn idle() -> Self {
        Self { handle: None, email: None, usage: None, status: Status::SignedOut, task: None, failure: None }
    }

    fn handle_event(&mut self, event: Event) {
        match event {
            Event::SignedIn(email) => self.email = Some(email),
            Event::SignedOut => {
                self.email = None;
                self.usage = None;
            }
            Event::Usage(usage) => self.usage = Some(usage),
            Event::Status(status) => self.status = status,
            Event::Working(task) => {
                self.task = Some(task);
                self.failure = None;
            }
            Event::Finished(task, failure) => {
                self.task = None;
                self.failure = failure.map(|f| (task, f));
            }
        }
    }

    fn send(&self, command: Command) {
        if let Some(handle) = &self.handle {
            let _ = handle.commands.send(command);
        }
    }

    pub fn sign_in(&mut self, email: String, password: String) {
        self.send(Command::SignIn { email, password });
    }

    pub fn sign_up(&mut self, email: String, password: String) {
        self.send(Command::SignUp { email, password });
    }

    pub fn sign_out(&mut self) {
        self.send(Command::SignOut);
    }

    pub fn change_password(&mut self, current: String, new: String) {
        self.send(Command::ChangePassword { current, new });
    }

    pub fn delete_account(&mut self, password: String) {
        self.send(Command::DeleteAccount { password });
    }

    pub fn sync_now(&mut self) {
        self.send(Command::SyncNow);
    }

    /// Shows a state without a sync thread: the appearance audit and the screenshots.
    pub fn preview(&mut self, email: Option<String>, usage: Option<Usage>, status: Status) {
        self.email = email;
        self.usage = usage;
        self.status = status;
    }
}

/// Starts the sync thread and wakes it whenever the app comes to the front.
pub fn init(cx: &mut App) {
    let dir = nd::system::app_dirs().data().join("sync");
    let remote: Arc<dyn remote::Remote> = Arc::new(remote::Http::new(remote::base_url()));
    let handle = service::start(remote, Arc::new(account::Keychain), dir);
    let events = handle.events.clone();
    let commands = handle.commands.clone();
    let model = cx.new(|_| SyncModel { handle: Some(handle), ..SyncModel::idle() });
    crate::pump::pump(events, model.downgrade(), |model: &mut SyncModel, events, cx| {
        for event in events {
            model.handle_event(event);
        }
        cx.notify();
        true
    });
    let system = nd::system::system();
    nd::system::on_event(&system, move |event, _| {
        if matches!(event, nd::system::SystemEvent::Activated) {
            let _ = commands.send(Command::Activated);
        }
    });
    cx.set_global(Global(model));
}

/// The app's sync model; without [`init`] (tests) an idle one.
pub fn model(cx: &mut App) -> Entity<SyncModel> {
    if let Some(Global(model)) = cx.try_global::<Global>() {
        return model.clone();
    }
    let model = cx.new(|_| SyncModel::idle());
    cx.set_global(Global(model.clone()));
    model
}

/// Points sync at the library the window shows; `None` for one that is not the user's.
pub fn follow_library(cx: &mut App, root: Option<PathBuf>) {
    model(cx).read(cx).send(Command::Root(root));
}

/// The notes the app has unsaved typing for: sync leaves them alone until it is saved.
pub fn set_unsaved(cx: &mut App, paths: HashSet<String>) {
    let model = model(cx);
    if let Some(handle) = &model.read(cx).handle {
        match handle.busy.lock() {
            Ok(mut busy) => *busy = paths,
            Err(poisoned) => *poisoned.into_inner() = paths,
        }
    }
}
