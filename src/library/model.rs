//! The library as the app sees it: folders and notes keyed by their path relative to the root,
//! and what sits in the trash. Pure data. `disk` builds it from a scan; `Store` keeps it current
//! between scans with the same edits it sends to disk.

use std::collections::BTreeMap;
use std::sync::Arc;

use jiff::Timestamp;

use super::properties;
use super::view::FolderView;

/// A folder by its path under the root, `/`-separated. The root itself (Home) is `""`.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FolderId(pub String);

/// A note by its path under the root, `/`-separated, ending in `.md`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NoteId(pub String);

/// A folder or a note: what renames, moves and trashing act on.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum EntryId {
    Folder(FolderId),
    Note(NoteId),
}

pub const NOTE_EXT: &str = "md";

impl FolderId {
    pub fn root() -> Self {
        Self(String::new())
    }

    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    /// The folder this one sits in; the root has none.
    pub fn parent(&self) -> Option<FolderId> {
        (!self.is_root()).then(|| FolderId(parent_of(&self.0).to_owned()))
    }

    pub fn name(&self) -> &str {
        name_of(&self.0)
    }

    /// `name` inside this folder.
    pub fn join(&self, name: &str) -> String {
        if self.is_root() { name.to_owned() } else { format!("{}/{name}", self.0) }
    }

    /// Whether `path` is this folder or lies anywhere under it.
    pub fn contains(&self, path: &str) -> bool {
        self.is_root() || path == self.0 || path.strip_prefix(&self.0).is_some_and(|rest| rest.starts_with('/'))
    }

    /// The folders from the root down to this one, root excluded.
    pub fn ancestry(&self) -> Vec<FolderId> {
        let mut out = Vec::new();
        let mut at = Some(self.clone());
        while let Some(folder) = at.filter(|f| !f.is_root()) {
            at = folder.parent();
            out.push(folder);
        }
        out.reverse();
        out
    }
}

impl NoteId {
    pub fn folder(&self) -> FolderId {
        FolderId(parent_of(&self.0).to_owned())
    }

    pub fn file_name(&self) -> &str {
        name_of(&self.0)
    }

    pub fn stem(&self) -> &str {
        let name = self.file_name();
        name.strip_suffix(".md").unwrap_or(name)
    }
}

impl EntryId {
    pub fn path(&self) -> &str {
        match self {
            EntryId::Folder(f) => &f.0,
            EntryId::Note(n) => &n.0,
        }
    }
}

fn parent_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}

fn name_of(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, name)| name)
}

/// `path` with its leading `from` swapped for `to`, when it lies under `from`.
pub fn rebase(path: &str, from: &str, to: &str) -> Option<String> {
    if path == from {
        return Some(to.to_owned());
    }
    path.strip_prefix(from).filter(|rest| rest.starts_with('/')).map(|rest| format!("{to}{rest}"))
}

#[derive(Clone, Debug, PartialEq)]
pub struct Folder {
    pub id: FolderId,
    pub created: Timestamp,
    pub modified: Timestamp,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub id: NoteId,
    pub text: Arc<str>,
    pub title: String,
    pub created: Timestamp,
    pub modified: Timestamp,
}

impl Note {
    pub fn new(id: NoteId, text: Arc<str>, created: Timestamp, modified: Timestamp) -> Self {
        let title = title_of(&text, id.stem());
        Self { id, text, title, created, modified }
    }

    /// Replaces the text; the title follows it.
    pub fn set_text(&mut self, text: Arc<str>, modified: Timestamp) {
        self.title = title_of(&text, self.id.stem());
        self.text = text;
        self.modified = modified;
    }

    fn rekey(&mut self, id: NoteId) {
        self.title = title_of(&self.text, id.stem());
        self.id = id;
    }
}

/// One thing in the trash: a note or a whole folder, under a name of its own in `.trash`.
#[derive(Clone, Debug, PartialEq)]
pub struct TrashItem {
    /// Its file or directory name inside `.trash`.
    pub name: String,
    /// Where it was, relative to the root.
    pub origin: String,
    pub title: String,
    pub kind: TrashKind,
    pub trashed: Timestamp,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TrashKind {
    Note { text: Arc<str> },
    Folder { items: usize },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Library {
    pub folders: BTreeMap<FolderId, Folder>,
    pub notes: BTreeMap<NoteId, Note>,
    pub trash: Vec<TrashItem>,
    /// Each folder's `.folder.json`, by folder; a folder without one shows cards.
    pub views: BTreeMap<FolderId, FolderView>,
}

/// How a page orders its cards. Folders always come first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sort {
    #[default]
    Modified,
    Created,
    Title,
}

impl Library {
    pub fn note(&self, id: &NoteId) -> Option<&Note> {
        self.notes.get(id)
    }

