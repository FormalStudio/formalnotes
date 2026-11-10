//! "What a device does" from `docs/protocol.md`: one sync is a pull of every page, each change
//! applied to the library on disk, then pushes of what changed here until nothing is left or
//! the rounds run out. It works between the folder and the server only; the app sees its writes
//! through the file watcher it already has, and it sees the app's edits as changed files.
//!
//! A file is never overwritten while it differs from the hash it last synced: that is a
//! conflict, and the local text survives it as a file of its own. A path the app holds unsaved
//! typing for is left alone until the typing is saved.

use std::collections::{BTreeMap, HashSet};
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::crypto::{Key, new_id};
use super::local::{self, Entry, Scan};
use super::object::{Kind, Payload};
use super::remote::{BLOB_LIMIT, Change, Outcome, PULL_LIMIT, PUSH_LIMIT, Push, Remote, RemoteError};
use super::state::{Sent, State, Tracked};
use crate::library::disk::Root;
use crate::library::model::rebase;

/// Paths the app holds unsaved edits for.
pub type Busy = Arc<Mutex<HashSet<String>>>;

/// Push rounds per sync: a conflict answered in one round is pushed again in the next.
const ROUNDS: usize = 4;

#[derive(Debug)]
pub enum SyncError {
    Remote(RemoteError),
    Io(io::Error),
}

impl std::fmt::Display for SyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncError::Remote(e) => e.fmt(f),
            SyncError::Io(e) => e.fmt(f),
        }
    }
}

impl From<RemoteError> for SyncError {
    fn from(e: RemoteError) -> Self {
        SyncError::Remote(e)
    }
}

