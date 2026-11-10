//! The command palette's rows. `⌘K` lists notes, folders and commands; `⌘J` searches notes by
//! title and by what they say. The palette shows exactly these rows; activating one comes back
//! as a [`Command`].

use nd::prelude::*;

use crate::actions::{self, shortcut};
use crate::library::{EntryId, Library, Note};
use crate::nav::{entry_key, parse_entry};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Commands,
    Search,
}

/// What a palette row does.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Open(EntryId),
    NewNote,
    NewFolder,
    Home,
    Search,
    Trash,
    MoveToTrash,
    ToggleSidebar,
    Settings,
    ViewAsCards,
    ViewAsTable,
}

const COMMANDS: [(&str, &str, Icon); 10] = [
    ("cmd:new-note", "New Note", Icon::SquarePen),
    ("cmd:new-folder", "New Folder", Icon::Folder),
    ("cmd:home", "Go Home", Icon::House),
    ("cmd:search", "Search Notes", Icon::Search),
    ("cmd:trash", "Open Trash", Icon::Trash),
    ("cmd:move-to-trash", "Move Note to Trash", Icon::Trash),
    ("cmd:sidebar", "Toggle Sidebar", Icon::PanelLeft),
    ("cmd:settings", "Settings", Icon::Settings),
    ("cmd:view-cards", "View as Cards", Icon::Notebook),
    ("cmd:view-table", "View as Table", Icon::ListPlus),
];

impl Command {
    pub fn parse(id: &str) -> Option<Command> {
        if let Some(entry) = id.strip_prefix("open:") {
            return parse_entry(entry).map(Command::Open);
        }
        Some(match id {
            "cmd:new-note" => Command::NewNote,
            "cmd:new-folder" => Command::NewFolder,
            "cmd:home" => Command::Home,
            "cmd:search" => Command::Search,
            "cmd:trash" => Command::Trash,
            "cmd:move-to-trash" => Command::MoveToTrash,
            "cmd:sidebar" => Command::ToggleSidebar,
            "cmd:settings" => Command::Settings,
            "cmd:view-cards" => Command::ViewAsCards,
            "cmd:view-table" => Command::ViewAsTable,
            _ => return None,
        })
    }

    fn hint(&self) -> Option<&'static str> {
        match self {
            Command::NewNote => shortcut(&actions::NewNote),
            Command::NewFolder => shortcut(&actions::NewFolder),
            Command::Home => shortcut(&actions::GoHome),
            Command::Search => shortcut(&actions::Search),
            Command::MoveToTrash => shortcut(&actions::MoveToTrash),
            Command::ToggleSidebar => shortcut(&nd::actions::ToggleSidebar),
            Command::Settings => shortcut(&actions::OpenSettings),
            Command::Open(_) | Command::Trash | Command::ViewAsCards | Command::ViewAsTable => None,
        }
    }
}

/// How many notes a query lists.
const NOTE_ROWS: usize = 20;

/// The rows for `query`. `note_open` offers the commands that act on the open note.
pub fn items(library: &Library, mode: Mode, query: &str, note_open: bool) -> Vec<PaletteItem> {
    let lower = query.to_lowercase();
    let notes = library.notes.values();
    let mut ranked: Vec<&Note> = fuzzy::rank(query, notes, |n: &&Note| n.title.as_str()).take(NOTE_ROWS).collect();
    if mode == Mode::Search && !lower.is_empty() {
        let extra: Vec<&Note> = library
            .notes
            .values()
            .filter(|n| !ranked.iter().any(|r| r.id == n.id) && n.text.to_lowercase().contains(&lower))
            .take(NOTE_ROWS.saturating_sub(ranked.len()))
            .collect();
        ranked.extend(extra);
    }
    let mut rows: Vec<PaletteItem> = ranked
        .into_iter()
        .map(|note| {
            let completes = !lower.is_empty() && note.title.to_lowercase().starts_with(&lower);
            let subtitle = match mode {
                Mode::Search if !lower.is_empty() => snippet(&note.text, &lower).unwrap_or_else(|| place(&note.id.0)),
                _ => place(&note.id.0),
            };
            palette_item(format!("open:{}", entry_key(&EntryId::Note(note.id.clone()))), note.title.clone())
                .subtitle(subtitle)
                .icon(Icon::FileText)
                .when(completes, |item| item.completion(note.title.clone()))
        })
        .collect();
    if mode == Mode::Search {
        return rows;
    }
    let folders = library.folders.values();
    rows.extend(fuzzy::rank(query, folders, |f| f.id.name()).take(8).map(|folder| {
        palette_item(format!("open:{}", entry_key(&EntryId::Folder(folder.id.clone()))), folder.id.name().to_owned())
            .subtitle(place(&folder.id.0))
            .icon(Icon::Folder)
    }));
    let commands = COMMANDS.into_iter().filter(|(id, ..)| note_open || *id != "cmd:move-to-trash");
    rows.extend(fuzzy::rank(query, commands, |c| c.1).map(|(id, title, icon)| {
        let hint = Command::parse(id).and_then(|c| c.hint());
        palette_item(id, title).icon(icon).when_some(hint, |item, keys| item.hint(keys))
    }));
    rows
}

/// "In Projects / Launch", or "In Home".
fn place(path: &str) -> String {
    match path.rsplit_once('/') {
        Some((folder, _)) => format!("In {}", folder.replace('/', " / ")),
        None => "In Home".to_owned(),
    }
}

/// The words around the first match of `lower` in `text`, on one line.
fn snippet(text: &str, lower: &str) -> Option<String> {
    let haystack = text.to_lowercase();
    let at = haystack.find(lower)?;
    // Lowercasing can shift byte offsets; work in characters from here.
    let start_chars = haystack[..at].chars().count().saturating_sub(24);
    let snippet: String = text.chars().skip(start_chars).take(90).collect();
    let line = snippet.split_whitespace().collect::<Vec<_>>().join(" ");
    Some(if start_chars > 0 { format!("…{line}") } else { line })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_parse_back() {
        for (id, ..) in COMMANDS {
            assert!(Command::parse(id).is_some(), "{id}");
        }
        assert_eq!(Command::parse("open:home"), Some(Command::Open(EntryId::Folder(Default::default()))));
        assert_eq!(Command::parse("nope"), None);
    }

    #[test]
    fn snippets_start_near_the_match() {
        assert_eq!(snippet("short text here", "text").as_deref(), Some("short text here"));
        let long = format!("{} needle tail", "word ".repeat(20));
        assert!(snippet(&long, "needle").is_some_and(|s| s.starts_with('…') && s.contains("needle")));
    }
}
