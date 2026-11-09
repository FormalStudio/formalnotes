#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Tier 1: the window over the demo library: pages, the presented note, and the palette.

use formalnotes::actions::{GoBack, NewNote, OpenEntry, OpenPalette};
use formalnotes::library::{FolderId, NoteId};
use formalnotes::nav::Location;
use formalnotes::views::FormalNotes;
use nd::components::TextAreaProps;
use nd::core::{NodeId, Point, WindowId};
use nd::prelude::*;
use nd::test::{Size, TestApp, by_test_id, text};

const WINDOW: Size = Size { width: 1280., height: 800. };

/// Runs frames on a ticking clock until nothing moves: cards fade up and notes grow from their
/// cards, which `run_until_idle` alone, on a stopped clock, never sees finish.
fn idle(cx: &mut TestApp) {
    for _ in 0..600 {
        cx.flush_effects();
        cx.drain_events();
        if !cx.needs_frame() && cx.events().is_empty() {
            return;
        }
        cx.advance_clock(1. / 60.);
        cx.frame();
    }
    panic!("still moving after ten seconds of frames");
}

struct Rig {
    cx: TestApp,
    win: WindowHandle<FormalNotes>,
    _dir: tempfile::TempDir,
}

fn open() -> Rig {
    let dir = tempfile::tempdir().unwrap();
    formalnotes::demo::seed_into(dir.path()).unwrap();
    let mut cx = TestApp::new();
    formalnotes::actions::init(&mut cx);
    let root = dir.path().to_owned();
    let win = cx.open_window(WINDOW, move |window, cx| cx.new(|cx| FormalNotes::new(root, true, window, cx)));
    let mut rig = Rig { cx, win, _dir: dir };
    rig.settle();
    rig
}

impl Rig {
    fn id(&self) -> WindowId {
        self.win.id()
    }

    fn view(&self) -> &FormalNotes {
        self.win.root().read(&self.cx)
    }

    fn settle(&mut self) {
        let store = self.view().store().clone();
        store.update(&mut self.cx, |s, cx| s.settle(cx));
        idle(&mut self.cx);
    }

    fn act(&mut self, action: &dyn Action) {
        let id = self.id();
        assert!(self.cx.dispatch_action(id, action), "{} was not handled", action.name());
        idle(&mut self.cx);
    }

    fn tap(&mut self, test_id: &str) {
        let id = self.id();
        let node = by_test_id(&mut self.cx, id, test_id).unwrap_or_else(|| panic!("no `{test_id}`"));
        self.emit(node.id, TapEvent::new(Point { x: 4., y: 4. }, 1));
    }

    /// As `TestApp::emit`, settling on a ticking clock.
    fn emit<E: 'static>(&mut self, node: NodeId, event: E) {
        self.cx.events().emitter(node).emit(event);
        idle(&mut self.cx);
    }

    fn all_text(&mut self) -> String {
        let id = self.id();
        text(&mut self.cx, id)
    }
}

#[nd::test]
fn home_then_folder_then_note_then_back() {
    let mut rig = open();
    assert_eq!(rig.view().location(), &Location::home());
    let home = rig.all_text();
    assert!(home.contains("Projects") && home.contains("Weekend plans"), "Home shows folders and notes");

    rig.tap("face:folder:Journal");
    assert_eq!(rig.view().location(), &Location::Folder(FolderId("Journal".into())));

    let note = NoteId("Journal/Monday.md".into());
    rig.act(&OpenEntry(format!("note:{}", note.0)));
    assert_eq!(rig.view().open_note(), Some(&note));
    assert!(rig.all_text().contains("Monday"), "the editor shows the note");

    rig.act(&GoBack);
    assert_eq!(rig.view().open_note(), None, "back closes the note first");
    assert_eq!(rig.view().location(), &Location::Folder(FolderId("Journal".into())));

    rig.act(&GoBack);
    assert_eq!(rig.view().location(), &Location::home());
}

#[nd::test]
fn new_note_opens_in_the_current_folder() {
    let mut rig = open();
    rig.tap("face:folder:Recipes");
    rig.act(&NewNote);
    let open = rig.view().open_note().cloned().expect("a note is open");
    assert_eq!(open.folder(), FolderId("Recipes".into()));
    rig.settle();
    assert!(rig.view().store().read(&rig.cx).note(&open).is_some());
}

#[nd::test]
fn a_new_note_writes_on_the_page_not_in_a_field() {
    let mut rig = open();
    rig.act(&NewNote);
    let id = rig.id();
    let body = by_test_id(&mut rig.cx, id, "body").expect("the note's body").id;
    let props = rig.cx.backend().props(body).and_then(|p| p.downcast_ref::<TextAreaProps>()).cloned();
    let props = props.expect("the body is a text area");
    // A bezelled area boxed the body in a bordered rectangle the width of the column.
    assert!(props.borderless, "the body has no field chrome");
    assert!(props.styles.is_some(), "the body styles its Markdown");
}

