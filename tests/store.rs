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

#[nd::test]
fn a_folder_view_is_written_read_back_and_keeps_unknown_keys() {
    use formalnotes::library::{Column, FolderView, PropType};
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir(root.join("Tasks")).unwrap();
    std::fs::write(root.join("Tasks/.folder.json"), r#"{"v":1,"view":"cards","columns":[],"layout":{"x":1}}"#).unwrap();
    let mut cx = TestApp::new();
    let store = store(&mut cx, root);
    let tasks = FolderId("Tasks".into());
    assert!(!store.read(&cx).library().is_table(&tasks));

    let mut view = store.read(&cx).view(&tasks);
    view.set_table(true);
    view.columns.push(Column::new("status", PropType::Select));
    store.update(&mut cx, |s, cx| s.set_view(&tasks, view, cx));
    settle(&mut cx, &store);
    let written: serde_json::Value = serde_json::from_str(&read(root, "Tasks/.folder.json")).unwrap();
    assert_eq!(written["view"], "table");
    assert_eq!(written["columns"][0]["type"], "select");
    assert_eq!(written["layout"]["x"], 1, "unknown keys survive the rewrite");
    assert!(store.read(&cx).library().is_table(&tasks), "the snapshot after the write agrees");
    assert!(std::fs::read_dir(root.join("Tasks")).unwrap().count() == 1, "no temp file left");

    // Another app changes it: the watcher picks it up.
    std::fs::write(root.join("Tasks/.folder.json"), r#"{"v":1,"view":"cards"}"#).unwrap();
    std::thread::sleep(Duration::from_millis(400));
    settle(&mut cx, &store);
    assert!(!store.read(&cx).library().is_table(&tasks), "an outside edit is read");
    assert_eq!(store.read(&cx).view(&tasks), FolderView::parse(br#"{"v":1,"view":"cards"}"#).unwrap());

    // The view moves with its folder.
    let moved = store.update(&mut cx, |s, cx| s.rename_folder(&tasks, "Todo", cx));
    assert!(store.read(&cx).library().view(&moved).is_some());
    settle(&mut cx, &store);
    assert!(store.read(&cx).library().view(&moved).is_some());
}

#[nd::test]
fn properties_and_columns_reach_the_notes() {
    use formalnotes::library::{Column, PropType, Value};
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir(root.join("Tasks")).unwrap();
    std::fs::write(root.join("Tasks/A.md"), "---\nstatus: Doing\nkeep: [x]\n---\n# A\n").unwrap();
    std::fs::write(root.join("Tasks/B.md"), "# B\n\nbody").unwrap();
    std::fs::write(root.join("Other.md"), "---\nstatus: Elsewhere\n---\n").unwrap();
    let mut cx = TestApp::new();
    let store = store(&mut cx, root);
    let tasks = FolderId("Tasks".into());
    let (a, b) = (NoteId("Tasks/A.md".into()), NoteId("Tasks/B.md".into()));

    store.update(&mut cx, |s, cx| s.set_property(&b, "done", Some(&Value::Checkbox(true)), cx));
    settle(&mut cx, &store);
    assert_eq!(read(root, "Tasks/B.md"), "---\ndone: true\n---\n# B\n\nbody", "written at once");
    assert_eq!(store.read(&cx).note(&b).unwrap().title, "B");

    let mut view = store.read(&cx).view(&tasks);
    view.columns.push(Column::new("status", PropType::Select));
    store.update(&mut cx, |s, cx| s.set_view(&tasks, view, cx));
    assert!(store.update(&mut cx, |s, cx| s.rename_column(&tasks, "status", "state", cx)));
    settle(&mut cx, &store);
    assert_eq!(read(root, "Tasks/A.md"), "---\nstate: Doing\nkeep: [x]\n---\n# A\n");
    assert_eq!(read(root, "Other.md"), "---\nstatus: Elsewhere\n---\n", "only the folder's notes");
    assert!(store.read(&cx).view(&tasks).column("state").is_some());

    let mut view = store.read(&cx).view(&tasks);
    view.remove_column("state");
    store.update(&mut cx, |s, cx| s.set_view(&tasks, view, cx));
    settle(&mut cx, &store);
    assert_eq!(read(root, "Tasks/A.md"), "---\nstate: Doing\nkeep: [x]\n---\n# A\n", "deleting a column leaves notes");

    store.update(&mut cx, |s, cx| s.set_property(&a, "state", None, cx));
    store.update(&mut cx, |s, cx| s.set_property(&a, "keep", None, cx));
    settle(&mut cx, &store);
    assert_eq!(read(root, "Tasks/A.md"), "# A\n", "the empty block goes with its fences");
}
