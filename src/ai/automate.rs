//! Runs the automations: watches the library for notes that are due, sends one at a time to the
//! model, files what comes back, and keeps the outcome for the window, which observes it.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use jiff::{SignedDuration, Timestamp};
use nd::prelude::*;

use super::chat::{AiError, Cancel};
use super::organize::{self, Applied, Automation, Job, Plan, Trigger};
use super::properties::FolderColumns;
use super::settings::connector;
use crate::library::{NoteId, Store};
use crate::settings::NotesSettings;

/// How often waiting notes are looked at again.
const TICK: Duration = Duration::from_secs(30);
/// A failed note is tried again after this, doubling each time, and given up on after
/// [`ATTEMPTS`] tries until its text changes.
const RETRY: SignedDuration = SignedDuration::from_mins(5);
const ATTEMPTS: u32 = 4;
/// After a failure Settings has to fix (no key, a rejected key), nothing runs for this long.
const QUIET: SignedDuration = SignedDuration::from_mins(30);

/// What the window hears from the automations.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Organized(Applied),
    Failed { title: String, error: AiError },
}

struct Failure {
    attempts: u32,
    retry_at: Timestamp,
}

struct Running {
    job: Arc<Job>,
    manual: bool,
    _cancel: Cancel,
}

type Finished = (u64, Result<Plan, AiError>);

pub struct Automations {
    store: Entity<Store>,
    open: Option<NoteId>,
    running: Option<(u64, Running)>,
    next_run: u64,
    /// Notes (by path and text) already organized or on their way: never picked up again.
    handled: HashSet<(NoteId, u64)>,
    failures: HashMap<(NoteId, u64), Failure>,
    quiet_until: Option<Timestamp>,
    finished_tx: flume::Sender<Finished>,
    finished_rx: flume::Receiver<Finished>,
    /// What happened since the window last took it.
    outcomes: Vec<Outcome>,
    /// The library's folder columns. Empty until the library models them.
    pub columns: FolderColumns,
}

impl Automations {
    pub fn new(store: Entity<Store>, cx: &mut Context<Self>) -> Self {
        let (finished_tx, finished_rx) = flume::unbounded();
        cx.observe(&store, |this, _, cx| this.check(Timestamp::now(), cx)).detach();
        if crate::pump::live() {
            crate::pump::pump(finished_rx.clone(), cx.weak_entity(), |this: &mut Self, batch, cx| {
                for (run, result) in batch {
                    this.finish(run, result, cx);
                }
                true
            });
            let (tick_tx, tick_rx) = flume::bounded(1);
            let _ = std::thread::Builder::new().name("formalnotes-automations".into()).spawn(move || {
                loop {
                    // nd-allow(clock-in-ui): a background ticker thread, not view code
                    std::thread::sleep(TICK);
                    if tick_tx.send(()).is_err() {
                        break;
                    }
                }
            });
            crate::pump::pump(tick_rx, cx.weak_entity(), |this: &mut Self, _, cx| {
                this.check(Timestamp::now(), cx);
                true
            });
        }
        Self {
            store,
            open: None,
            running: None,
            next_run: 0,
            handled: HashSet::new(),
            failures: HashMap::new(),
            quiet_until: None,
            finished_tx,
            finished_rx,
            outcomes: Vec::new(),
            columns: FolderColumns::default(),
        }
    }

    /// The note open in the editor, which is never picked up while it is open.
    pub fn set_open(&mut self, open: Option<NoteId>) {
        self.open = open;
    }

    /// The outcomes since the last call, oldest first.
    pub fn take_outcomes(&mut self) -> Vec<Outcome> {
        std::mem::take(&mut self.outcomes)
    }

    pub fn is_running(&self) -> bool {
        self.running.is_some()
    }

    /// Starts the first due note of every automation that is on, unless one is running.
    pub fn check(&mut self, now: Timestamp, cx: &mut Context<Self>) {
        if self.running.is_some() || self.quiet_until.is_some_and(|until| now < until) {
            return;
        }
        let settings = cx.settings::<NotesSettings>().clone();
        for automation in organize::AUTOMATIONS {
            let Some(trigger) = (automation.trigger)(&settings) else { continue };
            if let Some(note) = self.next_due(&trigger, now, cx) {
                self.start(automation, &note, trigger, false, cx);
                return;
            }
        }
    }

