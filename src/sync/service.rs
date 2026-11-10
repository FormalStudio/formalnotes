//! The sync thread: account requests, the schedule (two seconds after a local change, when the
//! app comes to the front, every 30 seconds) and the back-off while the server is out of
//! reach. It owns the engine and the session, and reports to the UI over a channel the way the
//! disk worker does.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use jiff::Timestamp;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use super::account::{self, AccountError, Keystore, Session};
use super::engine::{Busy, Engine, SyncError};
use super::picture::{self, Picture};
use super::remote::{Remote, RemoteError, Usage};
use super::state;

const AFTER_CHANGE: Duration = Duration::from_secs(2);
const EVERY: Duration = Duration::from_secs(30);
const BACKOFF_FIRST: Duration = Duration::from_secs(5);
const BACKOFF_MAX: Duration = Duration::from_secs(300);

pub enum Command {
    /// The library to sync; `None` stops syncing (a demo library).
    Root(Option<PathBuf>),
    SyncNow,
    Activated,
    LocalChange,
    SignIn {
        email: String,
        password: String,
    },
    SignUp {
        email: String,
        password: String,
    },
    SignOut,
    ChangePassword {
        current: String,
        new: String,
    },
    DeleteAccount {
        password: String,
    },
    /// An image file to use as the profile picture.
    SetPicture(PathBuf),
    RemovePicture,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Task {
    SigningIn,
    CreatingAccount,
    SigningOut,
    ChangingPassword,
    DeletingAccount,
    SavingPicture,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    SignedOut,
    /// Signed in, nothing has run yet.
    Waiting,
    Syncing,
    Synced {
        at: Timestamp,
        problems: Vec<String>,
    },
    Offline,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    SignedIn(String),
    SignedOut,
    Usage(Usage),
    /// The account's picture as it is now: after sign-in, a change here or one from another device.
    Picture(Option<Picture>),
    Status(Status),
    Working(Task),
    /// An account request finished: `None` when it worked, else what went wrong.
    Finished(Task, Option<String>),
}

pub struct Handle {
    pub commands: flume::Sender<Command>,
    pub events: flume::Receiver<Event>,
    pub busy: Busy,
}

pub struct Service {
    remote: Arc<dyn Remote>,
    keys: Arc<dyn Keystore>,
    state_dir: PathBuf,
    busy: Busy,
    events: flume::Sender<Event>,
    commands: flume::WeakSender<Command>,
    session: Option<Session>,
    root: Option<PathBuf>,
    engine: Option<Engine>,
    watcher: Option<RecommendedWatcher>,
    change_at: Option<Instant>,
    next_at: Option<Instant>,
    backoff: Option<Duration>,
    /// The version of the picture the UI was last sent.
    picture: Option<u64>,
}

/// Starts the sync thread. `state_dir` holds the per-library state files.
pub fn start(remote: Arc<dyn Remote>, keys: Arc<dyn Keystore>, state_dir: PathBuf) -> Handle {
    let (commands, rx) = flume::unbounded();
    let (tx, events) = flume::unbounded();
    let busy = Busy::default();
    let service = Service::new(remote, keys, state_dir, busy.clone(), tx, commands.downgrade());
    let _ = std::thread::Builder::new().name("formalnotes-sync".into()).spawn(move || service.run(&rx));
    Handle { commands, events, busy }
}

impl Service {
    pub fn new(
        remote: Arc<dyn Remote>,
        keys: Arc<dyn Keystore>,
        state_dir: PathBuf,
        busy: Busy,
        events: flume::Sender<Event>,
        commands: flume::WeakSender<Command>,
    ) -> Self {
        Self {
            remote,
            keys,
            state_dir,
            busy,
            events,
            commands,
            session: None,
            root: None,
            engine: None,
            watcher: None,
            change_at: None,
            next_at: None,
            backoff: None,
            picture: None,
        }
    }

    fn run(mut self, rx: &flume::Receiver<Command>) {
        match account::load(&*self.keys) {
            Ok(Some(session)) => self.signed_in(session),
            Ok(None) => self.send(Event::SignedOut),
            Err(e) => {
                self.send(Event::SignedOut);
                self.send(Event::Status(Status::Failed(e.to_string())));
            }
        }
        loop {
            // nd-allow(clock-in-ui): the sync thread's own schedule, not view code
            let now = Instant::now();
            let due = [self.change_at, self.next_at].into_iter().flatten().min();
            let command = match due {
                Some(at) => match rx.recv_timeout(at.saturating_duration_since(now)) {
                    Ok(c) => Some(c),
                    Err(flume::RecvTimeoutError::Timeout) => None,
                    Err(flume::RecvTimeoutError::Disconnected) => break,
                },
                None => match rx.recv() {
                    Ok(c) => Some(c),
                    Err(_) => break,
                },
            };
            match command {
                Some(command) => self.handle(command),
                None => {
                    // nd-allow(clock-in-ui): the sync thread's own schedule, not view code
                    let now = Instant::now();
                    let local_only =
                        self.change_at.is_some_and(|at| at <= now) && self.next_at.is_none_or(|at| at > now);
                    self.sync(local_only);
                }
            }
        }
    }

