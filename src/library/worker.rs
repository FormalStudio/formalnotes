//! The one thread that touches the library on disk. It runs the store's operations in order,
//! holds autosaves until they are due, watches the root for edits made outside the app, and
//! answers with fresh snapshots. Being one thread is what keeps a save from racing a rename.

use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use super::debounce::Debouncer;
use super::disk::{self, Root, ScanCache};
use super::model::{FolderId, Library, NoteId};

/// External edits arrive as bursts of events; one rescan follows the burst.
const WATCH_SETTLE: Duration = Duration::from_millis(150);

#[derive(Debug)]
pub enum Op {
    /// Points the worker at another library root.
    Open(PathBuf),
    Save {
        note: NoteId,
        text: Arc<str>,
        generation: u64,
    },
    /// Writes every pending save now: before quitting or switching notes.
    Flush,
    CreateNote {
        note: NoteId,
        text: String,
    },
    CreateFolder(FolderId),
    Rename {
        from: String,
        to: String,
    },
    Trash {
        path: String,
        name: String,
    },
    Restore {
        name: String,
        to: String,
    },
    EmptyTrash,
    /// Writes a folder's `.folder.json`.
    WriteView {
        folder: FolderId,
        bytes: Vec<u8>,
    },
    /// Something changed on disk.
    Changed,
    /// Answered with [`Event::Pong`] once every earlier operation is done.
    Ping(u64),
}

impl Op {
    /// Operations that change what is on disk, counted on both sides so the store can tell a
    /// snapshot taken before its latest change from one taken after.
    pub fn changes_library(&self) -> bool {
        matches!(
            self,
            Op::Open(_)
                | Op::CreateNote { .. }
                | Op::CreateFolder(_)
                | Op::Rename { .. }
                | Op::Trash { .. }
                | Op::Restore { .. }
                | Op::EmptyTrash
                | Op::WriteView { .. }
        )
    }
}

#[derive(Debug)]
pub enum Event {
    /// The library as scanned after `applied` library-changing operations.
    Snapshot {
        library: Arc<Library>,
        applied: u64,
    },
    Saved {
        generation: u64,
    },
    Failed(String),
    Pong(u64),
}

pub struct Worker {
    pub ops: flume::Sender<Op>,
    pub events: flume::Receiver<Event>,
}

/// Starts the worker on `root`.
pub fn start(root: PathBuf) -> Worker {
    let (ops, rx) = flume::unbounded();
    let (tx, events) = flume::unbounded();
    let watch_tx = ops.downgrade();
    let _ = std::thread::Builder::new().name("formalnotes-disk".into()).spawn(move || {
        let mut state = State {
            root: Root::new(root),
            cache: ScanCache::default(),
            saves: Debouncer::default(),
            last: None,
            tx,
            watcher: None,
            watch_tx,
            rescan_at: None,
            dirty: true,
            applied: 0,
        };
        state.watch();
        state.run(&rx);
    });
    Worker { ops, events }
}

struct State {
    root: Root,
    cache: ScanCache,
    saves: Debouncer,
    last: Option<(Arc<Library>, u64)>,
    tx: flume::Sender<Event>,
    watcher: Option<RecommendedWatcher>,
    /// Weak, so the channel closes (and the worker flushes and ends) once the store is gone.
    watch_tx: flume::WeakSender<Op>,
    rescan_at: Option<Instant>,
    dirty: bool,
    applied: u64,
}

impl State {
    fn run(&mut self, rx: &flume::Receiver<Op>) {
        loop {
            if self.dirty && rx.is_empty() {
                self.rescan();
            }
            // The worker's own clock: autosave deadlines, never the UI's.
            // nd-allow(clock-in-ui): a background thread, not view code
            let now = Instant::now();
            let deadline = [self.saves.next_due(), self.rescan_at].into_iter().flatten().min();
            let op = match deadline {
                Some(at) => match rx.recv_timeout(at.saturating_duration_since(now)) {
                    Ok(op) => Some(op),
                    Err(flume::RecvTimeoutError::Timeout) => None,
                    Err(flume::RecvTimeoutError::Disconnected) => break,
                },
                None => match rx.recv() {
                    Ok(op) => Some(op),
                    Err(_) => break,
                },
            };
            match op {
                Some(op) => self.apply(op),
                None => self.tick(),
            }
        }
        // The store is gone (the app quit): nothing typed is lost.
        let pending = self.saves.take_all();
        self.write(pending);
    }

