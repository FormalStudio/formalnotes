//! The window's side of AI: the Ask pane in the inspector, "Organize This Note", and the
//! notices the automations leave.

use nd::prelude::*;

use super::ask::{Ask, FIELD};
use super::notices::Undo;
use super::root::FormalNotes;
use crate::ai::chat::AiError;
use crate::ai::organize::{self, Applied};
use crate::ai::{Automations, Outcome};
use crate::library::{EntryId, Store};

pub(super) const ASK_WIDTH: f32 = 360.;
const ASK_MIN: f32 = 300.;
const ASK_MAX: f32 = 640.;

impl FormalNotes {
    pub(super) fn ai_parts(store: &Entity<Store>, cx: &mut Context<Self>) -> (Entity<Ask>, Entity<Automations>) {
        let ask = cx.new(|cx| Ask::new(store.clone(), cx));
        let automations = cx.new(|cx| Automations::new(store.clone(), cx));
        cx.observe(&automations, |this, _, cx| this.automations_changed(cx)).detach();
        (ask, automations)
    }

    pub fn ask(&self) -> &Entity<Ask> {
        &self.ask
    }

    pub fn automations(&self) -> &Entity<Automations> {
        &self.automations
    }

    pub fn ask_open(&self) -> bool {
        self.ask_open
    }

    pub fn set_ask_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.ask_open = open;
        cx.notify();
    }

    /// Shows the Ask pane with its field focused, or hides it.
    pub(super) fn toggle_ask(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ask_open = !self.ask_open;
        if self.ask_open {
            window.focus_id(cx, FIELD);
        }
        cx.notify();
    }

    pub(super) fn organize_open_note(&mut self, cx: &mut Context<Self>) {
        let Some(note) = self.open.clone() else { return };
        let started = self.automations.update(cx, |a, cx| a.organize_now(&note, cx));
        let title = self.store.read(cx).note(&note).map(|n| n.title.clone()).unwrap_or_default();
        let line = if started {
            format!("Organizing “{title}”…")
        } else {
            "Another note is being organized. Try again in a moment.".to_owned()
        };
        self.notices.show(line, None);
        cx.notify();
    }

    fn automations_changed(&mut self, cx: &mut Context<Self>) {
        let open = self.open.clone();
        let outcomes = self.automations.update(cx, |a, _| {
            a.set_open(open);
            a.take_outcomes()
        });
        for outcome in outcomes {
            match outcome {
                Outcome::Organized(applied) => {
                    let line = organized_line(&applied, self.store.read(cx).library());
                    let first = applied.created.first().cloned().map(EntryId::Note);
                    self.notices.show_opening(line, Some(Undo::Organized(applied)), first);
                }
                Outcome::Failed { title, error } => self.notices.show(failed_line(&title, &error), None),
            }
        }
        cx.notify();
    }

    pub(super) fn undo_organized(&mut self, applied: &Applied, cx: &mut Context<Self>) {
        let back = self.store.update(cx, |store, cx| organize::undo(store, applied, cx));
        if back.is_some() {
            self.notices.show(format!("Put back “{}”", applied.source_title), None);
        }
        cx.notify();
    }

    /// The pane while it is open. A closed one is left out rather than collapsed: the test
    /// backend lays a collapsed inspector out at full width.
    pub(super) fn render_ask(&self, cx: &mut Context<Self>) -> Option<Pane> {
        if !self.ask_open {
            return None;
        }
        let pane = nd::components::inspector()
            .resizable()
            .width(px(self.ask_width))
            .min_w(px(ASK_MIN))
            .max_w(px(ASK_MAX))
            .on_resize(cx.listener(|this, e: &nd::components::PaneResized, _, cx| {
                this.ask_width = e.width;
                cx.notify();
            }))
            .on_collapse_change(cx.listener(|this, e: &nd::components::PaneCollapsed, _, cx| {
                this.ask_open = !e.collapsed;
                cx.notify();
            }))
            .child(self.ask.clone());
        Some(pane)
    }
}

/// "Filed “Dump” as “Plan” in Projects", or "as 3 notes".
fn organized_line(applied: &Applied, library: &crate::library::Library) -> String {
    let source = &applied.source_title;
    match applied.created.as_slice() {
        [one] => {
            let title = library.note(one).map_or_else(|| one.stem().to_owned(), |n| n.title.clone());
            let folder = one.folder();
            let place = if folder.is_root() { "Home".to_owned() } else { folder.0.replace('/', " / ") };
            format!("Filed “{source}” as “{title}” in {place}")
        }
        many => format!("Filed “{source}” as {} notes", many.len()),
    }
}

fn failed_line(title: &str, error: &AiError) -> String {
    format!("Could not organize “{title}”. {}", error.line())
}