impl From<io::Error> for SyncError {
    fn from(e: io::Error) -> Self {
        SyncError::Io(e)
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Report {
    pub pulled: usize,
    pub pushed: usize,
    pub conflicts: usize,
    /// Objects that could not sync and why; the rest of the sync went on without them.
    pub problems: Vec<String>,
}

pub struct Engine {
    root: Root,
    remote: Arc<dyn Remote>,
    token: String,
    vault: Key,
    state: State,
    state_path: PathBuf,
    busy: Busy,
    page: u32,
}

enum Intent {
    Create(String, Entry),
    Update(String, Entry),
    Delete(String),
}

impl Engine {
    pub fn new(
        root: PathBuf,
        remote: Arc<dyn Remote>,
        token: String,
        vault: Key,
        state_path: PathBuf,
        busy: Busy,
    ) -> io::Result<Self> {
        let state = State::load(&state_path)?;
        Ok(Self { root: Root::new(root), remote, token, vault, state, state_path, busy, page: PULL_LIMIT })
    }

    /// Pulls in pages of `page` changes instead of the server's maximum.
    pub fn with_page(mut self, page: u32) -> Self {
        self.page = page;
        self
    }

    pub fn root(&self) -> &std::path::Path {
        self.root.path()
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn sync(&mut self) -> Result<Report, SyncError> {
        let mut report = Report::default();
        self.retry_deferred(&mut report)?;
        self.pull(&mut report)?;
        for _ in 0..ROUNDS {
            if !self.push(&mut report)? {
                break;
            }
        }
        Ok(report)
    }

    /// Whether the library differs from what last synced: a cheap check before going online.
    pub fn has_local_changes(&self) -> io::Result<bool> {
        let scan = local::scan(&self.root)?;
        let mut seen = HashSet::new();
        for t in self.state.objects.values() {
            match scan.get(&t.path).filter(|e| e.kind == t.kind) {
                Some(e) if e.hash == t.hash && t.path == t.synced_path => {
                    seen.insert(t.path.as_str());
                }
                _ => return Ok(true),
            }
        }
        Ok(scan.keys().any(|p| !seen.contains(p.as_str())) || !self.state.deferred.is_empty())
    }

    fn save(&self) -> io::Result<()> {
        self.state.save(&self.state_path)
    }

    fn is_busy(&self, path: &str) -> bool {
        if path.is_empty() {
            return false;
        }
        let busy = match self.busy.lock() {
            Ok(b) => b,
            Err(poisoned) => poisoned.into_inner(),
        };
        busy.iter().any(|b| rebase(b, path, "").is_some())
    }

    fn retry_deferred(&mut self, report: &mut Report) -> io::Result<()> {
        let held: Vec<Change> = self.state.deferred.values().cloned().collect();
        for change in held {
            self.apply_change(change, report)?;
        }
        self.save()
    }

    fn pull(&mut self, report: &mut Report) -> Result<(), SyncError> {
        loop {
            let page = self.remote.pull(&self.token, self.state.cursor, self.page)?;
            for change in page.changes {
                self.apply_change(change, report)?;
            }
            self.state.cursor = page.cursor;
            self.save()?;
            if !page.more {
                return Ok(());
            }
        }
    }

    fn open(&self, change: &Change, report: &mut Report) -> Result<Option<Payload>, ()> {
        match (&change.blob, change.deleted) {
            (Some(blob), false) => match Payload::open(blob, &self.vault, &change.id) {
                Ok(p) => Ok(Some(p)),
                Err(e) => {
                    report.problems.push(format!("object {}: {e}", change.id));
                    Err(())
                }
            },
            _ => Ok(None),
        }
    }

    fn apply_change(&mut self, change: Change, report: &mut Report) -> io::Result<()> {
        let Ok(payload) = self.open(&change, report) else { return Ok(()) };
        if let Some(p) = &payload
            && let Err(e) = local::checked(&self.root, &p.path, p.kind)
        {
            report.problems.push(format!("object {}: {e}", change.id));
            return Ok(());
        }
        let known = self.state.objects.get(&change.id).map(|t| t.seq);
        if known.is_none_or(|seq| change.seq > seq) {
            report.pulled += 1;
        }
        self.apply(&change, payload, report)
    }

    fn defer(&mut self, change: &Change) -> io::Result<()> {
        self.state.deferred.insert(change.id.clone(), change.clone());
        Ok(())
    }

    /// Where an object lands: `path`, or a numbered name beside it when something else is there.
    fn place(&self, path: &str, kind: Kind, current: Option<&str>) -> io::Result<String> {
        if current.is_some_and(|c| c.eq_ignore_ascii_case(path)) {
            return Ok(path.to_owned());
        }
        local::free_path(&self.root, path, kind)
    }

    fn rebase_tree(&mut self, from: &str, to: &str) {
        for t in self.state.objects.values_mut() {
            if let Some(path) = rebase(&t.path, from, to) {
                t.path = path;
            }
        }
    }

    /// The server's state for one object, from a pull or a conflict.
    fn apply(&mut self, change: &Change, payload: Option<Payload>, report: &mut Report) -> io::Result<()> {
        let id = change.id.clone();
        let seq = change.seq;
        let Some(mut t) = self.state.objects.get(&id).cloned() else { return self.apply_new(change, payload, report) };
        if seq <= t.seq {
            self.state.deferred.remove(&id);
            return Ok(());
        }
        if self.is_busy(&t.path) || payload.as_ref().is_some_and(|p| self.is_busy(&p.path)) {
            return self.defer(change);
        }
        self.state.deferred.remove(&id);
        // A push whose answer was lost: the server holds what this device sent.
        if let Some(sent) = t.sent.take() {
            let landed = match &payload {
                None => sent.deleted,
                Some(p) => !sent.deleted && p.path == sent.path && p.hash() == sent.hash,
            };
            if landed && payload.is_none() {
                self.state.objects.remove(&id);
                return Ok(());
            }
            if landed {
                t.hash = sent.hash;
                t.synced_path = sent.path;
                t.seq = seq;
                self.state.objects.insert(id, t);
                return Ok(());
            }
        }
        let local = (!t.path.is_empty()).then(|| local::read(&self.root, &t.path, t.kind)).flatten();
        let Some(p) = payload else {
            match local {
                None => {
                    self.state.objects.remove(&id);
                }
                Some(l) if l.hash == t.hash && t.path == t.synced_path => {
                    if t.kind == Kind::Folder && !local::is_empty_folder(&self.root, &t.path) {
                        // Something here still lives in it: the folder is pushed again.
                        t.seq = seq;
                        t.hash.clear();
                        self.state.objects.insert(id, t);
                    } else {
                        local::trash(&self.root, &t.path, t.kind)?;
                        self.state.objects.remove(&id);
                    }
                }
                Some(_) => {
                    // Edited here, deleted there: the edit is pushed again over the delete.
                    t.seq = seq;
                    self.state.objects.insert(id, t);
                }
            }
            return Ok(());
        };
        let server_hash = p.hash();
        let tracked = |path: String| Tracked {
            kind: p.kind,
            path,
            synced_path: p.path.clone(),
            hash: server_hash.clone(),
            seq,
            sent: None,
        };
        let Some(l) = local else {
            if t.path.is_empty() && t.kind == Kind::File {
                // Superseded by another view file: its delete goes out again on top of this.
                t.seq = seq;
                self.state.objects.insert(id, t);
                return Ok(());
            }
            // Deleted here, edited there: the server's version comes back.
            let path = self.place(&p.path, p.kind, None)?;
            local::write(&self.root, &path, &p)?;
            self.state.objects.insert(id, tracked(path));
            return Ok(());
        };
        if l.hash == server_hash && t.path == p.path {
            self.state.objects.insert(id, tracked(t.path));
            return Ok(());
        }
        let local_text = l.hash != t.hash;
        let server_text = server_hash != t.hash;
        if local_text && server_text && p.kind == Kind::Note {
            // Both edited: the server's version takes the file, the local text a file beside it.
            report.conflicts += 1;
            let copy = local::conflict_path(&self.root, &p.path, jiff::Zoned::now())?;
            local::rename(&self.root, &t.path, &copy, Kind::Note)?;
            let path = self.place(&p.path, p.kind, None)?;
            local::write(&self.root, &path, &p)?;
            self.state.objects.insert(id, tracked(path));
            return Ok(());
        }
        // Each side keeps what it changed; a `.folder.json` both sides edited takes the server's.
        let mut path = t.path.clone();
        if t.path == t.synced_path && p.path != t.path {
            let to = self.place(&p.path, p.kind, Some(&t.path))?;
            local::rename(&self.root, &t.path, &to, p.kind)?;
            if p.kind == Kind::Folder {
                self.rebase_tree(&t.path, &to);
            }
            path = to;
        }
        let keep_local_text = local_text && !server_text;
        if !keep_local_text && l.hash != server_hash {
            local::write(&self.root, &path, &p)?;
        }
        self.state.objects.insert(id, tracked(path));
        Ok(())
    }

    /// An object this device has not tracked before.
    fn apply_new(&mut self, change: &Change, payload: Option<Payload>, report: &mut Report) -> io::Result<()> {
        let id = change.id.clone();
        let Some(p) = payload else {
            self.state.deferred.remove(&id);
            return Ok(());
        };
        if self.is_busy(&p.path) {
            return self.defer(change);
        }
        self.state.deferred.remove(&id);
        let hash = p.hash();
        let track = |path: String, synced_hash: String| Tracked {
            kind: p.kind,
            path,
            synced_path: p.path.clone(),
            hash: synced_hash,
            seq: change.seq,
            sent: None,
        };
        let existing = local::read(&self.root, &p.path, p.kind);
        let owner = self.state.at(&p.path).map(str::to_owned);
        let Some(l) = existing else {
            if let Some(owner) = owner.and_then(|o| self.state.objects.get_mut(&o)) {
                // Its file is gone here, so its delete is still to be pushed.
                owner.path.clear();
            }
            local::write(&self.root, &p.path, &p)?;
            self.state.objects.insert(id, track(p.path.clone(), hash));
            return Ok(());
        };
        match (owner, p.kind) {
            // First sync, or the same folder made on two devices: the remote id is adopted.
            (None, Kind::Folder) | (Some(_), Kind::Folder) => {
                self.state.objects.insert(id, track(p.path.clone(), hash));
            }
            (None, _) if l.hash == hash => {
                self.state.objects.insert(id, track(p.path.clone(), hash));
            }
            // A view file made here before this one arrived: this device's text is pushed over it.
            (None, Kind::File) => {
                self.state.objects.insert(id, track(p.path.clone(), hash));
            }
            // A second view file for one folder: this one is deleted, the tracked one stays.
            (Some(_), Kind::File) => {
                self.state.objects.insert(id, track(String::new(), hash));
            }
            // The same path under two ids: the one that reached the server second takes `<name> 2`.
            (owner, _) => {
                report.conflicts += 1;
                let owner_seq = owner.as_ref().and_then(|o| self.state.objects.get(o)).map_or(0, |t| t.seq);
                if owner_seq == 0 || owner_seq > change.seq {
                    let to = local::free_path(&self.root, &p.path, p.kind)?;
                    local::rename(&self.root, &p.path, &to, p.kind)?;
                    if let Some(o) = owner.and_then(|o| self.state.objects.get_mut(&o)) {
                        o.path = to;
                    }
                    local::write(&self.root, &p.path, &p)?;
                    self.state.objects.insert(id, track(p.path.clone(), hash));
                } else {
                    let to = local::free_path(&self.root, &p.path, p.kind)?;
                    local::write(&self.root, &to, &p)?;
                    self.state.objects.insert(id, track(to, hash));
                }
            }
        }
        Ok(())
    }

    /// Finds what changed here since the last sync. Moves keep their id: a folder whose
    /// contents reappear under another folder moved there, and a note gone from one path while
    /// one with its text appeared at another moved there.
    fn plan(&mut self, scan: &Scan) -> Vec<Intent> {
        let present = |t: &Tracked| !t.path.is_empty() && scan.get(&t.path).is_some_and(|e| e.kind == t.kind);
        let tracked_paths: HashSet<String> = self.state.objects.values().map(|t| t.path.clone()).collect();
        let mut untracked: BTreeMap<String, Entry> =
            scan.iter().filter(|(p, _)| !tracked_paths.contains(*p)).map(|(p, e)| (p.clone(), e.clone())).collect();
        let mut missing: Vec<String> = self
            .state
            .objects
            .iter()
            .filter(|(_, t)| !t.path.is_empty() && !present(t))
            .map(|(id, _)| id.clone())
            .collect();
        missing.sort_by_key(|id| self.state.objects.get(id).map_or(0, |t| t.path.len()));

        for id in &missing {
            let Some(t) = self.state.objects.get(id) else { continue };
            if t.kind != Kind::Folder || present(t) {
                continue;
            }
            let from = t.path.clone();
            if let Some(to) = self.folder_move(&from, &untracked, scan) {
                self.rebase_tree(&from, &to);
                untracked.retain(|p, _| rebase(p, &to, "").is_none());
            }
        }
        for id in &missing {
            let Some(t) = self.state.objects.get(id) else { continue };
            if t.kind == Kind::Folder || present(t) {
                continue;
            }
            let name = t.path.rsplit('/').next().unwrap_or_default().to_owned();
            let candidates: Vec<&String> =
                untracked.iter().filter(|(_, e)| e.kind == t.kind && e.hash == t.hash).map(|(p, _)| p).collect();
            let same_name: Vec<&String> =
                candidates.iter().copied().filter(|p| p.rsplit('/').next() == Some(&name)).collect();
            let to = match (same_name.as_slice(), candidates.as_slice()) {
                ([one], _) | ([], [one]) => Some((*one).clone()),
                _ => None,
            };
            if let Some(to) = to
                && let Some(t) = self.state.objects.get_mut(id)
            {
                untracked.remove(&to);
                t.path = to;
            }
        }

        let mut intents = Vec::new();
        let mut deletes = Vec::new();
        for (id, t) in &self.state.objects {
            match (!t.path.is_empty()).then(|| scan.get(&t.path)).flatten().filter(|e| e.kind == t.kind) {
                Some(e) if e.hash != t.hash || t.path != t.synced_path => {
                    intents.push(Intent::Update(id.clone(), e.clone()))
                }
                Some(_) => {}
                None => deletes.push((t.path.clone(), id.clone())),
            }
        }
        intents.extend(untracked.into_iter().map(|(path, e)| Intent::Create(path, e)));
        let depth = |p: &str| p.matches('/').count();
        intents.sort_by_key(|i| match i {
            Intent::Create(path, _) => depth(path),
            Intent::Update(id, _) => self.state.objects.get(id).map_or(0, |t| depth(&t.path)),
            Intent::Delete(_) => 0,
        });
        deletes.sort_by_key(|(path, _)| std::cmp::Reverse(depth(path)));
        intents.extend(deletes.into_iter().map(|(_, id)| Intent::Delete(id)));
        intents
    }

    fn folder_move(&self, from: &str, untracked: &BTreeMap<String, Entry>, scan: &Scan) -> Option<String> {
        let folders = || untracked.iter().filter(|(_, e)| e.kind == Kind::Folder).map(|(p, _)| p);
        let children: Vec<&Tracked> =
            self.state.objects.values().filter(|t| t.path != from && rebase(&t.path, from, "").is_some()).collect();
        if children.is_empty() {
            // An empty folder: renamed in place, or moved under its own name, when only one fits.
            let parent = |p: &str| p.rsplit_once('/').map_or("", |(p, _)| p).to_owned();
            let name = |p: &str| p.rsplit('/').next().unwrap_or_default().to_owned();
            let near: Vec<&String> = folders().filter(|g| parent(g) == parent(from) || name(g) == name(from)).collect();
            return match near.as_slice() {
                [one] => Some((*one).clone()),
                _ => None,
            };
        }
        let fits: Vec<&String> = folders()
            .filter(|g| {
                children.iter().any(|c| {
                    rebase(&c.path, from, g).and_then(|moved| scan.get(&moved)).is_some_and(|e| {
                        e.kind == c.kind
                            && e.hash == c.hash
                            && untracked.contains_key(&rebase(&c.path, from, g).unwrap_or_default())
                    })
                })
            })
            .collect();
        match fits.as_slice() {
            [one] => Some((*one).clone()),
            _ => None,
        }
    }

    /// One round of pushes. Returns whether there was anything to push.
    fn push(&mut self, report: &mut Report) -> Result<bool, SyncError> {
        let scan = local::scan(&self.root)?;
        let intents = self.plan(&scan);
        if intents.is_empty() {
            return Ok(false);
        }
        let mut pushes = Vec::new();
        for intent in intents {
            let push = match intent {
                Intent::Create(path, entry) => {
                    let id = new_id().map_err(|e| io::Error::other(e.to_string()))?;
                    let Some(blob) = self.blob(&id, &path, &entry, report) else { continue };
                    self.state.objects.insert(
                        id.clone(),
                        Tracked {
                            kind: entry.kind,
                            path: path.clone(),
                            synced_path: path.clone(),
                            hash: String::new(),
                            seq: 0,
                            sent: Some(Sent { path, hash: entry.hash, deleted: false }),
                        },
                    );
                    Push { id, base: 0, deleted: false, blob: Some(blob) }
                }
                Intent::Update(id, entry) => {
                    let Some(path) = self.state.objects.get(&id).map(|t| t.path.clone()) else { continue };
                    let Some(blob) = self.blob(&id, &path, &entry, report) else { continue };
                    let Some(t) = self.state.objects.get_mut(&id) else { continue };
                    t.sent = Some(Sent { path, hash: entry.hash, deleted: false });
                    Push { id, base: t.seq, deleted: false, blob: Some(blob) }
                }
                Intent::Delete(id) => {
                    let Some(t) = self.state.objects.get_mut(&id) else { continue };
                    t.sent = Some(Sent { path: t.path.clone(), hash: String::new(), deleted: true });
                    Push { id, base: t.seq, deleted: true, blob: None }
                }
            };
            pushes.push(push);
        }
        if pushes.is_empty() {
            return Ok(false);
        }
        // What is about to be sent is on disk first, so a lost answer settles on the next pull.
        self.save()?;
        for batch in pushes.chunks(PUSH_LIMIT) {
            let outcomes = match self.remote.push(&self.token, batch) {
                Ok(outcomes) => outcomes,
                Err(e) => {
                    // A rejected request kept nothing; only a lost answer may have landed.
                    if matches!(e, RemoteError::Api { .. }) {
                        for push in batch {
                            if let Some(t) = self.state.objects.get_mut(&push.id) {
                                t.sent = None;
                            }
                        }
                        self.save()?;
                    }
                    return Err(e.into());
                }
            };
            for outcome in outcomes {
                match outcome {
                    Outcome::Ok { id, seq } => {
                        report.pushed += 1;
                        let Some(t) = self.state.objects.get_mut(&id) else { continue };
                        match t.sent.take() {
                            Some(sent) if sent.deleted => {
                                self.state.objects.remove(&id);
                            }
                            Some(sent) => {
                                t.seq = seq;
                                t.hash = sent.hash;
                                t.synced_path = sent.path;
                            }
                            None => t.seq = seq,
                        }
                    }
                    Outcome::Conflict { id, seq: 0, deleted: true, blob: None } => {
                        // The server never saw this id: it is created, not updated.
                        let Some(t) = self.state.objects.get_mut(&id) else { continue };
                        if t.sent.take().is_some_and(|s| s.deleted) {
                            self.state.objects.remove(&id);
                        } else {
                            t.seq = 0;
                        }
                    }
                    Outcome::Conflict { id, seq, deleted, blob } => {
                        let change = Change { id, seq, deleted, blob };
                        self.apply_change(change, report)?;
                    }
                }
            }
            self.save()?;
        }
        Ok(true)
    }

    fn blob(&self, id: &str, path: &str, entry: &Entry, report: &mut Report) -> Option<String> {
        let sealed = entry.payload(path).seal(&self.vault, id);
        match sealed {
            Ok(blob) if blob.len() / 4 * 3 <= BLOB_LIMIT => Some(blob),
            Ok(_) => {
                report.problems.push(format!("{path} is too large to sync"));
                None
            }
            Err(e) => {
                report.problems.push(format!("{path}: {e}"));
                None
            }
        }
    }
}
