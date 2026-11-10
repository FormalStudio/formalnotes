//! The library entity the views read. Every change lands here first, so the window updates on
//! the same frame, then goes to the disk worker as an operation; the worker's snapshots replace
//! the library once it has caught up. Unsaved edits are kept apart and laid over every
//! snapshot, so a scan that ran before a save can never wind the text back.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use jiff::Timestamp;
use nd::prelude::*;

use super::model::{EntryId, Folder, FolderId, Library, Note, NoteId, TrashItem, with_heading};
use super::worker::{self, Event, Op};

#[derive(Clone, Debug)]
struct Edit {
    text: Arc<str>,
    generation: u64,
}

pub struct Store {
    root: PathBuf,
    library: Arc<Library>,
    /// Unsaved text by note, newest first applied over each snapshot.
    edits: HashMap<NoteId, Edit>,
    generation: u64,
    ops: flume::Sender<Op>,
    events: flume::Receiver<Event>,
    ping: u64,
    /// Library-changing operations sent; snapshots from before the latest are stale.
    sent: u64,
    /// Every disk failure so far, oldest first; the window shows the ones it has not seen.
    failures: Vec<String>,
}

impl Store {
    /// Opens the library at `root`, creating the folder if needed. The first snapshot arrives
    /// from the worker; until then the library is empty.
    pub fn new(root: PathBuf, cx: &mut Context<Self>) -> Self {
        let worker = worker::start(root.clone());
        let this = Self {
            root,
            library: Arc::default(),
            edits: HashMap::new(),
            generation: 0,
            ops: worker.ops,
            events: worker.events,
            ping: 0,
            sent: 0,
            failures: Vec::new(),
        };
        if crate::pump::live() {
            crate::pump::pump(this.events.clone(), cx.weak_entity(), |store: &mut Store, events, cx| {
                for event in events {
                    store.handle(event, cx);
                }
                true
            });
        }
        this
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn library(&self) -> &Library {
        &self.library
    }

    pub fn note(&self, id: &NoteId) -> Option<&Note> {
        self.library.note(id)
    }

    pub fn failures(&self) -> &[String] {
        &self.failures
    }

    /// The notes with edits the worker has not written yet, by path.
    pub fn unsaved(&self) -> std::collections::HashSet<String> {
        self.edits.keys().map(|id| id.0.clone()).collect()
    }

    /// Whether `id` has edits the worker has not written yet.
    pub fn is_dirty(&self, id: &NoteId) -> bool {
        self.edits.contains_key(id)
    }

    /// Points the store at another root (the setting changed).
    pub fn open(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        if root == self.root {
            return;
        }
        self.send(Op::Open(root.clone()));
        self.root = root;
        self.library = Arc::default();
        self.edits.clear();
        cx.notify();
    }

    fn send(&mut self, op: Op) {
        if op.changes_library() {
            self.sent += 1;
        }
        let _ = self.ops.send(op);
    }

    fn lib(&mut self) -> &mut Library {
        Arc::make_mut(&mut self.library)
    }

    fn handle(&mut self, event: Event, cx: &mut Context<Self>) {
        match event {
            Event::Snapshot { library, applied } => {
                if applied < self.sent {
                    return;
                }
                self.library = library;
                let edits: Vec<(NoteId, Arc<str>)> =
                    self.edits.iter().map(|(id, e)| (id.clone(), e.text.clone())).collect();
                for (id, text) in edits {
                    if let Some(note) = self.lib().notes.get_mut(&id) {
                        let modified = note.modified;
                        note.set_text(text, modified);
                    }
                }
                cx.notify();
            }
            Event::Saved { generation } => self.edits.retain(|_, e| e.generation != generation),
            Event::Failed(message) => {
                self.failures.push(message);
                cx.notify();
            }
            Event::Pong(_) => {}
        }
    }

    /// Takes the user's newest text for a note; it is written once the typing rests.
    pub fn edit(&mut self, id: &NoteId, text: String, cx: &mut Context<Self>) {
        let text: Arc<str> = text.into();
        let Some(note) = self.lib().notes.get_mut(id) else { return };
        if note.text == text {
            return;
        }
        note.set_text(text.clone(), Timestamp::now());
        self.generation += 1;
        let generation = self.generation;
        self.edits.insert(id.clone(), Edit { text: text.clone(), generation });
        self.send(Op::Save { note: id.clone(), text, generation });
        cx.notify();
    }

    /// Writes every pending edit now.
    pub fn flush(&mut self) {
        self.send(Op::Flush);
    }

    pub fn create_note(&mut self, folder: &FolderId, cx: &mut Context<Self>) -> NoteId {
        let folder = if self.library.has_folder(folder) { folder.clone() } else { FolderId::root() };
        let id = self.library.free_note_path(&folder, "Untitled", None);
        let now = Timestamp::now();
        self.lib().insert_note(Note::new(id.clone(), Arc::from(""), now, now));
        self.send(Op::CreateNote { note: id.clone(), text: String::new() });
        cx.notify();
        id
    }

    pub fn create_folder(&mut self, parent: &FolderId, cx: &mut Context<Self>) -> FolderId {
        let parent = if self.library.has_folder(parent) { parent.clone() } else { FolderId::root() };
        let id = self.library.free_folder_path(&parent, "New Folder", None);
        let now = Timestamp::now();
        self.lib().insert_folder(Folder { id: id.clone(), created: now, modified: now });
        self.send(Op::CreateFolder(id.clone()));
        cx.notify();
        id
    }

    /// Sets a note's title: its leading heading becomes `title` and the file takes the title's
    /// name. Returns where the note is now.
    pub fn retitle(&mut self, id: &NoteId, title: &str, cx: &mut Context<Self>) -> NoteId {
        let title = title.trim();
        let Some(note) = self.library.note(id) else { return id.clone() };
        if title.is_empty() || title == note.title {
            return id.clone();
        }
        let text = with_heading(&note.text, title);
        self.edit(id, text, cx);
        let to = self.library.free_note_path(&id.folder(), title, Some(id));
        self.move_entry(&EntryId::Note(id.clone()), &to.0, cx);
        to
    }

    pub fn rename_folder(&mut self, id: &FolderId, name: &str, cx: &mut Context<Self>) -> FolderId {
        let (Some(parent), name) = (id.parent(), name.trim()) else { return id.clone() };
        if name.is_empty() || name == id.name() {
            return id.clone();
        }
        let to = self.library.free_folder_path(&parent, name, Some(id));
        self.move_entry(&EntryId::Folder(id.clone()), &to.0, cx);
        to
    }

    /// Moves a note or folder into `folder`, keeping its name (or the next free one). A folder
    /// never moves into itself. Returns where it is now.
    pub fn move_into(&mut self, entry: &EntryId, folder: &FolderId, cx: &mut Context<Self>) -> EntryId {
        let known = match entry {
            EntryId::Note(id) => self.library.notes.contains_key(id),
            EntryId::Folder(id) => self.library.folders.contains_key(id),
        };
        if !known || !self.library.has_folder(folder) {
            return entry.clone();
        }
        match entry {
            EntryId::Note(id) => {
                if id.folder() == *folder {
                    return entry.clone();
                }
                let stem = id.stem().to_owned();
                let to = self.library.free_note_path(folder, &stem, None);
                self.move_entry(entry, &to.0, cx);
                EntryId::Note(to)
            }
            EntryId::Folder(id) => {
                if id.is_root() || id.contains(&folder.0) || id.parent().as_ref() == Some(folder) {
                    return entry.clone();
                }
                let to = self.library.free_folder_path(folder, id.name(), None);
                self.move_entry(entry, &to.0, cx);
                EntryId::Folder(to)
            }
        }
    }

    fn move_entry(&mut self, entry: &EntryId, to: &str, cx: &mut Context<Self>) {
        let from = entry.path().to_owned();
        if from == to {
            return;
        }
        self.lib().rename(&from, to);
        let moved: Vec<NoteId> =
            self.edits.keys().filter(|id| super::model::rebase(&id.0, &from, to).is_some()).cloned().collect();
        for old in moved {
            if let (Some(edit), Some(path)) = (self.edits.remove(&old), super::model::rebase(&old.0, &from, to)) {
                self.edits.insert(NoteId(path), edit);
            }
        }
        self.send(Op::Rename { from, to: to.to_owned() });
        cx.notify();
    }

    /// Moves a note or folder to the trash. Returns its name there, for an undo.
    pub fn trash(&mut self, entry: &EntryId, cx: &mut Context<Self>) -> Option<String> {
        let path = entry.path().to_owned();
        let exists = match entry {
            EntryId::Note(id) => self.library.notes.contains_key(id),
            EntryId::Folder(id) => !id.is_root() && self.library.folders.contains_key(id),
        };
        if !exists {
            return None;
        }
        let file_name = path.rsplit('/').next().unwrap_or(&path).to_owned();
        let name = self.library.free_trash_name(&file_name);
        self.lib().trash(entry, name.clone(), Timestamp::now());
        self.edits.retain(|id, _| super::model::rebase(&id.0, &path, "").is_none());
        self.send(Op::Trash { path, name: name.clone() });
        cx.notify();
        Some(name)
    }

    /// Puts a trashed item back. Returns where it went.
    pub fn restore(&mut self, name: &str, cx: &mut Context<Self>) -> Option<EntryId> {
        let item: TrashItem = self.library.trash.iter().find(|t| t.name == name)?.clone();
        let target = self.library.restore_target(&item);
        let lib = self.lib();
        lib.trash.retain(|t| t.name != name);
        let now = Timestamp::now();
        match (&target, &item.kind) {
            (EntryId::Note(id), super::model::TrashKind::Note { text }) => {
                lib.insert_note(Note::new(id.clone(), text.clone(), now, now));
            }
            (EntryId::Folder(id), _) => lib.insert_folder(Folder { id: id.clone(), created: now, modified: now }),
            _ => {}
        }
        self.send(Op::Restore { name: name.to_owned(), to: target.path().to_owned() });
        cx.notify();
        Some(target)
    }

    pub fn empty_trash(&mut self, cx: &mut Context<Self>) {
        self.lib().trash.clear();
        self.send(Op::EmptyTrash);
        cx.notify();
    }

    /// Blocks until the worker has done everything asked of it and applies what it reported.
    /// Tier 1 tests have no host to pump the worker's events, so they settle instead.
    pub fn settle(&mut self, cx: &mut Context<Self>) {
        self.ping += 1;
        let ping = self.ping;
        self.send(Op::Ping(ping));
        while let Ok(event) = self.events.recv_timeout(std::time::Duration::from_secs(10)) {
            let done = matches!(event, Event::Pong(n) if n == ping);
            self.handle(event, cx);
            if done {
                break;
            }
        }
    }
}
