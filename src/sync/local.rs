//! The library as sync sees it: notes, folders and `.folder.json` files by their NFC path, and
//! the checked writes that apply remote changes. Every path a server sends goes through
//! [`checked`] before anything touches the disk; deletes go to the trash through the library's
//! own `disk` helpers.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use unicode_normalization::UnicodeNormalization;

use super::object::{Kind, Payload, content_hash};
use crate::library::disk::{self, Root};

pub const FOLDER_FILE: &str = ".folder.json";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub kind: Kind,
    pub text: Option<String>,
    pub hash: String,
    pub created: i64,
    pub modified: i64,
}

impl Entry {
    pub fn payload(&self, path: &str) -> Payload {
        Payload {
            v: 1,
            kind: self.kind,
            path: path.to_owned(),
            text: self.text.clone(),
            created: self.created,
            modified: self.modified,
        }
    }
}

pub type Scan = BTreeMap<String, Entry>;

fn millis(t: io::Result<SystemTime>) -> Option<i64> {
    t.ok()?.duration_since(UNIX_EPOCH).ok().and_then(|d| i64::try_from(d.as_millis()).ok())
}

fn kind_of(name: &str) -> Option<Kind> {
    if name == FOLDER_FILE {
        Some(Kind::File)
    } else if name.starts_with('.') {
        None
    } else if name.ends_with(".md") {
        Some(Kind::Note)
    } else {
        None
    }
}

fn read_entry(path: &Path, kind: Kind) -> Option<Entry> {
    let meta = fs::metadata(path).ok()?;
    let modified = millis(meta.modified()).unwrap_or(0);
    let created = millis(meta.created()).unwrap_or(modified);
    let text = match kind {
        Kind::Folder => {
            if !meta.is_dir() {
                return None;
            }
            None
        }
        _ => {
            if !meta.is_file() {
                return None;
            }
            Some(String::from_utf8_lossy(&fs::read(path).ok()?).into_owned())
        }
    };
    Some(Entry { kind, hash: content_hash(text.as_deref()), text, created, modified })
}

/// Every note, folder and `.folder.json` under `root`. Dot directories (the trash) are skipped.
pub fn scan(root: &Root) -> io::Result<Scan> {
    let mut out = Scan::new();
    let mut stack = vec![(root.path().to_owned(), String::new())];
    while let Some((dir, rel)) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if rel.is_empty() => return Err(e),
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let name: String = entry.file_name().to_string_lossy().nfc().collect();
            let path = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
            let Ok(file_type) = entry.file_type() else { continue };
            if file_type.is_dir() {
                if name.starts_with('.') {
                    continue;
                }
                if let Some(e) = read_entry(&entry.path(), Kind::Folder) {
                    out.insert(path.clone(), e);
                }
                stack.push((entry.path(), path));
            } else if let Some(kind) = kind_of(&name)
                && let Some(e) = read_entry(&entry.path(), kind)
            {
                out.insert(path, e);
            }
        }
    }
    Ok(out)
}

/// One entry, read now.
pub fn read(root: &Root, path: &str, kind: Kind) -> Option<Entry> {
    read_entry(&checked(root, path, kind).ok()?, kind)
}

/// `path` under `root` for an object of `kind`, or an error for anything that is not a library
/// path of that kind: absolute, `..`, a dot component, a backslash, a note without `.md`, or a
/// `.folder.json` anywhere but last.
pub fn checked(root: &Root, path: &str, kind: Kind) -> io::Result<PathBuf> {
    let bad = || io::Error::new(io::ErrorKind::InvalidInput, format!("{path:?} is not a library path"));
    if path.contains(['\\', '\0']) || path.nfc().collect::<String>() != path {
        return Err(bad());
    }
    match kind {
        Kind::Note if !path.ends_with(".md") => Err(bad()),
        Kind::File => match path.rsplit_once('/') {
            _ if path == FOLDER_FILE => Ok(root.path().join(FOLDER_FILE)),
            Some((folder, FOLDER_FILE)) => Ok(root.resolve(folder)?.join(FOLDER_FILE)),
            _ => Err(bad()),
        },
        _ => root.resolve(path),
    }
}

fn parent_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(p, _)| p)
}