    pub fn view(&self, folder: &FolderId) -> Option<&FolderView> {
        self.views.get(folder)
    }

    pub fn is_table(&self, folder: &FolderId) -> bool {
        self.view(folder).is_some_and(FolderView::is_table)
    }

    pub fn has_folder(&self, id: &FolderId) -> bool {
        id.is_root() || self.folders.contains_key(id)
    }

    /// The folders directly in `folder`, sorted.
    pub fn subfolders(&self, folder: &FolderId, sort: Sort) -> Vec<&Folder> {
        let mut out: Vec<&Folder> = self.folders.values().filter(|f| f.id.parent().as_ref() == Some(folder)).collect();
        match sort {
            Sort::Modified => out.sort_by_key(|a| std::cmp::Reverse(a.modified)),
            Sort::Created => out.sort_by_key(|a| std::cmp::Reverse(a.created)),
            Sort::Title => out.sort_by_cached_key(|f| f.id.name().to_lowercase()),
        }
        out
    }

    /// The notes directly in `folder`, sorted.
    pub fn notes_in(&self, folder: &FolderId, sort: Sort) -> Vec<&Note> {
        let mut out: Vec<&Note> = self.notes.values().filter(|n| n.id.folder() == *folder).collect();
        match sort {
            Sort::Modified => out.sort_by(|a, b| b.modified.cmp(&a.modified).then(a.id.cmp(&b.id))),
            Sort::Created => out.sort_by(|a, b| b.created.cmp(&a.created).then(a.id.cmp(&b.id))),
            Sort::Title => out.sort_by_cached_key(|n| (n.title.to_lowercase(), n.id.clone())),
        }
        out
    }

    /// Folders and notes directly in `folder`.
    pub fn count(&self, folder: &FolderId) -> usize {
        self.folders.keys().filter(|f| f.parent().as_ref() == Some(folder)).count()
            + self.notes.keys().filter(|n| n.folder() == *folder).count()
    }

    /// Whether a file or folder named `name` already sits in `folder` (case-insensitively, as
    /// the default macOS and Windows file systems compare).
    pub fn name_taken(&self, folder: &FolderId, name: &str) -> bool {
        let wanted = folder.join(name).to_lowercase();
        self.folders.keys().any(|f| f.0.to_lowercase() == wanted)
            || self.notes.keys().any(|n| n.0.to_lowercase() == wanted)
    }

    /// A free note path in `folder` for `title`: "Title.md", then "Title 2.md" and so on.
    pub fn free_note_path(&self, folder: &FolderId, title: &str, except: Option<&NoteId>) -> NoteId {
        let stem = file_stem_for(title);
        let mut n = 1;
        loop {
            let name = if n == 1 { format!("{stem}.{NOTE_EXT}") } else { format!("{stem} {n}.{NOTE_EXT}") };
            let id = NoteId(folder.join(&name));
            if except == Some(&id) || !self.name_taken(folder, &name) {
                return id;
            }
            n += 1;
        }
    }

    /// A free folder path in `folder` for `name`.
    pub fn free_folder_path(&self, folder: &FolderId, name: &str, except: Option<&FolderId>) -> FolderId {
        let base = file_stem_for(name);
        let mut n = 1;
        loop {
            let name = if n == 1 { base.clone() } else { format!("{base} {n}") };
            let id = FolderId(folder.join(&name));
            if except == Some(&id) || !self.name_taken(folder, &name) {
                return id;
            }
            n += 1;
        }
    }

    /// A free name in `.trash` for something named `name`.
    pub fn free_trash_name(&self, name: &str) -> String {
        let (stem, ext) = match name.rsplit_once('.') {
            Some((stem, ext)) if !stem.is_empty() => (stem, Some(ext)),
            _ => (name, None),
        };
        let mut n = 1;
        loop {
            let candidate = match (n, ext) {
                (1, _) => name.to_owned(),
                (_, Some(ext)) => format!("{stem} {n}.{ext}"),
                (_, None) => format!("{stem} {n}"),
            };
            if !self.trash.iter().any(|t| t.name.eq_ignore_ascii_case(&candidate)) {
                return candidate;
            }
            n += 1;
        }
    }

