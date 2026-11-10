//! Where the window is, and the string keys that name entries across the sidebar, the cards,
//! drags, palette rows and actions.

use nd::prelude::SharedString;

use crate::library::{EntryId, FolderId, NoteId};

/// The page under the editor: a folder (Home is the root folder) or the trash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Location {
    Folder(FolderId),
    Trash,
}

impl Location {
    pub fn home() -> Self {
        Location::Folder(FolderId::root())
    }
}

/// The drag kind of notes and folders, shared by the sidebar and the cards.
pub const ENTRY_DRAG: &str = "formalnotes.entry";

pub const HOME_KEY: &str = "home";
pub const TRASH_KEY: &str = "trash";
pub const LIBRARY_SECTION: &str = "library";

pub fn entry_key(entry: &EntryId) -> SharedString {
    match entry {
        EntryId::Note(n) => note_key(n),
        EntryId::Folder(f) => folder_key(f),
    }
}

pub fn note_key(note: &NoteId) -> SharedString {
    format!("note:{}", note.0).into()
}

/// The root folder is Home.
pub fn folder_key(folder: &FolderId) -> SharedString {
    if folder.is_root() { HOME_KEY.into() } else { format!("folder:{}", folder.0).into() }
}

/// The entry a key names. Paths are checked again by the disk layer before any file is touched.
pub fn parse_entry(key: &str) -> Option<EntryId> {
    if key == HOME_KEY || key == LIBRARY_SECTION {
        return Some(EntryId::Folder(FolderId::root()));
    }
    if let Some(path) = key.strip_prefix("note:") {
        return (!path.is_empty()).then(|| EntryId::Note(NoteId(path.to_owned())));
    }
    key.strip_prefix("folder:").filter(|p| !p.is_empty()).map(|p| EntryId::Folder(FolderId(p.to_owned())))
}

/// The id of a card's tile: the frame a presented note grows from.
pub fn card_id(entry: &EntryId) -> SharedString {
    format!("card:{}", entry_key(entry)).into()
}

/// The entry an [`ENTRY_DRAG`] carries: a sidebar row's key, or a card's id.
pub fn dragged_entry(id: &str) -> Option<EntryId> {
    parse_entry(id.strip_prefix("card:").unwrap_or(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip() {
        for entry in [
            EntryId::Note(NoteId("a/b.md".into())),
            EntryId::Folder(FolderId("a".into())),
            EntryId::Folder(FolderId::root()),
        ] {
            assert_eq!(parse_entry(&entry_key(&entry)), Some(entry));
        }
        assert_eq!(parse_entry("note:"), None);
        assert_eq!(parse_entry("trash"), None);
    }
}
