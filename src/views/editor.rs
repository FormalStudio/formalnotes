//! The note editor: the title over a centred column of live-preview Markdown. A leading `# `
//! heading is the title and is edited in place of it; the rest of the file is the body. Every
//! keystroke goes to the store, which saves once typing rests.

use std::rc::Rc;
use std::sync::Arc;

use nd::components::markdown;
use nd::prelude::*;

use super::format;
use super::metrics::*;
use crate::library::model::{compose, split, word_count};
use crate::library::{NoteId, Store};
use crate::settings::NotesSettings;

pub const BODY: &str = "body";
pub const TITLE: &str = "title";
pub const TITLE_FIELD: &str = "title-field";
/// The key context of the editor, where text editing keeps its own keys.
pub const EDITOR_CONTEXT: &str = "NoteEditor";

/// Told when a title edit moved the note to a new file name.
type OnRenamed = Rc<dyn Fn(NoteId, &mut Window, &mut App)>;

pub struct Editor {
    store: Entity<Store>,
    note: Option<NoteId>,
    /// The note whose title shows: kept after a close, while the page fades out.
    shown: Option<NoteId>,
    /// Whether the note's text starts with its title as a heading.
    heading: bool,
    body: Entity<TextFieldState>,
    title: Entity<TextFieldState>,
    editing_title: bool,
    /// The text last read from or sent to the store: anything else there came from outside.
    synced: Arc<str>,
    words: usize,
    /// Body text that changed on disk, waiting for a render to put it in the field.
    reload: Option<String>,
    /// The pane's measured width, which sets the text column's.
    pane: Option<f32>,
    on_renamed: OnRenamed,
}

impl Editor {
    pub fn new(
        store: Entity<Store>,
        on_renamed: impl Fn(NoteId, &mut Window, &mut App) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let body = cx.new(|cx| TextFieldState::new(window, cx).placeholder("Start writing"));
        let title = cx.new(|cx| TextFieldState::new(window, cx).placeholder("Title"));
        cx.observe(&body, |this, body, cx| {
            if body.read(cx).last_event() == Some(TextEvent::Edited) {
                let text = body.read(cx).text().to_string();
                this.body_edited(text, cx);
            }
        })
        .detach();
        cx.observe(&store, |this, _, cx| this.store_changed(cx)).detach();
        Self {
            store,
            note: None,
            shown: None,
            heading: false,
            body,
            title,
            editing_title: false,
            synced: Arc::from(""),
            words: 0,
            reload: None,
            pane: None,
            on_renamed: Rc::new(on_renamed),
        }
    }

    pub fn note(&self) -> Option<&NoteId> {
        self.note.as_ref()
    }

    pub fn words(&self) -> usize {
        self.words
    }

    /// The text column's width for the editor width setting, once the pane has been measured.
    fn column(&self, cx: &App) -> f32 {
        let width = cx.settings::<NotesSettings>().editor_width;
        match self.pane {
            Some(pane) => width.column(pane, EDITOR_MARGIN),
            None => width.max().unwrap_or(markdown::MEASURE),
        }
    }

    /// Shows `id`, writing what the previous note had pending first.
    pub fn open(&mut self, id: NoteId, window: &mut Window, cx: &mut Context<Self>) {
        if self.note.as_ref() == Some(&id) {
            return;
        }
        self.store.update(cx, |store, _| store.flush());
        self.note = Some(id.clone());
        self.shown = Some(id);
        self.editing_title = false;
        self.load(window, cx);
    }