fn ensure_parent(root: &Root, path: &str) -> io::Result<()> {
    let parent = parent_of(path);
    if parent.is_empty() {
        return fs::create_dir_all(root.path());
    }
    fs::create_dir_all(root.resolve(parent)?)
}

/// Writes a payload's file or folder at `path`, dated as the payload says.
pub fn write(root: &Root, path: &str, payload: &Payload) -> io::Result<()> {
    let target = checked(root, path, payload.kind)?;
    ensure_parent(root, path)?;
    match payload.kind {
        Kind::Folder => fs::create_dir_all(&target)?,
        _ => disk::write_atomic(&target, payload.text.as_deref().unwrap_or_default().as_bytes())?,
    }
    let when = UNIX_EPOCH + Duration::from_millis(u64::try_from(payload.modified).unwrap_or(0));
    if payload.kind != Kind::Folder {
        let _ = disk::set_modified(&target, when);
    }
    Ok(())
}

/// Moves a note or folder; fails when `to` is taken.
pub fn rename(root: &Root, from: &str, to: &str, kind: Kind) -> io::Result<()> {
    let (src, dst) = (checked(root, from, kind)?, checked(root, to, kind)?);
    ensure_parent(root, to)?;
    if kind != Kind::File {
        return disk::rename(root, from, to);
    }
    // `disk` refuses dot names, so a `.folder.json` moves here, under the same rule.
    if dst.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("{to} exists")));
    }
    fs::rename(src, dst)
}

/// Whether a folder holds nothing but dot files.
pub fn is_empty_folder(root: &Root, path: &str) -> bool {
    let Ok(dir) = checked(root, path, Kind::Folder) else { return false };
    fs::read_dir(dir).is_ok_and(|mut entries| {
        entries
            .all(|e| e.is_ok_and(|e| e.file_name().to_string_lossy().starts_with('.') && e.file_name() != FOLDER_FILE))
    })
}

/// Sends a note or folder to the trash, as the app's Move to Trash does. A `.folder.json` has
/// no place in the trash and is removed.
pub fn trash(root: &Root, path: &str, kind: Kind) -> io::Result<()> {
    let target = checked(root, path, kind)?;
    if kind == Kind::File {
        return fs::remove_file(target);
    }
    let name = path.rsplit('/').next().unwrap_or(path);
    let trash = root.path().join(disk::TRASH_DIR);
    let free = free_name(name, kind, |candidate| trash.join(candidate).exists())?;
    disk::trash(root, path, &free)
}

/// `name`, else `name 2`, `name 3` and on (before `.md` for a note) until `taken` says no. A
/// `.folder.json` has one name only.
pub fn free_name(name: &str, kind: Kind, taken: impl Fn(&str) -> bool) -> io::Result<String> {
    let (stem, ext) = match (kind, name.strip_suffix(".md")) {
        (Kind::Note, Some(stem)) => (stem, ".md"),
        _ => (name, ""),
    };
    let tries = if kind == Kind::File { 1 } else { 1000 };
    (1..=tries)
        .map(|n| if n == 1 { name.to_owned() } else { format!("{stem} {n}{ext}") })
        .find(|candidate| !taken(candidate))
        .ok_or_else(|| io::Error::new(io::ErrorKind::AlreadyExists, format!("no free name for {name}")))
}

/// A path beside `path` that is free on disk: `path` itself, or its name with a number.
pub fn free_path(root: &Root, path: &str, kind: Kind) -> io::Result<String> {
    checked(root, path, kind)?;
    let (parent, name) = path.rsplit_once('/').map_or(("", path), |(p, n)| (p, n));
    let join = |n: &str| if parent.is_empty() { n.to_owned() } else { format!("{parent}/{n}") };
    let free = free_name(name, kind, |candidate| checked(root, &join(candidate), kind).is_ok_and(|p| p.exists()))?;
    Ok(join(&free))
}

/// `<name> (conflict YYYY-MM-DD HH.mm).md` beside `path`, numbered if even that is taken.
pub fn conflict_path(root: &Root, path: &str, now: jiff::Zoned) -> io::Result<String> {
    let stem = path.strip_suffix(".md").unwrap_or(path);
    free_path(root, &format!("{stem} (conflict {}).md", now.strftime("%Y-%m-%d %H.%M")), Kind::Note)
}
