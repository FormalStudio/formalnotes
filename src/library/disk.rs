//! The library on disk: a root folder of nested folders and `.md` files, with the trash in
//! `.trash`. Every path from the rest of the app arrives relative to the root and is checked
//! here ([`Root::resolve`]) before anything touches the file system. Blocking; only the worker
//! thread calls it.

use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use jiff::Timestamp;

use super::model::{Folder, FolderId, Library, NOTE_EXT, Note, NoteId, TrashItem, TrashKind, title_of};
use super::view::{FolderView, VIEW_FILE};

pub const TRASH_DIR: &str = ".trash";
/// Where trashed items came from, by their name in the trash.
const ORIGINS: &str = ".origins.json";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Root(PathBuf);

impl Root {
    pub fn new(path: PathBuf) -> Self {
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// `rel` under the root. Refuses empty paths, absolute paths, `..`, and any component that
    /// starts with a dot, so nothing outside the library (or inside the trash) is reachable.
    pub fn resolve(&self, rel: &str) -> io::Result<PathBuf> {
        let path = Path::new(rel);
        let mut parts = 0;
        for c in path.components() {
            match c {
                Component::Normal(name) if !name.to_string_lossy().starts_with('.') => parts += 1,
                _ => return Err(invalid(format!("{rel:?} is not a library path"))),
            }
        }
        if parts == 0 {
            return Err(invalid("an empty library path".into()));
        }
        Ok(self.0.join(path))
    }

    fn trash_dir(&self) -> PathBuf {
        self.0.join(TRASH_DIR)
    }

    /// `name` inside `.trash`: a single plain component.
    fn trash_entry(&self, name: &str) -> io::Result<PathBuf> {
        let mut parts = Path::new(name).components();
        match (parts.next(), parts.next()) {
            (Some(Component::Normal(n)), None) if !n.to_string_lossy().starts_with('.') => Ok(self.trash_dir().join(n)),
            _ => Err(invalid(format!("{name:?} is not a trash name"))),
        }
    }
}

fn invalid(msg: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, msg)
}

/// What the last scan read, so a rescan reads only files whose size or time changed.
#[derive(Default)]
pub struct ScanCache {
    notes: HashMap<PathBuf, (SystemTime, u64, Arc<str>)>,
}

fn timestamp(t: io::Result<SystemTime>) -> Timestamp {
    t.ok().and_then(|t| Timestamp::try_from(t).ok()).unwrap_or(Timestamp::UNIX_EPOCH)
}

fn read_text(path: &Path, meta: &fs::Metadata, cache: &mut ScanCache) -> io::Result<Arc<str>> {
    let modified = meta.modified()?;
    if let Some((m, len, text)) = cache.notes.get(path)
        && *m == modified
        && *len == meta.len()
    {
        return Ok(text.clone());
    }
    let bytes = fs::read(path)?;
    let text: Arc<str> = String::from_utf8_lossy(&bytes).into();
    cache.notes.insert(path.to_owned(), (modified, meta.len(), text.clone()));
    Ok(text)
}

/// Reads the whole library. Unreadable entries are skipped, not fatal.
pub fn scan(root: &Root, cache: &mut ScanCache) -> io::Result<Library> {
    fs::create_dir_all(root.path())?;
    let mut lib = Library::default();
    let mut stack = vec![(root.path().to_owned(), FolderId::root())];
    let mut seen = Vec::new();
    while let Some((dir, folder)) = stack.pop() {
        if let Some(view) = fs::read(dir.join(VIEW_FILE)).ok().and_then(|b| FolderView::parse(&b)) {
            lib.views.insert(folder.clone(), view);
        }
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            let path = entry.path();
            if meta.is_dir() {
                let id = FolderId(folder.join(&name));
                lib.insert_folder(Folder {
                    id: id.clone(),
                    created: timestamp(meta.created()),
                    modified: timestamp(meta.modified()),
                });
                stack.push((path, id));
            } else if path.extension().is_some_and(|e| e == NOTE_EXT)
                && let Ok(text) = read_text(&path, &meta, cache)
            {
                seen.push(path);
                lib.insert_note(Note::new(
                    NoteId(folder.join(&name)),
                    text,
                    timestamp(meta.created()),
                    timestamp(meta.modified()),
                ));
            }
        }
    }
    cache.notes.retain(|p, _| seen.contains(p));
    lib.trash = scan_trash(root);
    Ok(lib)
}

fn read_origins(root: &Root) -> HashMap<String, String> {
    fs::read(root.trash_dir().join(ORIGINS)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn write_origins(root: &Root, origins: &HashMap<String, String>) -> io::Result<()> {
    let json = serde_json::to_vec_pretty(origins).map_err(io::Error::other)?;
    write_atomic(&root.trash_dir().join(ORIGINS), &json)
}

fn scan_trash(root: &Root) -> Vec<TrashItem> {
    let origins = read_origins(root);
    let Ok(entries) = fs::read_dir(root.trash_dir()) else { return Vec::new() };
    let mut items: Vec<TrashItem> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                return None;
            }
            let meta = entry.metadata().ok()?;
            let origin = origins.get(&name).cloned().unwrap_or_else(|| name.clone());
            let (title, kind) = if meta.is_dir() {
                (name.clone(), TrashKind::Folder { items: count_entries(&entry.path()) })
            } else {
                let text: Arc<str> = String::from_utf8_lossy(&fs::read(entry.path()).ok()?).into();
                let stem = name.strip_suffix(".md").unwrap_or(&name);
                (title_of(&text, stem), TrashKind::Note { text })
            };
            // Moving a file into the trash keeps its times; the change time is when it moved.
            let trashed = timestamp(meta.modified());
            Some(TrashItem { name, origin, title, kind, trashed })
        })
        .collect();
    items.sort_by(|a, b| b.trashed.cmp(&a.trashed).then(a.name.cmp(&b.name)));
    items
}