    fn send(&self, event: Event) {
        let _ = self.events.send(event);
    }

    pub fn handle(&mut self, command: Command) {
        match command {
            Command::Root(root) => {
                if root != self.root {
                    self.root = root;
                    self.engine = None;
                    self.watch();
                    self.sync(false);
                }
            }
            Command::SyncNow | Command::Activated => {
                self.backoff = None;
                self.sync(false);
            }
            Command::LocalChange => {
                if self.session.is_some() && self.root.is_some() {
                    // nd-allow(clock-in-ui): the sync thread's own schedule, not view code
                    let at = Instant::now() + AFTER_CHANGE;
                    self.change_at = Some(self.backoff_until().map_or(at, |b| b.max(at)));
                }
            }
            Command::SignIn { email, password } => {
                self.account(Task::SigningIn, |s| account::sign_in(&*s.remote, &*s.keys, &email, &password).map(Some))
            }
            Command::SignUp { email, password } => self.account(Task::CreatingAccount, |s| {
                account::sign_up(&*s.remote, &*s.keys, &email, &password).map(Some)
            }),
            Command::SignOut => self.account(Task::SigningOut, |s| match &s.session {
                Some(session) => account::sign_out(&*s.remote, &*s.keys, session).map(|()| None),
                None => Ok(None),
            }),
            Command::ChangePassword { current, new } => self.account(Task::ChangingPassword, |s| match &s.session {
                Some(session) => {
                    account::change_password(&*s.remote, session, &current, &new).map(|()| s.session.clone())
                }
                None => Ok(None),
            }),
            Command::DeleteAccount { password } => self.account(Task::DeletingAccount, |s| match &s.session {
                Some(session) => {
                    account::delete_account(&*s.remote, &*s.keys, session, &password)?;
                    s.forget_state(session);
                    Ok(None)
                }
                None => Ok(None),
            }),
            Command::SetPicture(path) => self.picture_task(|s, session| {
                let file = std::fs::read(&path).map_err(|e| format!("The picture could not be read: {e}"))?;
                let jpeg = picture::square_jpeg(&file).ok_or("That file is not a picture Formal Notes can read.")?;
                let blob = picture::seal(&session.vault, &jpeg).map_err(|e| e.to_string())?;
                let version = s.remote.put_avatar(&session.token, &blob).map_err(|e| AccountError::from(e).to_string())?;
                Ok(Some(Picture { version, jpeg: jpeg.into() }))
            }),
            Command::RemovePicture => self.picture_task(|s, session| {
                s.remote.delete_avatar(&session.token).map_err(|e| AccountError::from(e).to_string())?;
                Ok(None)
            }),
        }
    }

    /// Changes the account's picture and shows the result.
    fn picture_task(&mut self, run: impl FnOnce(&Self, &Session) -> Result<Option<Picture>, String>) {
        let Some(session) = self.session.clone() else { return };
        self.send(Event::Working(Task::SavingPicture));
        match run(self, &session) {
            Ok(picture) => {
                self.picture = picture.as_ref().map(|p| p.version);
                self.send(Event::Picture(picture));
                self.send(Event::Finished(Task::SavingPicture, None));
            }
            Err(why) => self.send(Event::Finished(Task::SavingPicture, Some(why))),
        }
    }

    /// Fetches the picture when the account reports a version other than the one shown.
    fn follow_picture(&mut self, version: Option<u64>) {
        if version == self.picture {
            return;
        }
        let Some(session) = &self.session else { return };
        let picture = match version {
            None => None,
            Some(version) => match self.remote.avatar(&session.token) {
                Ok(Some(blob)) => match picture::open(&session.vault, &blob) {
                    Ok(jpeg) => Some(Picture { version, jpeg: jpeg.into() }),
                    Err(_) => return,
                },
                Ok(None) => None,
                Err(_) => return,
            },
        };
        self.picture = picture.as_ref().map(|p| p.version);
        self.send(Event::Picture(picture));
    }

