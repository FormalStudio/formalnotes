//! The page under the editor: Home or a folder as a grid of cards, or the trash.

use jiff::Timestamp;
use nd::prelude::*;

use super::cards::{self, FolderFace, GROUP, NoteFace};
use super::format;
use super::metrics::*;
use super::motion;
use super::root::FormalNotes;
use crate::actions::*;
use crate::library::{EntryId, FolderId, Library};
use crate::nav::{ENTRY_DRAG, Location, card_id, dragged_entry, entry_key, face_id, folder_key, frame_id};
use crate::settings::{NotesSettings, SortOrder};

impl FormalNotes {
    /// The page for the location, keyed by it: a new page rises in as the one it replaces fades
    /// out where it stood, so Home, folders and the trash cross over rather than cut. Under a
    /// closing note it only fades in, so the card the note goes back into holds still.
    pub(super) fn render_page(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let (key, page) = match self.location.clone() {
            Location::Folder(folder) => (format!("page:{}", folder_key(&folder)), self.folder_page(&folder, cx)),
            Location::Trash => ("page:trash".to_owned(), self.trash_page(cx)),
        };
        // The window opens with its page in place: only a page after the first comes in.
        let shown = self.region.is_some();
        let rise = if self.presented.is_closing() { 0. } else { motion::PAGE_RISE };
        div()
            .id(key)
            .size_full()
            .animate(motion::layout())
            .when(shown, |d| d.enter_from(move |p| p.opacity(0.).offset(px(0.), px(rise))))
            .exit_to(|p| p.opacity(0.))
            .child(page)
            .into_any_element()
    }