    pub fn insert_note(&mut self, note: Note) {
        self.notes.insert(note.id.clone(), note);
    }

    pub fn insert_folder(&mut self, folder: Folder) {
        self.folders.insert(folder.id.clone(), folder);
    }

    /// Moves a note, or a folder with everything under it, from `from` to `to`.
    pub fn rename(&mut self, from: &str, to: &str) {
        let folders: Vec<FolderId> =
            self.folders.keys().filter(|f| rebase(&f.0, from, to).is_some()).cloned().collect();
        for old in folders {
            if let (Some(mut folder), Some(path)) = (self.folders.remove(&old), rebase(&old.0, from, to)) {
                folder.id = FolderId(path);
                self.folders.insert(folder.id.clone(), folder);
            }
        }
        let views: Vec<FolderId> =
            self.views.keys().filter(|f| !f.is_root() && rebase(&f.0, from, to).is_some()).cloned().collect();
        for old in views {
            if let (Some(view), Some(path)) = (self.views.remove(&old), rebase(&old.0, from, to)) {
                self.views.insert(FolderId(path), view);
            }
        }
        let notes: Vec<NoteId> = self.notes.keys().filter(|n| rebase(&n.0, from, to).is_some()).cloned().collect();
        for old in notes {
            if let (Some(mut note), Some(path)) = (self.notes.remove(&old), rebase(&old.0, from, to)) {
                note.rekey(NoteId(path));
                self.notes.insert(note.id.clone(), note);
            }
        }
    }

    /// Takes `entry` (and, for a folder, everything under it) out of the library and into the
    /// trash as `name`.
    pub fn trash(&mut self, entry: &EntryId, name: String, now: Timestamp) {
        let item = match entry {
            EntryId::Note(id) => {
                let Some(note) = self.notes.remove(id) else { return };
                TrashItem {
                    name,
                    origin: id.0.clone(),
                    title: note.title,
                    kind: TrashKind::Note { text: note.text },
                    trashed: now,
                }
            }
            EntryId::Folder(id) => {
                if id.is_root() || self.folders.remove(id).is_none() {
                    return;
                }
                let before = self.folders.len() + self.notes.len();
                self.folders.retain(|f, _| !id.contains(&f.0));
                self.notes.retain(|n, _| !id.contains(&n.0));
                self.views.retain(|f, _| f.is_root() || !id.contains(&f.0));
                let items = before - self.folders.len() - self.notes.len();
                TrashItem {
                    name,
                    origin: id.0.clone(),
                    title: id.name().to_owned(),
                    kind: TrashKind::Folder { items },
                    trashed: now,
                }
            }
        };
        self.trash.push(item);
    }

    /// Where a trashed item goes back to: its old place when its folder still exists, else the
    /// root, under a free name either way.
    pub fn restore_target(&self, item: &TrashItem) -> EntryId {
        let origin_folder = FolderId(parent_of(&item.origin).to_owned());
        let folder = if self.has_folder(&origin_folder) { origin_folder } else { FolderId::root() };
        let name = name_of(&item.origin);
        match item.kind {
            TrashKind::Note { .. } => {
                let stem = name.strip_suffix(".md").unwrap_or(name);
                let id = NoteId(folder.join(name));
                if self.name_taken(&folder, name) {
                    EntryId::Note(self.free_note_path(&folder, stem, None))
                } else {
                    EntryId::Note(id)
                }
            }
            TrashKind::Folder { .. } => EntryId::Folder(self.free_folder_path(&folder, name, None)),
        }
    }
}

/// The title of a note: its first `# ` heading after the properties, else its file name.
pub fn title_of(text: &str, stem: &str) -> String {
    properties::body(text)
        .lines()
        .find_map(|line| line.strip_prefix("# ").map(str::trim).filter(|t| !t.is_empty()))
        .unwrap_or(stem)
        .to_owned()
}

/// A file name for a title: path separators and characters the file systems refuse become
/// spaces, leading dots go (a dot file would be hidden), and an empty result is "Untitled".
pub fn file_stem_for(title: &str) -> String {
    let cleaned: String =
        title
            .chars()
            .map(|c| {
                if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() {
                    ' '
                } else {
                    c
                }
            })
            .collect();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let cleaned = cleaned.trim_start_matches('.').trim();
    let cleaned: String = cleaned.chars().take(120).collect();
    if cleaned.is_empty() { "Untitled".to_owned() } else { cleaned }
}

/// A note split for the editor: the properties block (edited apart, never shown as text), the
/// leading `# ` heading when one comes next (it is shown as the page title), and the rest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Split<'a> {
    /// The properties block with the blank lines after it, as written; empty without one.
    pub front: &'a str,
    pub heading: Option<&'a str>,
    pub body: &'a str,
}

