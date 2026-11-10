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
}

#[derive(Default)]
pub struct Notices {
    toasts: Vec<Toast>,
    undo: HashMap<SharedString, Undo>,
    next: u64,
}

impl Notices {
    pub fn toasts(&self) -> &[Toast] {
        &self.toasts
    }

    pub fn show(&mut self, title: impl Into<SharedString>, undo: Option<Undo>) {
        self.next += 1;
        let id: SharedString = format!("toast-{}", self.next).into();
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

    pub fn dismissed(&mut self, id: &str) {
        self.toasts.retain(|t| t.id != id);
        self.undo.remove(id);
    }

    /// The undo behind the toast's button, taken so it runs once.
    pub fn take_undo(&mut self, id: &str) -> Option<Undo> {
        self.undo.remove(id)
    }
}