    /// The note moved (renamed, or its folder did): keep editing it at its new path.
    pub fn follow(&mut self, id: NoteId, cx: &mut Context<Self>) {
        self.note = Some(id.clone());
        self.shown = Some(id);
        cx.notify();
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.store.update(cx, |store, _| store.flush());
        self.note = None;
        self.editing_title = false;
        cx.notify();
    }

    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.note.as_ref().and_then(|id| self.store.read(cx).note(id)).map(|n| n.text.clone());
        let text = text.unwrap_or_else(|| Arc::from(""));
        let parts = split(&text);
        self.heading = parts.heading.is_some();
        let body = parts.body.to_owned();
        self.words = word_count(&text);
        self.synced = text;
        self.body.update(cx, |state, cx| state.set_text(body, window, cx));
        cx.notify();
    }

    fn body_edited(&mut self, body: String, cx: &mut Context<Self>) {
        let Some(id) = self.note.clone() else { return };
        let title = self.store.read(cx).note(&id).map(|n| n.title.clone()).unwrap_or_default();
        let text = compose(self.heading.then_some(title.as_str()), &body);
        self.words = word_count(&text);
        self.synced = Arc::from(text.as_str());
        self.store.update(cx, |store, cx| store.edit(&id, text, cx));
        cx.notify();
    }

    /// Takes the note's text again when it changed on disk while it had nothing unsaved here.
    fn store_changed(&mut self, cx: &mut Context<Self>) {
        let Some(id) = &self.note else { return };
        let store = self.store.read(cx);
        let Some(note) = store.note(id) else { return };
        if note.text == self.synced || store.is_dirty(id) {
            return;
        }
        let text = note.text.clone();
        let parts = split(&text);
        self.heading = parts.heading.is_some();
        let body = parts.body.to_owned();
        self.words = word_count(&text);
        self.synced = text;
        // Setting a field's text takes a window, which an observer has none of: the next
        // render applies it.
        self.reload = Some(body);
        cx.notify();
    }

    fn start_title_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = &self.note else { return };
        let title = self.store.read(cx).note(id).map(|n| n.title.clone()).unwrap_or_default();
        self.title.update(cx, |state, cx| state.set_text(title, window, cx));
        self.editing_title = true;
        cx.notify();
        window.focus_id(cx, TITLE_FIELD);
    }

    fn commit_title(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editing_title {
            return;
        }
        self.editing_title = false;
        let Some(id) = self.note.clone() else { return };
        let title = self.title.read(cx).text().to_string();
        let moved = self.store.update(cx, |store, cx| store.retitle(&id, &title, cx));
        self.heading = true;
        if let Some(note) = self.store.read(cx).note(&moved) {
            self.synced = note.text.clone();
        }
        self.note = Some(moved.clone());
        self.shown = Some(moved.clone());
        cx.notify();
        if moved != id {
            (self.on_renamed)(moved, window, cx);
        }
    }
}

impl Render for Editor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(body) = self.reload.take() {
            self.body.update(cx, |state, cx| state.set_text(body, window, cx));
        }
        let theme = cx.theme();
        let column = self.column(cx);
        let title = self.shown.as_ref().and_then(|id| self.store.read(cx).note(id)).map(|n| n.title.clone());
        // Everything sits in the text column. AppKit draws a text area with a border until it
        // has a borderless style; then the body can run the pane's full width, scrolling at its
        // edge, and keep its text to the column with `max_measure` alone.
        let in_column = |e: AnyElement| h_flex().flex_none().w_full().justify_center().child(div().w(px(column)).child(e));
        let heading: AnyElement = if self.editing_title {
            text_field(TITLE_FIELD)
                .bind(&self.title)
                .large()
                .label("Title")
                .on_submit(cx.listener(|this, _: &TextSubmitted, window, cx| this.commit_title(window, cx)))
                .on_end_editing(cx.listener(|this, _: &TextEditingEnded, window, cx| this.commit_title(window, cx)))
                .on_cancel(cx.listener(|this, _: &TextCancelled, _, cx| {
                    this.editing_title = false;
                    cx.notify();
                }))
                .my(px((TITLE_LINE - TITLE_SIZE) / 2.))
                .into_any_element()
        } else {
            div()
                .id(TITLE)
                .on_tap(cx.listener(|this, _: &TapEvent, window, cx| this.start_title_edit(window, cx)))
                .child(
                    text(title.unwrap_or_default())
                        .text_size(px(TITLE_SIZE))
                        .line_height(px(TITLE_LINE))
                        .font_weight(FontWeight::BOLD)
                        .text_color(theme.text.primary),
                )
                .into_any_element()
        };
        let words = h_flex()
            .h(px(HEADER_HEIGHT))
            .justify_end()
            .items_center()
            .child(
                text(format::words(self.words))
                    .id("word-count")
                    .tabular_nums()
                    .text_size(theme.type_scale.caption)
                    .text_color(theme.text.secondary),
            )
            .into_any_element();
        let this = cx.weak_entity();
        v_flex()
            .id("editor")
            .key_context(EDITOR_CONTEXT)
            .size_full()
            .pt(px(EDITOR_TOP))
            .on_resize(move |e, _, cx| {
                let _ = this.update(cx, |this, cx| {
                    // The window's first layout reports no width; the column keeps its widest.
                    if e.width >= 1. && this.pane != Some(e.width) {
                        this.pane = Some(e.width);
                        cx.notify();
                    }
                });
            })
            .child(in_column(heading))
            .child(
                markdown_editor(BODY, &self.body)
                    .max_measure(column)
                    .label("Note")
                    .w(px(column))
                    .mx_auto()
                    .flex_1()
                    .min_h_0()
                    .mt(theme.spacing.sm)
                    .rounded(px(0.)),
            )
            .child(in_column(words))
    }
}