pub fn split(text: &str) -> Split<'_> {
    let block = properties::block_len(text);
    let after = &text[block..];
    let trimmed = after.trim_start_matches(['\n', '\r']);
    let (first, rest) = trimmed.split_once('\n').unwrap_or((trimmed, ""));
    match first.trim_end_matches('\r').strip_prefix("# ") {
        Some(heading) => {
            let front = if block == 0 { "" } else { &text[..text.len() - trimmed.len()] };
            Split { front, heading: Some(heading.trim()), body: rest.trim_start_matches(['\n', '\r']) }
        }
        None => Split { front: &text[..block], heading: None, body: after },
    }
}

/// The text with its leading heading set to `title`, adding one when it has none.
pub fn with_heading(text: &str, title: &str) -> String {
    let parts = split(text);
    compose(parts.front, Some(title), parts.body)
}

/// Joins the properties block, a heading and a body back into a note's text.
pub fn compose(front: &str, heading: Option<&str>, body: &str) -> String {
    let gap = if front.is_empty() || front.ends_with('\n') { "" } else { "\n" };
    match heading {
        Some(h) if body.is_empty() => format!("{front}{gap}# {h}\n"),
        Some(h) => format!("{front}{gap}# {h}\n\n{body}"),
        None => format!("{front}{gap}{body}"),
    }
}

/// Words in a note, Markdown markers and properties aside.
pub fn word_count(text: &str) -> usize {
    properties::body(text).split_whitespace().filter(|w| w.chars().any(char::is_alphanumeric)).count()
}

/// A line of a card's miniature: what kind of block it is and its text without the markers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PreviewLine {
    Heading(String),
    Bullet(String),
    Task { done: bool, text: String },
    Quote(String),
    Code(String),
    Text(String),
}