    fn next_due(&self, trigger: &Trigger, now: Timestamp, cx: &App) -> Option<NoteId> {
        let store = self.store.read(cx);
        store
            .library()
            .notes_in(&trigger.folder, crate::library::Sort::Modified)
            .into_iter()
            .rev()
            .filter(|n| organize::due(n, trigger, self.open.as_ref(), store.is_dirty(&n.id), now))
            .find(|n| {
                let key = (n.id.clone(), organize::text_hash(&n.text));
                !self.handled.contains(&key)
                    && self.failures.get(&key).is_none_or(|f| f.attempts < ATTEMPTS && now >= f.retry_at)
            })
            .map(|n| n.id.clone())
    }

    /// Organizes `note` now, whatever the triggers say ("Organize This Note"). Returns false
    /// while another run is in flight.
    pub fn organize_now(&mut self, note: &NoteId, cx: &mut Context<Self>) -> bool {
        if self.running.is_some() || self.store.read(cx).note(note).is_none() {
            return false;
        }
        let dumps = cx.settings::<NotesSettings>().brain_dumps.clone();
        let trigger = Trigger {
            folder: crate::library::FolderId(dumps.inbox_folder.trim().trim_matches('/').to_owned()),
            idle: SignedDuration::ZERO,
            extra: dumps.extra_instructions.trim().to_owned(),
        };
        self.start(&organize::ORGANIZE_BRAIN_DUMPS, note, trigger, true, cx);
        true
    }

    fn start(
        &mut self,
        automation: &'static Automation,
        note: &NoteId,
        trigger: Trigger,
        manual: bool,
        cx: &mut Context<Self>,
    ) {
        let store = self.store.read(cx);
        let Some(found) = store.note(note) else { return };
        let job = Arc::new(Job::new(automation, found, store.library(), &self.columns, trigger.folder, trigger.extra));
        self.handled.insert((job.note.clone(), job.hash));
        self.next_run += 1;
        let run = self.next_run;
        let cancel: Cancel = Arc::new(AtomicBool::new(false));
        let assistant = cx.settings::<NotesSettings>().assistant.clone();
        let connect = connector(cx);
        let (tx, thread_job, thread_cancel) = (self.finished_tx.clone(), job.clone(), cancel.clone());
        let _ = std::thread::Builder::new().name("formalnotes-organize".into()).spawn(move || {
            let result = connect.connect(&assistant).and_then(|t| thread_job.run(t.as_ref(), &thread_cancel));
            let _ = tx.send((run, result));
        });
        self.running = Some((run, Running { job, manual, _cancel: cancel }));
        cx.notify();
    }

    fn finish(&mut self, run: u64, result: Result<Plan, AiError>, cx: &mut Context<Self>) {
        let Some((current, running)) = self.running.take() else { return };
        if current != run {
            self.running = Some((current, running));
            return;
        }
        let job = &running.job;
        let key = (job.note.clone(), job.hash);
        let title = self.store.read(cx).note(&job.note).map_or_else(|| job.note.stem().to_owned(), |n| n.title.clone());
        let columns = self.columns.clone();
        let applied = result.and_then(|plan| {
            self.store
                .update(cx, |store, cx| organize::apply(store, &job.note, job.hash, &plan, &columns, cx))
                .map_err(AiError::BadReply)
        });
        let now = Timestamp::now();
        match applied {
            Ok(applied) => {
                self.failures.remove(&key);
                self.outcomes.push(Outcome::Organized(applied));
            }
            Err(error) => {
                self.handled.remove(&key);
                let failure = self.failures.entry(key).or_insert(Failure { attempts: 0, retry_at: now });
                failure.attempts += 1;
                let wait = RETRY * 2i32.pow(failure.attempts.saturating_sub(1).min(8));
                failure.retry_at = now + wait;
                let first = failure.attempts == 1;
                let quiet_now = self.quiet_until.is_some_and(|until| now < until);
                if error.wants_settings() {
                    self.quiet_until = Some(now + QUIET);
                }
                if running.manual || (first && !quiet_now) {
                    self.outcomes.push(Outcome::Failed { title, error });
                }
            }
        }
        cx.notify();
        self.check(now, cx);
    }

    /// Blocks until the run in flight has finished and handles it. Tier 1 tests have no host to
    /// pump the result, so they settle instead.
    pub fn settle(&mut self, cx: &mut Context<Self>) {
        if self.running.is_none() {
            return;
        }
        if let Ok((run, result)) = self.finished_rx.recv_timeout(Duration::from_secs(10)) {
            self.finish(run, result, cx);
        }
    }
}
