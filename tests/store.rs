#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Tier 1: the store over a real folder. The tests settle the store instead of pumping its
//! worker, so every assertion sees what reached the disk.

use std::path::Path;
use std::time::Duration;

use formalnotes::library::{EntryId, FolderId, NoteId, Store};
use nd::prelude::*;
use nd::test::TestApp;

fn store(cx: &mut TestApp, root: &Path) -> Entity<Store> {
    let store = cx.new(|cx| Store::new(root.to_owned(), cx));
    settle(cx, &store);
    store
}

fn settle(cx: &mut TestApp, store: &Entity<Store>) {
    store.update(cx, |store, cx| store.settle(cx));
}

fn read(root: &Path, path: &str) -> String {
    std::fs::read_to_string(root.join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

#[nd::test]
fn titles_come_from_the_heading_or_the_file_name() {
    let dir = tempfile::tempdir().unwrap();
    formalnotes::demo::seed_into(dir.path()).unwrap();
    let mut cx = TestApp::new();
    let store = store(&mut cx, dir.path());
    let library = store.read(&cx).library().clone();
    assert_eq!(library.note(&NoteId("Weekend plans.md".into())).unwrap().title, "Weekend plans");
    assert_eq!(library.note(&NoteId("Reading notes.md".into())).unwrap().title, "Reading notes", "no heading");
    assert!(library.has_folder(&FolderId("Projects/Launch".into())));
    assert_eq!(library.notes.len(), 20);
}

#[nd::test]
fn create_rename_move_trash_and_restore_reach_the_disk() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut cx = TestApp::new();
    let store = store(&mut cx, root);

    let note = store.update(&mut cx, |s, cx| s.create_note(&FolderId::root(), cx));
    assert_eq!(note.0, "Untitled.md");
    let note = store.update(&mut cx, |s, cx| s.retitle(&note, "Groceries", cx));
    assert_eq!(note.0, "Groceries.md");
    let folder = store.update(&mut cx, |s, cx| s.create_folder(&FolderId::root(), cx));
    assert_eq!(folder.0, "New Folder");
    let folder = store.update(&mut cx, |s, cx| s.rename_folder(&folder, "Lists", cx));
    store.update(&mut cx, |s, _| s.flush());
    settle(&mut cx, &store);
    assert!(read(root, "Groceries.md").starts_with("# Groceries"));
    assert!(!root.join("Untitled.md").exists());
    assert!(root.join("Lists").is_dir() && !root.join("New Folder").exists());

    let moved = store.update(&mut cx, |s, cx| s.move_into(&EntryId::Note(note), &folder, cx));
    assert_eq!(moved, EntryId::Note(NoteId("Lists/Groceries.md".into())));
    settle(&mut cx, &store);
    assert!(read(root, "Lists/Groceries.md").starts_with("# Groceries"));

    let name = store.update(&mut cx, |s, cx| s.trash(&EntryId::Folder(folder.clone()), cx)).expect("trashed");
    settle(&mut cx, &store);
    assert!(!root.join("Lists").exists());
    assert_eq!(store.read(&cx).library().trash.len(), 1);
    assert!(store.read(&cx).library().notes.is_empty());

    let back = store.update(&mut cx, |s, cx| s.restore(&name, cx)).expect("restored");
    assert_eq!(back, EntryId::Folder(folder));
    settle(&mut cx, &store);
    assert!(read(root, "Lists/Groceries.md").starts_with("# Groceries"));
    assert!(store.read(&cx).library().trash.is_empty());
    assert!(store.read(&cx).note(&NoteId("Lists/Groceries.md".into())).is_some());
}

#[nd::test]
fn edits_save_once_typing_rests() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("Draft.md"), "# Draft\n").unwrap();
    let mut cx = TestApp::new();
    let store = store(&mut cx, root);
    let id = NoteId("Draft.md".into());

    store.update(&mut cx, |s, cx| s.edit(&id, "# Draft\n\nOne".into(), cx));
    store.update(&mut cx, |s, cx| s.edit(&id, "# Draft\n\nOne two".into(), cx));
    settle(&mut cx, &store);
    assert_eq!(read(root, "Draft.md"), "# Draft\n", "nothing written while typing");
    assert!(store.read(&cx).is_dirty(&id));
    assert_eq!(&*store.read(&cx).note(&id).unwrap().text, "# Draft\n\nOne two", "the store has it already");

    std::thread::sleep(Duration::from_millis(700));
    settle(&mut cx, &store);
    assert_eq!(read(root, "Draft.md"), "# Draft\n\nOne two");
    assert!(!store.read(&cx).is_dirty(&id));
}

#[nd::test]
fn paths_outside_the_library_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut cx = TestApp::new();
    let store = store(&mut cx, dir.path());
    let moved =
        store.update(&mut cx, |s, cx| s.move_into(&EntryId::Note(NoteId("../x.md".into())), &FolderId::root(), cx));
    assert_eq!(moved, EntryId::Note(NoteId("../x.md".into())), "an unknown note does not move");
    settle(&mut cx, &store);
    assert!(store.read(&cx).failures().is_empty());
}
