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
use crate::nav::{Location, card_id, entry_key, folder_key};
use crate::settings::{NotesSettings, SortOrder};

impl FormalNotes {
    pub(super) fn render_page(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        match self.location.clone() {
            Location::Folder(folder) => self.folder_page(&folder, cx),
            Location::Trash => self.trash_page(cx),
        }
    }

    fn folder_page(&self, folder: &FolderId, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let sort = Self::sort(cx);
        let reduce = cx.reduce_motion();
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
                },
            );
            let menu_key = entry_key(&entry).to_string();
            let face = face.context_menu(move |menu, _, _| {
                menu.item("Open", OpenEntry(menu_key.clone()))
                    .item("New Note Here", NewNoteIn(menu_key.clone()))
                    .separator()
                    .item("Rename…", RenameEntry(menu_key.clone()))
                    .item("Reveal in Finder", RevealEntry(menu_key.clone()))
                    .separator()
                    .item_destructive("Move to Trash", TrashEntry(menu_key.clone()))
            });
            tiles.push(self.tile(entry, face, tiles.len(), reduce, cx));
        }
        for n in &notes {
            let entry = EntryId::Note(n.id.clone());
            let face = cards::note_card(
                &theme,
                NoteFace {
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
                    .item("Reveal in Finder", RevealEntry(menu_key.clone()))
                    .submenu(move_to)
                    .separator()
                    .item_destructive("Move to Trash", TrashEntry(menu_key.clone()))
            });
            tiles.push(self.tile(entry, face, tiles.len(), reduce, cx));
        }

        let count = folders.len() + notes.len();
        let empty = count == 0;
        let gap = if beside_folders { CARD_GAP_FOLDERS } else { CARD_GAP };
        let header = self.header(folder, count, cx);
        let body = if empty {
            div()
                .flex_1()
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
                .child(
                    div().px(px(PAGE_PAD_X)).pt(px(PAGE_PAD_TOP)).pb(px(PAGE_PAD_X)).child(self.grid(tiles, gap, cx)),
                )
                .into_any_element()
        };
        let _ = theme;
        v_flex().id("page").size_full().child(header).child(body).into_any_element()
    }

    /// A card in the grid: it fades up the first time the page shows, lifts under the pointer,
    /// and opens on a click.
    fn tile(
        &self,
        entry: EntryId,
        face: Div,
        index: usize,
        reduce: bool,
        cx: &mut Context<Self>,
    ) -> (SharedString, AnyElement) {
        let id = card_id(&entry);
        let hover_id = id.clone();
        let face = face
            .id(format!("face:{}", entry_key(&entry)))
            .group(GROUP)
            .size_full()
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
        let content = div()
            .size_full()
            .opacity(1.)
            .animate(motion::card_enter(index, reduce))
            .enter_from(|p| p.opacity(0.).offset(px(0.), px(motion::ENTER_RISE)))
            .child(face);
        (id, content.into_any_element())
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
        let reduce = cx.reduce_motion();
        let now = Timestamp::now();
        let items = self.store.read(cx).library().trash.clone();
        let tiles: Vec<(SharedString, AnyElement)> = items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let caption = cards::trash_caption(&item.origin, item.trashed, now);
                let face = match &item.kind {
                    crate::library::TrashKind::Note { text } => cards::note_card(
                        &theme,
                        NoteFace { title: &item.title, text, caption, hovered: false, beside_folders: false },
                    ),
                    crate::library::TrashKind::Folder { items } => cards::folder_card(
                        &theme,
                        FolderFace { name: &item.title, items: *items, first_page: None, hovered: false },
                    ),
                };
                let name = item.name.clone();
                let face = face
                    .id(format!("trashed:{}", item.name))
                    .size_full()
                    .context_menu(move |menu, _, _| menu.item("Put Back", RestoreTrashed(name.clone())));
                let content = div()
                    .size_full()
                    .animate(motion::card_enter(i, reduce))
                    .enter_from(|p| p.opacity(0.).offset(px(0.), px(motion::ENTER_RISE)))
                    .child(face);
                (SharedString::from(format!("trash:{}", item.name)), content.into_any_element())
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
            div()
                .flex_1()
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
                .child(
                    div()
                        .px(px(PAGE_PAD_X))
                        .pt(px(PAGE_PAD_TOP))
                        .pb(px(PAGE_PAD_X))
                        .child(self.grid(tiles, CARD_GAP, cx)),
                )
                .into_any_element()
        };
        v_flex().id("page").size_full().child(header).child(body).into_any_element()
    }
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