    /// Runs an account request. `Some(session)` leaves this device signed in with it, `None`
    /// signed out.
    fn account(&mut self, task: Task, run: impl FnOnce(&mut Self) -> account::Result<Option<Session>>) {
        self.send(Event::Working(task));
        match run(self) {
            Ok(Some(session)) => {
                self.send(Event::Finished(task, None));
                if self.session.as_ref().is_none_or(|s| s.email != session.email || s.token != session.token) {
                    self.signed_in(session);
                }
            }
            Ok(None) => {
                self.signed_out();
                self.send(Event::Finished(task, None));
            }
            Err(e) => self.send(Event::Finished(task, Some(e.to_string()))),
        }
    }

    fn signed_in(&mut self, session: Session) {
        self.send(Event::SignedIn(session.email.clone()));
        self.session = Some(session);
        self.engine = None;
        self.backoff = None;
        self.picture = None;
        self.send(Event::Picture(None));
        self.send(Event::Status(Status::Waiting));
        self.sync(false);
    }

    fn signed_out(&mut self) {
        self.session = None;
        self.picture = None;
        self.send(Event::Picture(None));
        self.engine = None;
        self.change_at = None;
        self.next_at = None;
        self.send(Event::SignedOut);
        self.send(Event::Status(Status::SignedOut));
    }

    /// Removes this device's sync state for the account: every library's.
    fn forget_state(&self, session: &Session) {
        if let Some(root) = &self.root {
            let file = state::file(&self.state_dir, &session.email, root);
            if file.parent() == Some(self.state_dir.as_path()) {
                let _ = std::fs::remove_file(file);
            }
        }
    }

    fn backoff_until(&self) -> Option<Instant> {
        // nd-allow(clock-in-ui): the sync thread's own schedule, not view code
        self.backoff.and(self.next_at).filter(|at| *at > Instant::now())
    }

    fn engine(&mut self) -> Option<&mut Engine> {
        let (session, root) = (self.session.as_ref()?, self.root.as_ref()?);
        if self.engine.as_ref().is_none_or(|e| e.root() != root.as_path()) {
            let file = state::file(&self.state_dir, &session.email, root);
            match Engine::new(
                root.clone(),
                self.remote.clone(),
                session.token.clone(),
                session.vault,
                file,
                self.busy.clone(),
            ) {
                Ok(engine) => self.engine = Some(engine),
                Err(e) => {
                    self.send(Event::Status(Status::Failed(format!("The sync state could not be read: {e}"))));
                    return None;
                }
            }
        }
        self.engine.as_mut()
    }

    /// One sync. `local_only` skips it when nothing here changed: a wake from the watcher,
    /// which also sees the engine's own writes.
    pub fn sync(&mut self, local_only: bool) {
        self.change_at = None;
        // nd-allow(clock-in-ui): the sync thread's own schedule, not view code
        let now = Instant::now();
        let events = self.events.clone();
        let Some(engine) = self.engine() else { return };
        if local_only && !engine.has_local_changes().unwrap_or(true) {
            return;
        }
        let _ = events.send(Event::Status(Status::Syncing));
        let result = engine.sync();
        match result {
            Ok(report) => {
                self.backoff = None;
                self.next_at = Some(now + EVERY);
                self.send(Event::Status(Status::Synced { at: Timestamp::now(), problems: report.problems }));
                if let Some(session) = &self.session
                    && let Ok(usage) = self.remote.usage(&session.token)
                {
                    let picture = usage.avatar;
                    self.send(Event::Usage(usage));
                    self.follow_picture(picture);
                }
            }
            Err(SyncError::Remote(RemoteError::Unauthorized)) => {
                let _ = account::forget(&*self.keys);
                self.signed_out();
                self.send(Event::Status(Status::Failed("The session has ended. Sign in again.".into())));
            }
            Err(e) => {
                let offline = matches!(e, SyncError::Remote(RemoteError::Offline(_)))
                    || matches!(&e, SyncError::Remote(RemoteError::Api { status, .. }) if *status == 429 || *status >= 500);
                let wait = self.backoff.map_or(BACKOFF_FIRST, |b| (b * 2).min(BACKOFF_MAX));
                self.backoff = Some(wait);
                self.next_at = Some(now + if offline { wait } else { EVERY });
                self.send(Event::Status(if offline { Status::Offline } else { Status::Failed(e.to_string()) }));
            }
        }
    }

    /// Watches the library for edits, made in the app or anywhere else.
    fn watch(&mut self) {
        self.watcher = None;
        let Some(root) = &self.root else { return };
        let tx = self.commands.clone();
        let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if res.is_ok_and(|e| matches!(e.kind, EventKind::Access(_))) {
                return;
            }
            if let Some(tx) = tx.upgrade() {
                let _ = tx.send(Command::LocalChange);
            }
        });
        let Ok(mut watcher) = watcher else { return };
        if watcher.watch(root, RecursiveMode::Recursive).is_ok() {
            self.watcher = Some(watcher);
        }
    }
}
