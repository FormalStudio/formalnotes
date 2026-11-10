//! Toasts, and what their Undo button reverses.

use std::collections::HashMap;

use nd::prelude::*;

use crate::library::{EntryId, FolderId};

/// What a toast's Undo does.
#[derive(Clone, Debug, PartialEq)]
pub enum Undo {
    /// Put the trashed item with this name back.
    Restore(String),
    /// Move the restored entry to the trash again.
    Trash(EntryId),
    /// Move the entry back to the folder it came from.
    Move(EntryId, FolderId),
    /// Take back what an automation filed and put its note back.
    Organized(crate::ai::organize::Applied),
}

#[derive(Default)]
pub struct Notices {
    toasts: Vec<Toast>,
    undo: HashMap<SharedString, Undo>,
    /// What a click on the toast opens.
    opens: HashMap<SharedString, EntryId>,
    next: u64,
}

impl Notices {
    pub fn toasts(&self) -> &[Toast] {
        &self.toasts
    }

    pub fn show(&mut self, title: impl Into<SharedString>, undo: Option<Undo>) {
        self.show_opening(title, undo, None);
    }

    /// A toast that opens `opens` when clicked.
    pub fn show_opening(&mut self, title: impl Into<SharedString>, undo: Option<Undo>, opens: Option<EntryId>) {
        self.next += 1;
        let id: SharedString = format!("toast-{}", self.next).into();
        if let Some(entry) = opens {
            self.opens.insert(id.clone(), entry);
        }
        let toast = toast(id.clone(), title);
        let toast = match undo {
            Some(undo) => {
                self.undo.insert(id, undo);
                toast.action("Undo")
            }
            None => toast,
        };
        self.toasts.push(toast);
    }

    /// The toast left; returns what it opens when the user clicked it away.
    pub fn dismissed(&mut self, id: &str, clicked: bool) -> Option<EntryId> {
        self.toasts.retain(|t| t.id != id);
        self.undo.remove(id);
        self.opens.remove(id).filter(|_| clicked)
    }

    /// The undo behind the toast's button, taken so it runs once.
    pub fn take_undo(&mut self, id: &str) -> Option<Undo> {
        self.undo.remove(id)
    }
}