    fn folder_page(&self, folder: &FolderId, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let sort = Self::sort(cx);
        let now = Timestamp::now();
        let library = self.store.read(cx).library().clone();
        let folders = library.subfolders(folder, sort);
        let notes = library.notes_in(folder, sort);
        let beside_folders = !folders.is_empty();
        let mut tiles = Vec::with_capacity(folders.len() + notes.len());
        for f in &folders {
            let entry = EntryId::Folder(f.id.clone());
            let first = library.notes_in(&f.id, sort).first().map(|n| n.text.clone());
            let face = cards::folder_card(
                &theme,
                FolderFace {
                    name: f.id.name(),
                    items: library.count(&f.id),
                    first_page: first.as_deref(),
                    hovered: self.hovered.as_ref() == Some(&card_id(&entry)),
                    targeted: self.drop_target.as_ref() == Some(&card_id(&entry)),
                },
            );
            let menu_key = entry_key(&entry).to_string();
            let face = face.context_menu(move |menu, _, _| {
                menu.item("Open", OpenEntry(menu_key.clone()))
                    .item("New Note Here", NewNoteIn(menu_key.clone()))
                    .separator()
                    .item("Rename…", RenameEntry(menu_key.clone()))
                    .item(reveal_label(), RevealEntry(menu_key.clone()))
                    .separator()
                    .item_destructive("Move to Trash", TrashEntry(menu_key.clone()))
            });
            tiles.push(self.tile(entry, face, cx));
        }
        for n in &notes {
            let entry = EntryId::Note(n.id.clone());
            let face = cards::note_card(
                &theme,
                NoteFace {
                    frame: frame_id(&entry),
                    title: &n.title,
                    text: &n.text,
                    caption: format::relative(n.modified, now),
                    hovered: self.hovered.as_ref() == Some(&card_id(&entry)),
                    beside_folders,
                },
            );
            let menu_key = entry_key(&entry).to_string();
            let destinations = move_targets(&library, &n.id.folder());
            let face = face.context_menu(move |menu, _, _| {
                let move_to = Menu::new("Move To").items(
                    destinations
                        .iter()
                        .map(|(name, path)| MenuItem::action(name.clone(), MoveEntry(menu_key.clone(), path.clone()))),
                );
                menu.item("Open", OpenEntry(menu_key.clone()))
                    .item("Rename…", RenameEntry(menu_key.clone()))
                    .item(reveal_label(), RevealEntry(menu_key.clone()))
                    .submenu(move_to)
                    .separator()
                    .item_destructive("Move to Trash", TrashEntry(menu_key.clone()))
            });
            tiles.push(self.tile(entry, face, cx));
        }

        let count = folders.len() + notes.len();
        let empty = count == 0;
        let gap = if beside_folders { CARD_GAP_FOLDERS } else { CARD_GAP };
        let header = self.header(folder, count, cx);
        let body = if empty {
            empty_state(self.region.is_some())
                .child(
                    status_page(if folder.is_root() { "No notes yet" } else { "This folder is empty" })
                        .icon(Icon::Notebook)
                        .description(match shortcut(&NewNote) {
                            Some(keys) => format!("Press {keys} to write one."),
                            None => "Use the New Note button to write one.".to_owned(),
                        }),
                )
                .into_any_element()
        } else {
            div()
                .id("page-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(div().px(px(PAGE_PAD_X)).pt(px(PAGE_PAD_TOP)).pb(px(PAGE_PAD_X)).child(self.grid(
                    format!("cards:{}", folder_key(folder)),
                    tiles,
                    gap,
                    true,
                )))
                .into_any_element()
        };
        let _ = theme;
        v_flex().id("page").size_full().child(header).child(body).into_any_element()
    }

    /// A card in the grid: it lifts under the pointer and opens on a click. A folder card takes
    /// dragged entries: it grows a little and rings while one is over it, opens after the drag
    /// rests on it, and moves what is dropped on it into the folder.
    fn tile(&self, entry: EntryId, face: Div, cx: &mut Context<Self>) -> (SharedString, AnyElement) {
        let id = card_id(&entry);
        let hover_id = id.clone();
        // Under Reduce Motion the ring fades in on its own.
        let grow = self.drop_target.as_ref() == Some(&id) && !cx.reduce_motion();
        let face = match &entry {
            EntryId::Folder(folder) => {
                self.folder_drops(face, &id, folder, cx).scale(if grow { DROP_SCALE } else { 1. })
            }
            EntryId::Note(_) => face,
        };
        let face = face
            .id(face_id(&entry))
            .group(GROUP)
            .size_full()
            .active(|p| p.scale(0.98))
            .animate(motion::press())
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                let was = this.hovered.clone();
                if *hovered {
                    this.hovered = Some(hover_id.clone());
                } else if this.hovered.as_ref() == Some(&hover_id) {
                    this.hovered = None;
                }
                if was != this.hovered {
                    cx.notify();
                }
            }))
            .on_tap(cx.listener(move |this, _: &TapEvent, window, cx| this.open_entry(&entry, window, cx)));
        (id, face.into_any_element())
    }

    fn folder_drops(&self, face: Div, id: &SharedString, folder: &FolderId, cx: &mut Context<Self>) -> Div {
        let (over, dropped, sprung) = (id.clone(), folder.clone(), folder.clone());
        face.accepts_drops([ENTRY_DRAG])
            .on_drag_over(cx.listener(move |this, e: &DragOver, _, cx| {
                let target = (e.phase != DragOverPhase::Exited).then(|| over.clone());
                if target.is_none() && this.drop_target.as_ref() != Some(&over) {
                    return;
                }
                if this.drop_target != target {
                    this.drop_target = target;
                    cx.notify();
                }
            }))
            .on_drop(cx.listener(move |this, e: &Dropped, _, cx| {
                this.drop_target = None;
                if let Some(entry) = dragged_entry(&e.item.id) {
                    this.move_entry(&entry, &dropped, cx);
                }
                cx.notify();
            }))
            .on_spring_load(cx.listener(move |this, _: &SpringLoaded, _, cx| {
                this.drop_target = None;
                this.go(Location::Folder(sprung.clone()), cx);
            }))
    }

    /// The page's title (a path of crumbs inside folders), its count, and the sort menu.
    fn header(&self, folder: &FolderId, count: usize, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme().clone();
        let sort = cx.settings::<NotesSettings>().sort;
        let mut crumbs = h_flex().gap(px(6.)).items_center().min_w_0();
        if folder.is_root() {
            crumbs = crumbs.child(page_title(&theme, "Home"));
        } else {
            let mut trail = vec![FolderId::root()];
            trail.extend(folder.ancestry());
            let last = trail.len() - 1;
            for (i, f) in trail.into_iter().enumerate() {
                let name = if f.is_root() { "Home".to_owned() } else { f.name().to_owned() };
                if i == last {
                    crumbs = crumbs.child(page_title(&theme, &name));
                } else {
                    crumbs = crumbs
                        .child(
                            div()
                                .id(format!("crumb:{}", folder_key(&f)))
                                .on_tap(cx.listener(move |this, _: &TapEvent, _, cx| {
                                    this.go(Location::Folder(f.clone()), cx)
                                }))
                                .child(text(name).text_size(theme.type_scale.title).text_color(theme.text.secondary)),
                        )
                        .child(icon(Icon::ChevronRight, 12.).text_color(theme.text.tertiary));
                }
            }
        }
        let mut sort_button = menu_button("sort", Icon::ChevronDown, move |menu, _, _| {
            SortOrder::ALL.into_iter().fold(menu, |menu, order| {
                menu.entry(MenuItem::action(order.label(), SetSort(order)).checked(order == sort))
            })
        })
        .tooltip("Sort By");
        sort_button.props_mut().label = sort.label().into();
        h_flex()
            .flex_none()
            .h(px(HEADER_HEIGHT))
            .px(px(PAGE_PAD_X + 2.))
            .gap(theme.spacing.md)
            .items_center()
            .child(crumbs)
            .child(text(format::items(count)).id("page-count").text_color(theme.text.secondary))
            .child(div().flex_1())
            .child(sort_button)
    }

    fn trash_page(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let now = Timestamp::now();
        let items = self.store.read(cx).library().trash.clone();
        let tiles: Vec<(SharedString, AnyElement)> = items
            .iter()
            .map(|item| {
                let caption = cards::trash_caption(&item.origin, item.trashed, now);
                let face = match &item.kind {
                    crate::library::TrashKind::Note { text } => cards::note_card(
                        &theme,
                        NoteFace {
                            frame: format!("frame:trash:{}", item.name).into(),
                            title: &item.title,
                            text,
                            caption,
                            hovered: false,
                            beside_folders: false,
                        },
                    ),
                    crate::library::TrashKind::Folder { items } => cards::folder_card(
                        &theme,
                        FolderFace {
                            name: &item.title,
                            items: *items,
                            first_page: None,
                            hovered: false,
                            targeted: false,
                        },
                    ),
                };
                let name = item.name.clone();
                let face = face
                    .id(format!("trashed:{}", item.name))
                    .size_full()
                    .context_menu(move |menu, _, _| menu.item("Put Back", RestoreTrashed(name.clone())));
                (SharedString::from(format!("trash:{}", item.name)), face.into_any_element())
            })
            .collect();
        let header = h_flex()
            .flex_none()
            .h(px(HEADER_HEIGHT))
            .px(px(PAGE_PAD_X + 2.))
            .gap(theme.spacing.md)
            .items_center()
            .child(page_title(&theme, "Trash"))
            .child(text(format::items(items.len())).id("page-count").text_color(theme.text.secondary))
            .child(div().flex_1())
            .when(!items.is_empty(), |h| h.child(button("empty-trash", "Empty Trash").dispatch(EmptyTrash)));
        let body = if items.is_empty() {
            empty_state(self.region.is_some())
                .child(
                    status_page("Trash is empty")
                        .icon(Icon::Trash)
                        .description("Notes and folders you delete wait here until you empty it."),
                )
                .into_any_element()
        } else {
            div()
                .id("trash-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(div().px(px(PAGE_PAD_X)).pt(px(PAGE_PAD_TOP)).pb(px(PAGE_PAD_X)).child(self.grid(
                    "cards:trash",
                    tiles,
                    CARD_GAP,
                    false,
                )))
                .into_any_element()
        };
        v_flex().id("page").size_full().child(header).child(body).into_any_element()
    }
}

/// Where a page's empty state goes: it fades in when the last card leaves. Not while the window
/// opens, which shows its first page as it is.
fn empty_state(shown: bool) -> Div {
    div().id("empty").flex_1().animate(Animation::normal()).when(shown, |d| d.enter_from(|p| p.opacity(0.)))
}

fn page_title(theme: &Theme, title: &str) -> Text {
    text(title.to_owned())
        .text_size(theme.type_scale.title)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme.text.primary)
        .truncate()
}

/// The folders a note can move to, by display name and path: Home and every folder but its own.
fn move_targets(library: &Library, from: &FolderId) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if !from.is_root() {
        out.push(("Home".to_owned(), String::new()));
    }
    out.extend(library.folders.keys().filter(|f| *f != from).map(|f| (f.0.replace('/', " / "), f.0.clone())));
    out
}
