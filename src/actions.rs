//! Commands: one action drives the shortcut, the menu item, the palette row and the context
//! menu item. Actions that act on one entry carry its key ([`crate::nav::entry_key`]).

use nd::prelude::*;
use serde::Deserialize;

use crate::settings::SortOrder;

actions!(
    formalnotes,
    [
        NewNote,
        NewFolder,
        OpenPalette,
        Search,
        GoHome,
        GoBack,
        OpenTrash,
        EmptyTrash,
        MoveToTrash,
        OpenSettings,
        ReportIssue,
    ]
);

macro_rules! entry_action {
    ($($name:ident),+ $(,)?) => {$(
        #[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Action)]
        #[schemars(crate = "nd::schemars")]
        #[action(namespace = formalnotes)]
        pub struct $name(pub String);
    )+};
}

// Each carries an entry key: `note:<path>` or `folder:<path>`.
entry_action!(OpenEntry, RenameEntry, RevealEntry, TrashEntry, NewNoteIn, NewFolderIn, RestoreTrashed);

/// Moves the entry `.0` into the folder `.1` (a folder path, `""` for Home).
#[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Action)]
#[schemars(crate = "nd::schemars")]
#[action(namespace = formalnotes)]
pub struct MoveEntry(pub String, pub String);

/// Orders Home and folder pages.
#[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Action)]
#[schemars(crate = "nd::schemars")]
#[action(namespace = formalnotes)]
pub struct SetSort(pub SortOrder);

pub const CONTEXT: &str = "FormalNotes";

/// The key combination shown next to a command in the palette.
pub fn shortcut(action: &dyn Action) -> Option<&'static str> {
    let mac = cfg!(target_os = "macos"); // nd-allow(cfg-target-os): shortcut labels only
    let label = |mac_label, other: &'static str| Some(if mac { mac_label } else { other });
    match action.name() {
        "formalnotes::NewNote" => label("⌘N", "Ctrl+N"),
        "formalnotes::NewFolder" => label("⇧⌘N", "Shift+Ctrl+N"),
        "formalnotes::OpenPalette" => label("⌘K", "Ctrl+K"),
        "formalnotes::Search" => label("⌘J", "Ctrl+J"),
        "formalnotes::GoHome" => label("⌘1", "Ctrl+1"),
        "formalnotes::MoveToTrash" => label("⌘⌫", "Ctrl+Backspace"),
        "formalnotes::OpenSettings" => label("⌘,", "Ctrl+,"),
        "nd::ToggleSidebar" => label("⌘\\", "Ctrl+\\"),
        _ => None,
    }
}

/// The menu item that shows an entry in the system's file manager.
pub fn reveal_label() -> &'static str {
    // nd-allow(cfg-target-os): menu labels only
    if cfg!(target_os = "macos") {
        "Reveal in Finder"
    } else if cfg!(windows) {
        "Show in Explorer"
    } else {
        "Show in Files"
    }
}

pub fn init(cx: &mut App) {
    cx.bind_keys(nd::actions::standard_bindings());
    cx.bind_keys([
        KeyBinding::new("secondary-n", NewNote, Some(CONTEXT)),
        KeyBinding::new("secondary-shift-n", NewFolder, Some(CONTEXT)),
        KeyBinding::new("secondary-k", OpenPalette, Some(CONTEXT)),
        KeyBinding::new("secondary-j", Search, Some(CONTEXT)),
        // In the editor Command-F is Edit > Find, the find bar over the note, as in Notes.
        KeyBinding::new("secondary-f", Search, Some("FormalNotes && !NoteEditor")),
        KeyBinding::new("secondary-1", GoHome, Some(CONTEXT)),
        KeyBinding::new("secondary-[", GoBack, Some(CONTEXT)),
        // Not while typing: there Command-Delete deletes to the start of the line.
        KeyBinding::new("secondary-backspace", MoveToTrash, Some("FormalNotes && !NoteEditor")),
        KeyBinding::new("secondary-\\", nd::actions::ToggleSidebar, None),
        KeyBinding::new("secondary-,", OpenSettings, None),
    ]);

    // Move to Trash stays out of the menu bar: a menu key equivalent would take Command-Delete
    // from the editor too. AppKit adds the Edit menu (with Find) and the Window menu, and Close
    // at the end of File.
    set_menus(
        cx,
        [
            Menu::app([MenuItem::action("Settings…", OpenSettings)]),
            Menu::new("File").items([
                MenuItem::action("New Note", NewNote),
                MenuItem::action("New Folder", NewFolder),
                MenuItem::separator(),
                MenuItem::action("Empty Trash", EmptyTrash),
            ]),
            Menu::new("View").items([
                MenuItem::action("Home", GoHome),
                MenuItem::action("Back", GoBack),
                MenuItem::action("Search Notes", Search),
                MenuItem::action("Command Palette", OpenPalette),
                MenuItem::action("Trash", OpenTrash),
                MenuItem::separator(),
                MenuItem::role(MenuRole::ToggleSidebar),
            ]),
            Menu::new("Format").items([MenuItem::role(MenuRole::Bold), MenuItem::role(MenuRole::Italic)]),
            Menu::help([MenuItem::action("Report an Issue…", ReportIssue)]),
        ],
    );
}

/// Writes an incident bundle (tree, last inputs, frames, logs) and reveals it.
pub fn report_issue() {
    if let Some(bundle) = nd::report_issue("user report") {
        let _ = nd::system::reveal_path(&bundle);
    }
}