    fn tick(&mut self) {
        // nd-allow(clock-in-ui): a background thread, not view code
        let now = Instant::now();
        let due = self.saves.take_due(now);
        self.write(due);
        if self.rescan_at.is_some_and(|at| at <= now) {
            self.rescan_at = None;
            self.dirty = true;
        }
    }

    fn apply(&mut self, op: Op) {
        if op.changes_library() {
            self.applied += 1;
        }
        let result = match op {
            Op::Open(path) => {
                let all = self.saves.take_all();
                self.write(all);
                self.root = Root::new(path);
                self.cache = ScanCache::default();
                self.last = None;
                self.watch();
                Ok(())
            }
            Op::Save { note, text, generation } => {
                // nd-allow(clock-in-ui): a background thread, not view code
                self.saves.push(note, text, generation, Instant::now());
                return;
            }
            Op::Flush => {
                let all = self.saves.take_all();
                self.write(all);
                return;
            }
            Op::CreateNote { note, text } => disk::create_note(&self.root, &note, &text),
            Op::CreateFolder(folder) => disk::create_folder(&self.root, &folder),
            Op::Rename { from, to } => {
                let pending = self.saves.take_under(&from);
                self.write(pending);
                disk::rename(&self.root, &from, &to)
            }
            Op::Trash { path, name } => {
                let pending = self.saves.take_under(&path);
                self.write(pending);
                disk::trash(&self.root, &path, &name)
            }
            Op::Restore { name, to } => disk::restore(&self.root, &name, &to),
            Op::EmptyTrash => disk::empty_trash(&self.root),
            Op::WriteView { folder, bytes } => disk::write_view(&self.root, &folder, &bytes),
            Op::Changed => {
                // nd-allow(clock-in-ui): a background thread, not view code
                self.rescan_at = Some(Instant::now() + WATCH_SETTLE);
                return;
            }
            Op::Ping(n) => {
                if self.dirty {
                    self.rescan();
                }
                let _ = self.tx.send(Event::Pong(n));
                return;
            }
        };
        self.report(result);
        self.dirty = true;
    }

    fn write(&mut self, saves: Vec<(NoteId, super::debounce::Pending)>) {
        for (note, pending) in saves {
            let result = disk::save(&self.root, &note, &pending.text);
            if result.is_ok() {
                let _ = self.tx.send(Event::Saved { generation: pending.generation });
            }
            self.report(result);
        }
    }

    fn report(&self, result: io::Result<()>) {
        if let Err(e) = result {
            let _ = self.tx.send(Event::Failed(e.to_string()));
        }
    }

    fn rescan(&mut self) {
        self.dirty = false;
        match disk::scan(&self.root, &mut self.cache) {
            Ok(lib) => {
                if self.last.as_ref().is_none_or(|(last, applied)| **last != lib || *applied != self.applied) {
                    let lib = Arc::new(lib);
                    self.last = Some((lib.clone(), self.applied));
                    let _ = self.tx.send(Event::Snapshot { library: lib, applied: self.applied });
                }
            }
            Err(e) => self.report(Err(e)),
        }
    }

    /// Watches the root for changes made by other apps (and the worker's own writes, which a
    /// rescan finds unchanged).
    fn watch(&mut self) {
        self.watcher = None;
        let tx = self.watch_tx.clone();
        let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if res.is_ok_and(|e| matches!(e.kind, EventKind::Access(_))) {
                return;
            }
            if let Some(tx) = tx.upgrade() {
                let _ = tx.send(Op::Changed);
            }
        });
        let Ok(mut watcher) = watcher else { return };
        let _ = std::fs::create_dir_all(self.root.path());
        if watcher.watch(self.root.path(), RecursiveMode::Recursive).is_ok() {
            self.watcher = Some(watcher);
        }
    }
}
