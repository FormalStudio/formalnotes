#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Tier 1: the window over the demo library: pages, the presented note, and the palette.

use formalnotes::actions::{GoBack, NewFolder, NewNote, OpenEntry, OpenPalette};
use formalnotes::library::{FolderId, NoteId};
use formalnotes::nav::Location;
use formalnotes::views::FormalNotes;
use nd::components::{SourceTreeProps, TextAreaProps};
use nd::core::{NodeId, Point, WindowId};
use nd::prelude::*;
use nd::test::{DragPreview, SPRING_LOAD_DELAY, Size, TestApp, by_test_id, text};

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

    fn view_region(&self) -> (f32, f32) {
        self.view().region().expect("the region is measured")
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
fn a_new_folder_opens_its_rename_with_the_name_selected() {
    use nd::components::TextFieldProps;
    let mut rig = open();
    rig.act(&NewFolder);
    let id = rig.id();
    let field = by_test_id(&mut rig.cx, id, "rename-field").expect("the rename field").id;
    let props = rig.cx.backend().props(field).and_then(|p| p.downcast_ref::<TextFieldProps>()).cloned();
    let props = props.expect("a text field");
    let name = props.text.chars().count();
    assert!(name > 0, "the field starts with the folder's name");
    assert_eq!(props.select_on_focus, Some(0..name), "typing replaces the whole name");
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

/// The node `test_id` without settling: a stopped clock would hold its motion forever.
fn node_now(rig: &mut Rig, test_id: &str) -> Option<NodeId> {
    let tree = rig.cx.semantic_window(rig.id())?.root;
    tree.iter().find(|n| n.test_id.as_deref() == Some(test_id)).map(|n| n.id)
}

fn div_props(rig: &mut Rig, test_id: &str) -> Option<nd::test::DivProps> {
    let node = node_now(rig, test_id)?;
    rig.cx.backend().props(node).and_then(|p| p.downcast_ref::<nd::test::DivProps>()).cloned()
}

/// One frame on a moving clock, without settling.
fn step(rig: &mut Rig, seconds: f64) {
    rig.cx.flush_effects();
    rig.cx.drain_events();
    rig.cx.advance_clock(seconds);
    rig.cx.frame();
}

fn page_comes_in(reduce: bool) {
    let mut rig = open();
    rig.cx.set_reduce_motion(reduce);
    let id = rig.id();
    let home = by_test_id(&mut rig.cx, id, &format!("page:{}", formalnotes::nav::HOME_KEY)).expect("Home's page").id;
    assert!(rig.cx.dispatch_action(id, &OpenEntry("folder:Journal".into())));
    step(&mut rig, 1. / 60.);
    step(&mut rig, 0.05);
    let page = div_props(&mut rig, "page:folder:Journal").expect("the folder's page");
    let o = page.style.opacity.expect("fading in");
    assert!(o > 0. && o < 1., "fading in: {o}");
    if reduce {
        assert_eq!(page.transform.offset, (0., 0.), "reduce motion only fades");
    } else {
        assert!(page.transform.offset.1 > 0., "rising: {:?}", page.transform.offset);
    }
    let old = rig.cx.backend().props(home).and_then(|p| p.downcast_ref::<nd::test::DivProps>().cloned());
    let old = old.expect("Home's page is held while it fades out");
    assert!(old.inert && old.style.opacity.unwrap_or(1.) < 1., "Home fades out where it stood");
    idle(&mut rig.cx);
    assert!(!rig.cx.backend().is_live(home), "gone once faded");
    assert_eq!(div_props(&mut rig, "page:folder:Journal").and_then(|p| p.style.opacity).unwrap_or(1.), 1.);
}

#[nd::test]
fn a_new_page_rises_in_over_the_old_one_fading_out() {
    page_comes_in(false);
}

#[nd::test]
fn under_reduce_motion_pages_crossfade() {
    page_comes_in(true);
}

fn switch_notes(reduce: bool) {
    let mut rig = open();
    rig.cx.set_reduce_motion(reduce);
    let (monday, ideas) = (NoteId("Journal/Monday.md".into()), NoteId("Ideas.md".into()));
    rig.act(&OpenEntry(format!("note:{}", monday.0)));
    let id = rig.id();
    assert!(rig.cx.dispatch_action(id, &OpenEntry(format!("note:{}", ideas.0))));
    step(&mut rig, 1. / 60.);
    step(&mut rig, 0.05);
    assert_eq!(rig.view().open_note(), Some(&monday), "the open note's text fades out first");
    let o = div_props(&mut rig, "note-content").and_then(|p| p.style.opacity).expect("fading");
    assert!(o < 1., "fading out: {o}");
    idle(&mut rig.cx);
    assert_eq!(rig.view().open_note(), Some(&ideas), "then the other note takes the editor");
    assert_eq!(div_props(&mut rig, "note-content").and_then(|p| p.style.opacity).unwrap_or(1.), 1.);
    assert!(rig.all_text().contains("Ideas"));
}

#[nd::test]
fn switching_notes_in_the_editor_crossfades_the_text() {
    switch_notes(false);
}

#[nd::test]
fn under_reduce_motion_switching_notes_crossfades_too() {
    switch_notes(true);
}

#[nd::test]
fn a_new_note_grows_out_of_its_new_card() {
    let mut rig = open();
    rig.act(&OpenEntry("folder:Journal".into()));
    let id = rig.id();
    assert!(rig.cx.dispatch_action(id, &NewNote));
    for _ in 0..3 {
        step(&mut rig, 1. / 60.);
    }
    step(&mut rig, 0.05);
    let paper = node_now(&mut rig, "note-paper").expect("the note's paper");
    let width = rig.cx.backend().frame(paper).expect("laid out").size.width;
    let (region, _) = rig.view_region();
    assert!(width < region - 1., "on its way out of the card, not popped to full size: {width} of {region}");
    idle(&mut rig.cx);
    let width = rig.cx.backend().frame(paper).expect("laid out").size.width;
    assert!((width - region).abs() < 1., "fills the region: {width} of {region}");
}

fn new_note_over(reduce: bool) {
    let mut rig = open();
    rig.cx.set_reduce_motion(reduce);
    let monday = NoteId("Journal/Monday.md".into());
    rig.act(&OpenEntry(format!("note:{}", monday.0)));
    let id = rig.id();
    assert!(rig.cx.dispatch_action(id, &NewNote));
    step(&mut rig, 1. / 60.);
    step(&mut rig, 0.05);
    assert_eq!(rig.view().open_note(), Some(&monday), "the open note's text fades out first");
    let o = div_props(&mut rig, "note-content").and_then(|p| p.style.opacity).expect("fading");
    assert!(o < 1., "fading out: {o}");
    idle(&mut rig.cx);
    let open = rig.view().open_note().cloned().expect("a note is open");
    assert_ne!(open, monday, "the new note takes the editor");
    assert_eq!(open.folder(), monday.folder());
}

#[nd::test]
fn a_new_note_over_an_open_one_crossfades_in() {
    new_note_over(false);
}

#[nd::test]
fn under_reduce_motion_a_new_note_over_an_open_one_crossfades_in() {
    new_note_over(true);
}

/// Where the presented note's sheet stands in the window this frame, and how wide it is.
fn sheet(rig: &mut Rig) -> Option<(f32, f32, f32)> {
    let paper = node_now(rig, "note-paper")?;
    let props = div_props(rig, "note-paper")?;
    let frame = rig.cx.backend().window_frame_of(paper)?;
    let (dx, dy) = props.transform.offset;
    Some((frame.origin.x + dx, frame.origin.y + dy, frame.size.width))
}

fn window_frame(rig: &mut Rig, test_id: &str) -> nd::core::Rect {
    let node = node_now(rig, test_id).unwrap_or_else(|| panic!("no `{test_id}`"));
    rig.cx.backend().window_frame_of(node).expect("laid out")
}

/// Leaves Journal/Monday by `leave` and records where the sheet stands each frame until it
/// is gone, with the frame its card had while the note was open.
fn leave_note(leave: impl FnOnce(&mut Rig)) -> (Rig, nd::core::Rect, Vec<(f32, f32, f32)>) {
    let mut rig = open();
    rig.act(&OpenEntry("folder:Journal".into()));
    let card = window_frame(&mut rig, "frame:note:Journal/Monday.md");
    rig.act(&OpenEntry("note:Journal/Monday.md".into()));
    leave(&mut rig);
    let mut shots = Vec::new();
    for _ in 0..600 {
        step(&mut rig, 1. / 60.);
        match sheet(&mut rig) {
            Some(s) => shots.push(s),
            None => break,
        }
    }
    (rig, card, shots)
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1.
}

#[nd::test]
fn a_note_left_for_home_shrinks_into_its_folders_card() {
    let (mut rig, card, shots) = leave_note(|rig| {
        let crumb = node_now(rig, "note-crumb:home").expect("Home in the note's path");
        rig.cx.events().emitter(crumb).emit(TapEvent::new(Point { x: 4., y: 4. }, 1));
    });
    assert_eq!(rig.view().location(), &Location::home());
    let on_card =
        shots.iter().any(|&(x, y, w)| near(x, card.origin.x) && near(y, card.origin.y) && near(w, card.size.width));
    assert!(!on_card, "never lands where the card stood in the folder: {card:?} {shots:?}");
    let folder = window_frame(&mut rig, "face:folder:Journal");
    let &(x, y, w) =
        shots.iter().rev().find(|s| s.2 < folder.size.width + 1.).unwrap_or_else(|| panic!("{folder:?} {shots:?}"));
    assert!(near(x, folder.origin.x) && near(y, folder.origin.y) && near(w, folder.size.width), "{folder:?} {shots:?}");
}

#[nd::test]
fn a_note_left_for_the_trash_fades_where_it_is() {
    let (rig, _, shots) = leave_note(|rig| {
        rig.win.root().clone().update(&mut rig.cx, |v, cx| v.go(Location::Trash, cx));
    });
    assert_eq!(rig.view().location(), &Location::Trash);
    let (region, _) = rig.view_region();
    let &(x0, y0, _) = shots.first().expect("fades out over a few frames");
    assert!(shots.iter().all(|&(x, y, w)| near(x, x0) && near(y, y0) && near(w, region)), "no flight: {shots:?}");
}

#[nd::test]
fn a_note_closed_with_back_lands_on_its_card() {
    let (mut rig, _, shots) = leave_note(|rig| {
        let id = rig.id();
        assert!(rig.cx.dispatch_action(id, &GoBack));
    });
    let card = window_frame(&mut rig, "frame:note:Journal/Monday.md");
    let &(x, y, w) = shots.last().expect("frames");
    assert!(near(x, card.origin.x) && near(y, card.origin.y) && near(w, card.size.width), "{card:?} {shots:?}");
}

/// Drags the card `from` with real drag events, one frame per step, so a spring-loaded folder
/// opens only when the clock says so.
struct CardDrag {
    card: NodeId,
    start: Point,
}

impl Rig {
    fn centre(&mut self, test_id: &str) -> Point {
        let id = self.id();
        let b =
            by_test_id(&mut self.cx, id, test_id).and_then(|n| n.bounds).unwrap_or_else(|| panic!("no `{test_id}`"));
        Point { x: b.origin.x + b.size.width / 2., y: b.origin.y + b.size.height / 2. }
    }

    fn lift(&mut self, card: &str) -> CardDrag {
        let id = self.id();
        self.cx.backend_mut().set_window_frame(id, nd::core::Rect::new(0., 0., WINDOW.width, WINDOW.height));
        let start = self.centre(card);
        let card = by_test_id(&mut self.cx, id, card).expect("the card").id;
        let drag = CardDrag { card, start };
        self.drag_step(&drag, GesturePhase::Began, start);
        drag
    }

    fn drag_step(&mut self, drag: &CardDrag, phase: GesturePhase, to: Point) {
        let translation = Point { x: to.x - drag.start.x, y: to.y - drag.start.y };
        self.cx.events().emitter(drag.card).emit(DragEvent {
            phase,
            start: drag.start,
            position: to,
            translation,
            velocity: Point::default(),
            modifiers: Default::default(),
        });
        self.cx.drain_events();
        self.cx.advance_clock(1. / 60.);
        self.cx.frame();
    }

    fn wait(&mut self, seconds: f64) {
        self.cx.advance_clock(seconds);
        self.cx.frame();
        self.cx.drain_events();
    }

    fn undo_last(&mut self) {
        let toast = self.view().toasts().last().expect("a toast").id.clone();
        let id = self.id();
        let overlay = by_test_id(&mut self.cx, id, "toasts").expect("the toasts").id;
        self.emit(overlay, ToastActionClicked { id: toast });
        self.settle();
    }

    fn has_note(&self, note: &str) -> bool {
        self.view().store().read(&self.cx).note(&NoteId(note.into())).is_some()
    }
}

#[nd::test]
fn a_card_dropped_on_a_folder_card_moves_in_and_undo_brings_it_back() {
    let mut rig = open();
    let folder = rig.centre("card:folder:Journal");
    let drag = rig.lift("card:note:Ideas.md");
    rig.drag_step(&drag, GesturePhase::Changed, folder);
    rig.drag_step(&drag, GesturePhase::Ended, folder);
    rig.settle();
    assert!(rig.has_note("Journal/Ideas.md") && !rig.has_note("Ideas.md"), "moved into Journal");
    assert_eq!(rig.view().location(), &Location::home(), "the page stays");
    let landed = rig.cx.backend().previews().iter().any(|(_, p)| matches!(p, DragPreview::Land { into: true, .. }));
    assert!(landed, "the picture shrinks into the folder");

    rig.undo_last();
    assert!(rig.has_note("Ideas.md") && !rig.has_note("Journal/Ideas.md"), "undo moves it back");
}

#[nd::test]
fn a_drag_resting_on_a_folder_card_opens_it_after_the_delay_and_not_before() {
    let mut rig = open();
    let folder = rig.centre("card:folder:Journal");
    let drag = rig.lift("card:note:Ideas.md");
    rig.drag_step(&drag, GesturePhase::Changed, folder);
    rig.wait(SPRING_LOAD_DELAY - 0.1);
    assert_eq!(rig.view().location(), &Location::home(), "not yet");
    rig.wait(0.15);
    assert_eq!(rig.view().location(), &Location::Folder(FolderId("Journal".into())), "sprung open");
    // The folder's page is up; the card it sprang from is on its way out and takes nothing.
    rig.wait(1. / 60.);
    rig.drag_step(&drag, GesturePhase::Ended, folder);
    rig.settle();
    assert!(rig.has_note("Ideas.md"), "a release over no target leaves it where it was");
}

#[nd::test]
fn escape_cancels_a_card_drag_and_the_picture_settles_back() {
    let mut rig = open();
    let folder = rig.centre("card:folder:Journal");
    let drag = rig.lift("card:note:Ideas.md");
    rig.drag_step(&drag, GesturePhase::Changed, folder);
    let id = rig.id();
    rig.cx.dispatch_keystroke(id, &nd::prelude::gpui::Keystroke::parse("escape").unwrap());
    rig.drag_step(&drag, GesturePhase::Ended, folder);
    rig.settle();
    assert!(rig.has_note("Ideas.md"), "nothing moved");
    assert_eq!(rig.view().location(), &Location::home());
    let last = rig.cx.backend().previews().iter().rev().find(|(_, p)| !matches!(p, DragPreview::Gone)).map(|(_, p)| *p);
    assert!(matches!(last, Some(DragPreview::Land { into: false, .. })), "it settles home: {last:?}");
    assert!(rig.view().toasts().is_empty());
}

fn sidebar_drop(rig: &mut Rig, list: &str, entry: &str, on: &str) {
    let id = rig.id();
    let node = by_test_id(&mut rig.cx, id, list).expect("the list").id;
    let slot = DropSlot { parent: None, index: 0, on: Some(on.to_owned().into()) };
    rig.emit(
        node,
        Dropped {
            item: DragItem::new(formalnotes::nav::ENTRY_DRAG, entry.to_owned()),
            position: Point::default(),
            local: Point::default(),
            source_window: id,
            slot: Some(slot),
            modifiers: Default::default(),
        },
    );
    rig.settle();
}

#[nd::test]
fn a_card_dropped_on_a_sidebar_folder_or_home_moves_there() {
    let mut rig = open();
    sidebar_drop(&mut rig, "sidebar", "card:note:Ideas.md", "folder:Projects");
    assert!(rig.has_note("Projects/Ideas.md"));
    sidebar_drop(&mut rig, "sidebar", "note:Projects/Ideas.md", "home");
    assert!(rig.has_note("Ideas.md"), "Home takes it back to the library root");
}

#[nd::test]
fn a_drag_resting_on_a_sidebar_folder_expands_it() {
    let mut rig = open();
    let id = rig.id();
    let sidebar = by_test_id(&mut rig.cx, id, "sidebar").expect("the sidebar").id;
    let expanded = |rig: &Rig| {
        let props = rig.cx.backend().props(sidebar).and_then(|p| p.downcast_ref::<SourceTreeProps>()).cloned();
        props.expect("a source tree").nodes.iter().any(|n| n.id == "folder:Projects" && n.expanded)
    };
    assert!(!expanded(&rig));
    let slot = DropSlot { parent: None, index: 0, on: Some("folder:Projects".into()) };
    let item = DragItem::new(formalnotes::nav::ENTRY_DRAG, "card:note:Ideas.md");
    rig.emit(sidebar, SpringLoaded { item, slot: Some(slot) });
    assert!(expanded(&rig));
}

#[nd::test]
fn a_card_dropped_on_the_trash_row_goes_to_the_trash_with_undo() {
    let mut rig = open();
    sidebar_drop(&mut rig, "sidebar-trash", "card:note:Ideas.md", "trash");
    assert!(!rig.has_note("Ideas.md"));
    assert_eq!(rig.view().store().read(&rig.cx).library().trash.len(), 1);
    rig.undo_last();
    assert!(rig.has_note("Ideas.md"), "undo puts it back");
}