fn count_entries(dir: &Path) -> usize {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
                .map(|e| if e.path().is_dir() { 1 + count_entries(&e.path()) } else { 1 })
                .sum()
        })
        .unwrap_or(0)
}

static TMP: AtomicU64 = AtomicU64::new(0);

/// Writes `bytes` to `path` all at once: a fresh temp file beside it, synced, then renamed over
/// it, so a crash leaves the old file or the new one, never half of either.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path.parent().ok_or_else(|| invalid(format!("{} has no folder", path.display())))?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!(".{name}.{}-{}.tmp", std::process::id(), TMP.fetch_add(1, Ordering::Relaxed)));
    let result = (|| {
        let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&tmp, path)?;
        // The rename is durable once the folder's entry is.
        if let Ok(dir) = fs::File::open(dir) {
            let _ = dir.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Saves a note's text over its file.
pub fn save(root: &Root, note: &NoteId, text: &str) -> io::Result<()> {
    let path = root.resolve(&note.0)?;
    if !path.exists() {
        return Err(io::Error::new(io::ErrorKind::NotFound, format!("{} is gone", note.0)));
    }
    write_atomic(&path, text.as_bytes())
}

/// Writes a folder's `.folder.json`.
pub fn write_view(root: &Root, folder: &FolderId, bytes: &[u8]) -> io::Result<()> {
    let dir = if folder.is_root() { root.path().to_owned() } else { root.resolve(&folder.0)? };
    if !dir.is_dir() {
        return Err(io::Error::new(io::ErrorKind::NotFound, format!("{} is gone", folder.0)));
    }
    write_atomic(&dir.join(VIEW_FILE), bytes)
}

/// Creates a note file; fails when the name is taken.
pub fn create_note(root: &Root, note: &NoteId, text: &str) -> io::Result<()> {
    let path = root.resolve(&note.0)?;
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()
}

pub fn create_folder(root: &Root, folder: &FolderId) -> io::Result<()> {
    fs::create_dir(root.resolve(&folder.0)?)
}

/// Renames or moves a note or folder; fails when the target exists. A rename that only changes
/// case is allowed on case-insensitive file systems.
pub fn rename(root: &Root, from: &str, to: &str) -> io::Result<()> {
    let (src, dst) = (root.resolve(from)?, root.resolve(to)?);
    if dst.exists() && !from.eq_ignore_ascii_case(to) {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("{to} exists")));
    }
    fs::rename(src, dst)
}

/// Moves `rel` into the trash as `name`, remembering where it came from.
pub fn trash(root: &Root, rel: &str, name: &str) -> io::Result<()> {
    let src = root.resolve(rel)?;
    let dst = root.trash_entry(name)?;
    fs::create_dir_all(root.trash_dir())?;
    if dst.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("{name} is in the trash")));
    }
    fs::rename(src, &dst)?;
    let mut origins = read_origins(root);
    origins.insert(name.to_owned(), rel.to_owned());
    write_origins(root, &origins)
}

/// Moves trashed `name` back to `to`.
pub fn restore(root: &Root, name: &str, to: &str) -> io::Result<()> {
    let src = root.trash_entry(name)?;
    let dst = root.resolve(to)?;
    if dst.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("{to} exists")));
    }
    fs::rename(src, dst)?;
    let mut origins = read_origins(root);
    origins.remove(name);
    write_origins(root, &origins)
}

/// Deletes everything in the trash for good.
pub fn empty_trash(root: &Root) -> io::Result<()> {
    let Ok(entries) = fs::read_dir(root.trash_dir()) else { return Ok(()) };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        // Only paths `trash_entry` builds, which never leave `.trash`.
        let path = root.trash_entry(&name)?;
        if path.is_dir() { fs::remove_dir_all(&path)? } else { fs::remove_file(&path)? }
    }
    write_origins(root, &HashMap::new())
}

/// Sets a file's modified time: the demo library dates its notes.
pub fn set_modified(path: &Path, when: SystemTime) -> io::Result<()> {
    fs::File::options().write(true).open(path)?.set_modified(when)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_outside_the_library_are_refused() {
        let root = Root::new(PathBuf::from("/lib"));
        assert!(root.resolve("a/b.md").is_ok());
        for bad in ["", "../x.md", "/etc/passwd", "a/../../x", ".trash/x.md", "a/.hidden"] {
            assert!(root.resolve(bad).is_err(), "{bad:?} must be refused");
        }
        assert!(root.trash_entry("a/b").is_err());
        assert!(root.trash_entry("..").is_err());
    }

    #[test]
    fn atomic_writes_leave_no_temp_files() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("n.md");
        write_atomic(&path, b"one").expect("first write");
        write_atomic(&path, b"two").expect("second write");
        assert_eq!(fs::read_to_string(&path).expect("read"), "two");
        assert_eq!(fs::read_dir(dir.path()).expect("list").count(), 1);
    }
}