/// The first `max` blocks of a note's body (its title heading left out), for a card.
pub fn preview(text: &str, max: usize) -> Vec<PreviewLine> {
    let body = split(text).body;
    let mut in_code = false;
    let mut out = Vec::new();
    for line in body.lines() {
        if out.len() >= max {
            break;
        }
        let line = line.trim_end();
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            out.push(PreviewLine::Code(line.to_owned()));
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.chars().all(|c| matches!(c, '-' | '*' | '_')) {
            continue;
        }
        let strip = |s: &str| s.replace("**", "").replace('`', "").trim().to_owned();
        let parsed = if let Some(rest) = trimmed.strip_prefix("- [ ] ").or_else(|| trimmed.strip_prefix("* [ ] ")) {
            PreviewLine::Task { done: false, text: strip(rest) }
        } else if let Some(rest) = trimmed.strip_prefix("- [x] ").or_else(|| trimmed.strip_prefix("* [x] ")) {
            PreviewLine::Task { done: true, text: strip(rest) }
        } else if let Some(rest) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")) {
            PreviewLine::Bullet(strip(rest))
        } else if trimmed.starts_with('#') {
            PreviewLine::Heading(strip(trimmed.trim_start_matches('#')))
        } else if let Some(rest) = trimmed.strip_prefix('>') {
            PreviewLine::Quote(strip(rest))
        } else {
            PreviewLine::Text(strip(trimmed))
        };
        out.push(parsed);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(path: &str, text: &str) -> Note {
        Note::new(NoteId(path.into()), text.into(), Timestamp::UNIX_EPOCH, Timestamp::UNIX_EPOCH)
    }

    #[test]
    fn the_title_is_the_first_heading_or_the_file_name() {
        assert_eq!(title_of("intro\n# Plan\nbody", "file"), "Plan");
        assert_eq!(title_of("## Sub\ntext", "file"), "file");
        assert_eq!(title_of("#   \n", "file"), "file", "an empty heading names nothing");
        assert_eq!(note("a/Weekend.md", "no heading").title, "Weekend");
    }

    #[test]
    fn a_leading_heading_splits_off_and_composes_back() {
        let s = split("\n# Trip\n\nDay one");
        assert_eq!(s, Split { front: "", heading: Some("Trip"), body: "Day one" });
        assert_eq!(split("Just text\n# Later"), Split { front: "", heading: None, body: "Just text\n# Later" });
        assert_eq!(compose("", Some("Trip"), "Day one"), "# Trip\n\nDay one");
        assert_eq!(with_heading("plain body", "New"), "# New\n\nplain body");
        assert_eq!(with_heading("# Old\n\nkept", "New"), "# New\n\nkept");
    }

    #[test]
    fn the_properties_block_is_kept_apart_from_the_title_and_body() {
        let text = "---\nstatus: Doing\n---\n\n# Plan\n\nFirst step";
        let s = split(text);
        assert_eq!(s, Split { front: "---\nstatus: Doing\n---\n\n", heading: Some("Plan"), body: "First step" });
        assert_eq!(compose(s.front, s.heading, s.body), text, "composes back byte for byte");
        assert_eq!(with_heading(text, "Ship"), "---\nstatus: Doing\n---\n\n# Ship\n\nFirst step");
        let bare = "---\ndue: 2026-01-01\n---\nNo heading";
        assert_eq!(split(bare), Split { front: "---\ndue: 2026-01-01\n---\n", heading: None, body: "No heading" });
        assert_eq!(title_of(bare, "file"), "file");
        assert_eq!(title_of("---\n# not: a title\n---\n# Real", "file"), "Real");
        assert_eq!(word_count("---\na: one two\n---\nthree"), 1);
        assert_eq!(preview("---\na: b\n---\n# T\nline", 5), [PreviewLine::Text("line".into())]);
        assert_eq!(compose("---\na: b\n---", None, "x"), "---\na: b\n---\nx");
    }

    #[test]
    fn file_names_drop_what_file_systems_refuse() {
        assert_eq!(file_stem_for("a/b: c?"), "a b c");
        assert_eq!(file_stem_for("..hidden"), "hidden");
        assert_eq!(file_stem_for("   "), "Untitled");
    }

    #[test]
    fn free_paths_count_up_past_taken_names() {
        let mut lib = Library::default();
        lib.insert_note(note("Untitled.md", ""));
        lib.insert_note(note("untitled 2.md", ""));
        assert_eq!(lib.free_note_path(&FolderId::root(), "Untitled", None).0, "Untitled 3.md");
        let own = NoteId("Untitled.md".into());
        assert_eq!(lib.free_note_path(&FolderId::root(), "Untitled", Some(&own)), own, "a note keeps its own name");
    }

    #[test]
    fn renaming_a_folder_moves_everything_under_it() {
        let mut lib = Library::default();
        let t = Timestamp::UNIX_EPOCH;
        lib.insert_folder(Folder { id: FolderId("Work".into()), created: t, modified: t });
        lib.insert_folder(Folder { id: FolderId("Work/Q3".into()), created: t, modified: t });
        lib.insert_note(note("Work/Q3/Plan.md", "# Plan"));
        lib.insert_note(note("Workshop.md", ""));
        lib.rename("Work", "Job");
        assert!(lib.folders.contains_key(&FolderId("Job/Q3".into())));
        assert!(lib.notes.contains_key(&NoteId("Job/Q3/Plan.md".into())));
        assert!(lib.notes.contains_key(&NoteId("Workshop.md".into())), "a sibling sharing the prefix stays");
    }

    #[test]
    fn previews_skip_the_title_and_read_blocks() {
        let lines = preview("# T\n\nIntro **bold**\n- [x] done\n- item\n```\ncode\n```\n> quote", 10);
        assert_eq!(
            lines,
            [
                PreviewLine::Text("Intro bold".into()),
                PreviewLine::Task { done: true, text: "done".into() },
                PreviewLine::Bullet("item".into()),
                PreviewLine::Code("code".into()),
                PreviewLine::Quote("quote".into()),
            ]
        );
        assert_eq!(word_count("# Title\n\n- two words"), 3);
    }
}