#[nd::test]
fn palette_opens_notes_and_runs_commands() {
    let mut rig = open();
    rig.act(&OpenPalette);
    assert!(rig.view().palette_open());
    let rows = rig.view().palette_rows(&rig.cx);
    assert!(rows.iter().any(|r| r.id == "cmd:new-folder"), "commands are listed");
    let id = rig.id();
    let palette = by_test_id(&mut rig.cx, id, "palette").expect("the palette").id;

    rig.emit(palette, PaletteQueryChanged { text: "roadmap".into() });
    let rows = rig.view().palette_rows(&rig.cx);
    assert_eq!(rows.first().map(|r| r.id.to_string()), Some("open:note:Projects/Roadmap.md".to_owned()));

    rig.emit(palette, PaletteActivated { id: "open:note:Projects/Roadmap.md".into() });
    assert!(!rig.view().palette_open());
    assert_eq!(rig.view().open_note(), Some(&NoteId("Projects/Roadmap.md".into())));

    rig.act(&OpenPalette);
    rig.emit(palette, PaletteActivated { id: "cmd:trash".into() });
    assert_eq!(rig.view().location(), &Location::Trash);
    assert_eq!(rig.view().open_note(), None);
}

#[nd::test]
fn trash_shows_an_undo_toast_that_puts_it_back() {
    let mut rig = open();
    let note = NoteId("Ideas.md".into());
    rig.act(&formalnotes::actions::TrashEntry(format!("note:{}", note.0)));
    rig.settle();
    assert!(rig.view().store().read(&rig.cx).note(&note).is_none());
    let toast = rig.view().toasts().first().expect("a toast").id.clone();
    let id = rig.id();
    let overlay = by_test_id(&mut rig.cx, id, "toasts").expect("the toasts").id;
    rig.emit(overlay, ToastActionClicked { id: toast });
    rig.settle();
    assert!(rig.view().store().read(&rig.cx).note(&note).is_some(), "undo puts the note back");
}

#[nd::test]
fn the_note_opens_in_a_markdown_editor_on_the_text_column() {
    use nd::components::TextAreaProps;
    use nd::components::markdown::MEASURE;

    let mut rig = open();
    let note = NoteId("Weekend plans.md".into());
    rig.act(&OpenEntry(format!("note:{}", note.0)));
    let id = rig.id();
    let body = by_test_id(&mut rig.cx, id, formalnotes::views::editor::BODY).expect("the body").id;
    let props: TextAreaProps =
        rig.cx.backend().props(body).and_then(|p| p.downcast_ref::<TextAreaProps>()).cloned().expect("a text area");
    assert!(props.styles.is_some(), "styled as markdown");
    assert!(props.borderless, "the body is the page, without a field's chrome");
    assert_eq!(props.max_measure, Some(MEASURE), "the standard column at 1280 wide");
    assert!(props.text.contains("- [ ] Pick up the film scans"), "the body, syntax and all: {}", props.text);
    assert!(!props.text.starts_with("# "), "the title heading is not in the body");
}

#[nd::test]
fn a_trashed_card_leaves_where_it_stood_and_comes_back_on_undo() {
    let mut rig = open();
    let note = NoteId("Ideas.md".into());
    let key = format!("card:note:{}", note.0);
    let id = rig.id();
    let card = by_test_id(&mut rig.cx, id, &key).expect("the card").id;

    assert!(rig.cx.dispatch_action(id, &formalnotes::actions::TrashEntry(format!("note:{}", note.0))));
    rig.cx.frame();
    assert!(rig.cx.backend().is_live(card), "held while it exits");
    // Read without settling: the stopped clock would hold the exit forever.
    let tree = rig.cx.semantic_window(id).expect("the window").root;
    assert!(tree.iter().all(|n| n.test_id.as_deref() != Some(key.as_str())), "out of the accessibility tree");

    idle(&mut rig.cx);
    assert!(!rig.cx.backend().is_live(card), "gone once it has faded");

    let toast = rig.view().toasts().last().expect("a toast").id.clone();
    let overlay = by_test_id(&mut rig.cx, id, "toasts").expect("the toasts").id;
    rig.emit(overlay, ToastActionClicked { id: toast });
    rig.settle();
    assert!(by_test_id(&mut rig.cx, id, &key).is_some(), "undo puts the card back");
}

#[nd::test]
fn a_closing_note_keeps_its_path_and_title_while_it_fades() {
    let mut rig = open();
    let note = NoteId("Journal/Monday.md".into());
    rig.act(&OpenEntry(format!("note:{}", note.0)));
    let id = rig.id();
    assert!(rig.cx.dispatch_action(id, &GoBack));
    rig.cx.advance_clock(1. / 60.);
    rig.cx.frame();
    assert_eq!(rig.view().open_note(), None);
    let tree = rig.cx.semantic_window(id).expect("the window").root;
    let crumb = tree.iter().any(|n| n.test_id.as_deref() == Some("note-crumb:folder:Journal"));
    assert!(crumb, "the page fades out with its path");
}

#[nd::test]
fn a_card_dips_while_it_is_pressed() {
    use nd::test::DivProps;

    let mut rig = open();
    let id = rig.id();
    for key in ["face:note:Ideas.md", "face:folder:Projects"] {
        let face = by_test_id(&mut rig.cx, id, key).unwrap_or_else(|| panic!("no `{key}`")).id;
        let props: DivProps =
            rig.cx.backend().props(face).and_then(|p| p.downcast_ref::<DivProps>()).cloned().expect("a div");
        assert_eq!(props.active.and_then(|p| p.scale), Some(0.98), "{key} dips to 98%");
    }
}
