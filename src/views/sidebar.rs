//! The sidebar's rows: Home, the library as a tree of folders and notes, and the trash. Built
//! from the library each render; expansion and selection are the window's.

use std::collections::HashSet;

use nd::prelude::*;

use crate::library::{FolderId, Library, Sort};
use crate::nav::{HOME_KEY, LIBRARY_SECTION, TRASH_KEY, folder_key, note_key};

/// The row action on folders that adds a note inside them.
pub const NEW_NOTE_HERE: &str = "new-note-here";

/// Every row of the tree, parents before children.
pub fn nodes(library: &Library, sort: Sort, expanded: &HashSet<FolderId>) -> Vec<SourceNode> {
    let mut out = vec![source_node(HOME_KEY, "Home").icon(Icon::House).movable(false)];
    out.push(source_section(LIBRARY_SECTION, "Notes"));
    add_level(library, sort, expanded, &FolderId::root(), LIBRARY_SECTION.into(), &mut out);
    out
}

fn add_level(
    library: &Library,
    sort: Sort,
    expanded: &HashSet<FolderId>,
    folder: &FolderId,
    parent: SharedString,
    out: &mut Vec<SourceNode>,
) {
    for sub in library.subfolders(folder, sort) {
        let key = folder_key(&sub.id);
        let open = expanded.contains(&sub.id);
        out.push(
            source_node(key.clone(), sub.id.name().to_owned())
                .parent(parent.clone())
                .icon(Icon::Folder)
                .has_children(library.count(&sub.id) > 0)
                .expanded(open)
                .actions([NEW_NOTE_HERE]),
        );
        add_level(library, sort, expanded, &sub.id, key, out);
    }
    for note in library.notes_in(folder, sort) {
        out.push(source_node(note_key(&note.id), note.title.clone()).parent(parent.clone()).icon(Icon::FileText));
    }
}

pub fn row_actions() -> [SourceAction; 1] {
    [source_action(NEW_NOTE_HERE, Icon::Plus).tooltip("New Note Here")]
}

/// The trash, kept at the foot of the sidebar.
pub fn trash_node(count: usize) -> SourceNode {
    let node = source_node(TRASH_KEY, "Trash").icon(Icon::Trash).movable(false);
    if count > 0 { node.badge(count.to_string()) } else { node }
}

impl super::root::FormalNotes {
    /// The library tree, and the trash at the foot.
    pub(super) fn render_sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        use crate::actions::{MoveEntry, NewFolderIn, NewNoteIn, RenameEntry, RevealEntry, TrashEntry, reveal_label};
        use crate::library::EntryId;
        use crate::nav::{ENTRY_DRAG, Location, parse_entry};

        let library = self.store.read(cx).library();
        let selected: SharedString = match (self.switching.as_ref().or(self.open.as_ref()), &self.location) {
            (Some(note), _) => note_key(note),
            (None, Location::Folder(folder)) => folder_key(folder),
            (None, Location::Trash) => TRASH_KEY.into(),
        };
        let tree = source_tree("sidebar")
            .nodes(nodes(library, Self::sort(cx), &self.expanded))
            .actions(row_actions())
            .selected(selected.clone())
            .draggable_rows(ENTRY_DRAG)
            .accepts_drops([ENTRY_DRAG])
            .drops_onto_rows()
            .flex_1()
            .on_select(cx.listener(|this, e: &SourceSelected, window, cx| match parse_entry(&e.id) {
                Some(EntryId::Folder(folder)) => {
                    this.expanded.insert(folder.clone());
                    this.go(Location::Folder(folder), cx);
                }
                Some(EntryId::Note(note)) => this.show_note(note, window, cx),
                None => {}
            }))
            .on_expand(cx.listener(|this, e: &SourceExpanded, _, cx| {
                if let Some(EntryId::Folder(f)) = parse_entry(&e.id) {
                    this.expanded.insert(f);
                    cx.notify();
                }
            }))
            .on_collapse(cx.listener(|this, e: &SourceCollapsed, _, cx| {
                if let Some(EntryId::Folder(f)) = parse_entry(&e.id) {
                    this.expanded.remove(&f);
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, e: &SourceActionClicked, window, cx| {
                if e.action == NEW_NOTE_HERE
                    && let Some(EntryId::Folder(f)) = parse_entry(&e.id)
                {
                    this.new_note_in(&f, window, cx);
                }
            }))
            .on_drop(cx.listener(|this, e: &Dropped, _, cx| {
                let Some(entry) = parse_entry(&e.item.id) else { return };
                // Onto a folder row, or between rows inside a folder; a note row stands for its
                // folder.
                let target = e.slot.as_ref().and_then(|slot| slot.on.clone().or_else(|| slot.parent.clone()));
                let folder = match target.as_deref().and_then(parse_entry) {
                    Some(EntryId::Folder(f)) => f,
                    Some(EntryId::Note(n)) => n.folder(),
                    None => FolderId::root(),
                };
                this.move_entry(&entry, &folder, cx);
            }))
            .context_menu(|id, menu, _, _| match parse_entry(id) {
                Some(EntryId::Folder(f)) if !f.is_root() => menu
                    .item("New Note Here", NewNoteIn(id.to_string()))
                    .item("New Folder Here", NewFolderIn(id.to_string()))
                    .separator()
                    .item("Rename…", RenameEntry(id.to_string()))
                    .item(reveal_label(), RevealEntry(id.to_string()))
                    .separator()
                    .item_destructive("Move to Trash", TrashEntry(id.to_string())),
                Some(EntryId::Note(n)) => {
                    let menu = menu
                        .item("Rename…", RenameEntry(id.to_string()))
                        .item(reveal_label(), RevealEntry(id.to_string()));
                    let menu = if n.folder().is_root() {
                        menu
                    } else {
                        menu.item("Move to Home", MoveEntry(id.to_string(), String::new()))
                    };
                    menu.separator().item_destructive("Move to Trash", TrashEntry(id.to_string()))
                }
                _ => menu.item("New Note", NewNoteIn(id.to_string())).item("New Folder", NewFolderIn(id.to_string())),
            });
        let trash = source_list("sidebar-trash")
            .nodes([trash_node(library.trash.len())])
            .selected_opt((selected == TRASH_KEY).then_some(selected.clone()))
            .flex_none()
            .h(px(36.))
            .on_select(cx.listener(|this, _: &SourceSelected, _, cx| this.go(Location::Trash, cx)));
        v_flex().size_full().child(tree).child(trash).into_any_element()
    }
}
