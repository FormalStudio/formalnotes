//! The window: the sidebar, the page of cards, and the note presented over it. This file holds
//! the window's state and what the commands do to it; `page.rs` and `note.rs` render the two
//! layers, `sidebar.rs` the sidebar's rows.

use std::collections::HashSet;
use std::path::PathBuf;

use nd::prelude::*;
use nd::theme::{Appearance, ThemeOverride};

use super::editor::{BODY, Editor};
use super::notices::{Notices, Undo};
use super::palette::{self, Command, Mode};
use crate::actions::*;
use crate::library::{EntryId, FolderId, NoteId, Sort, Store};
use crate::nav::{Location, parse_entry};
use crate::settings::{NotesSettings, SortOrder, ThemeChoice};

const RENAME_FIELD: &str = "rename-field";

pub struct FormalNotes {
    pub(super) store: Entity<Store>,
    pub(super) editor: Entity<Editor>,
    pub(super) location: Location,
    /// The note presented over the page.
    pub(super) open: Option<NoteId>,
    /// The note the presented page shows: kept through the close, so the page fades out as it
    /// was instead of losing its path first.
    pub(super) shown: Option<NoteId>,
    pub(super) presented: Presentation,
    /// The note opening in place of the one in the editor: its text fades out first, and
    /// this one comes in once it has.
    pub(super) switching: Option<NoteId>,
    pub(super) expanded: HashSet<FolderId>,
    /// The card under the pointer, which lifts.
    pub(super) hovered: Option<SharedString>,
    /// The folder card a dragged entry is over, which opens to take it.
    pub(super) drop_target: Option<SharedString>,
    /// The content region's size: the presented note grows to it, and the card grid takes its
    /// columns from its width.
    pub(super) region: Option<(f32, f32)>,
    pub(super) sidebar_width: f32,
    palette: Option<Mode>,
    palette_query: SharedString,
    pub(super) notices: Notices,
    rename: Option<(EntryId, Entity<TextFieldState>)>,
    seen_failures: usize,
    /// A demo library ignores the library setting.
    fixed_root: bool,
    theme: Option<ThemeChoice>,
    pub(super) ask: Entity<super::ask::Ask>,
    pub(super) ask_open: bool,
    pub(super) ask_width: f32,
    pub(super) automations: Entity<crate::ai::Automations>,
}

impl FormalNotes {
    /// The window over the library at `root`; `fixed_root` keeps it there whatever the settings
    /// say (the demo library).
    pub fn new(root: PathBuf, fixed_root: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let store = cx.new(|cx| Store::new(root, cx));
        let this = cx.weak_entity();
        let editor = cx.new(|cx| {
            Editor::new(
                store.clone(),
                move |moved, _window, cx| {
                    let _ = this.update(cx, |root, cx| {
                        root.open = Some(moved.clone());
                        root.shown = Some(moved.clone());
                        cx.notify();
                    });
                },
                window,
                cx,
            )
        });
        cx.observe(&store, |this, store, cx| this.store_changed(store, cx)).detach();
        let (ask, automations) = Self::ai_parts(&store, cx);
        Self {
            store,
            editor,
            location: Location::home(),
            open: None,
            shown: None,
            presented: Presentation::new(),
            switching: None,
            expanded: HashSet::new(),
            hovered: None,
            drop_target: None,
            region: None,
            sidebar_width: super::metrics::SIDEBAR_WIDTH,
            palette: None,
            palette_query: SharedString::default(),
            notices: Notices::default(),
            rename: None,
            seen_failures: 0,
            fixed_root,
            theme: None,
            ask,
            ask_open: false,
            ask_width: super::ai::ASK_WIDTH,
            automations,
        }
    }

    pub fn store(&self) -> &Entity<Store> {
        &self.store
    }

    pub fn editor(&self) -> &Entity<Editor> {
        &self.editor
    }

    pub fn location(&self) -> &Location {
        &self.location
    }

    pub fn open_note(&self) -> Option<&NoteId> {
        self.open.as_ref()
    }

    /// The content region's size, once measured.
    pub fn region(&self) -> Option<(f32, f32)> {
        self.region
    }

    pub fn palette_open(&self) -> bool {
        self.palette.is_some()
    }

    pub fn toasts(&self) -> &[Toast] {
        self.notices.toasts()
    }

    /// The rows the palette shows now.
    pub fn palette_rows(&self, cx: &App) -> Vec<PaletteItem> {
        let mode = self.palette.unwrap_or(Mode::Commands);
        palette::items(self.store.read(cx).library(), mode, &self.palette_query, self.open.is_some())
    }

    pub(super) fn sort(cx: &App) -> Sort {
        cx.settings::<NotesSettings>().sort.into()
    }

    fn store_changed(&mut self, store: Entity<Store>, cx: &mut Context<Self>) {
        let failures = store.read(cx).failures();
        let fresh: Vec<String> = failures.get(self.seen_failures..).unwrap_or_default().to_vec();
        self.seen_failures = failures.len();
        for failure in fresh {
            self.notices.show(format!("Could not save: {failure}"), None);
        }
        // Something removed from disk takes the page or the note with it.
        let library = store.read(cx).library().clone();
        if let Some(id) = &self.open
            && library.note(id).is_none()
            && !library.notes.is_empty()
        {
            self.close_note(cx);
        }
        if let Location::Folder(folder) = &self.location
            && !library.has_folder(folder)
            && !library.folders.is_empty()
        {
            self.location = Location::home();
        }
        cx.notify();
    }

    // Navigation.

    pub fn open_entry(&mut self, entry: &EntryId, window: &mut Window, cx: &mut Context<Self>) {
        match entry {
            EntryId::Folder(folder) => self.go(Location::Folder(folder.clone()), cx),
            EntryId::Note(note) => self.show_note(note.clone(), window, cx),
        }
    }

    /// Shows a page; a note open over it closes.
    pub fn go(&mut self, location: Location, cx: &mut Context<Self>) {
        if let Location::Folder(folder) = &location {
            self.reveal_in_sidebar(folder);
        }
        self.close_note(cx);
        self.location = location;
        self.hovered = None;
        cx.notify();
    }

    /// Presents a note over its folder's page, growing out of its card. Over another open note
    /// it crossfades: the open one's text fades out, then this one's fades in.
    pub fn show_note(&mut self, id: NoteId, window: &mut Window, cx: &mut Context<Self>) {
        if self.store.read(cx).note(&id).is_none() {
            return;
        }
        let folder = id.folder();
        self.reveal_in_sidebar(&folder);
        if self.presented.is_open() && self.open.as_ref().is_some_and(|open| *open != id) {
            self.switching = Some(id);
            cx.notify();
            return;
        }
        self.present(id, window, cx);
    }

    fn present(&mut self, id: NoteId, window: &mut Window, cx: &mut Context<Self>) {
        self.switching = None;
        let folder = id.folder();
        // Over its own folder, so the card it grows from (and closes back onto) is on screen.
        if self.location != Location::Folder(folder.clone()) && !self.presented.is_open() {
            self.location = Location::Folder(folder);
        }
        self.editor.update(cx, |editor, cx| editor.open(id.clone(), window, cx));
        self.shown = Some(id.clone());
        self.open = Some(id);
        self.presented.set_open(true);
        cx.notify();
    }

    /// The open note's text has faded out: the note waiting to switch in takes the editor.
    pub(super) fn finish_switch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.switching.take()
            && self.store.read(cx).note(&id).is_some()
        {
            self.present(id, window, cx);
        }
    }

    pub fn close_note(&mut self, cx: &mut Context<Self>) {
        self.switching = None;
        if self.open.take().is_some() {
            self.presented.set_open(false);
            self.editor.update(cx, |editor, cx| editor.close(cx));
            cx.notify();
        }
    }

    pub fn back(&mut self, cx: &mut Context<Self>) {
        if self.open.is_some() {
            self.close_note(cx);
            return;
        }
        let up = match &self.location {
            Location::Folder(folder) => folder.parent().map(Location::Folder),
            Location::Trash => Some(Location::home()),
        };
        if let Some(up) = up {
            self.go(up, cx);
        }
    }

    fn reveal_in_sidebar(&mut self, folder: &FolderId) {
        self.expanded.extend(folder.ancestry());
    }

    /// Where a new note or folder goes: the open note's folder, else the page's.
    fn current_folder(&self) -> FolderId {
        match (&self.open, &self.location) {
            (Some(note), _) => note.folder(),
            (None, Location::Folder(folder)) => folder.clone(),
            (None, Location::Trash) => FolderId::root(),
        }
    }

    // Library commands.

    pub fn new_note_in(&mut self, folder: &FolderId, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.store.update(cx, |store, cx| store.create_note(folder, cx));
        self.location = Location::Folder(id.folder());
        self.show_note(id, window, cx);
        window.focus_id(cx, BODY);
    }

    pub fn new_folder_in(&mut self, parent: &FolderId, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.store.update(cx, |store, cx| store.create_folder(parent, cx));
        self.go(Location::Folder(parent.clone()), cx);
        self.start_rename(EntryId::Folder(id), window, cx);
    }

    pub fn trash(&mut self, entry: &EntryId, cx: &mut Context<Self>) {
        let title = self.title_of(entry, cx);
        let Some(name) = self.store.update(cx, |store, cx| store.trash(entry, cx)) else { return };
        let path = entry.path().to_owned();
        if self.open.as_ref().is_some_and(|n| crate::library::model::rebase(&n.0, &path, "").is_some()) {
            self.close_note(cx);
        }
        if let Location::Folder(folder) = &self.location
            && let EntryId::Folder(gone) = entry
            && gone.contains(&folder.0)
        {
            self.location = Location::Folder(gone.parent().unwrap_or_default());
        }
        self.notices.show(format!("Moved “{title}” to Trash"), Some(Undo::Restore(name)));
        cx.notify();
    }

    pub fn restore(&mut self, name: &str, cx: &mut Context<Self>) {
        let title = self.store.read(cx).library().trash.iter().find(|t| t.name == name).map(|t| t.title.clone());
        let Some(target) = self.store.update(cx, |store, cx| store.restore(name, cx)) else { return };
        self.notices.show(format!("Put back “{}”", title.unwrap_or_default()), Some(Undo::Trash(target)));
        cx.notify();
    }

    pub fn move_entry(&mut self, entry: &EntryId, folder: &FolderId, cx: &mut Context<Self>) {
        let title = self.title_of(entry, cx);
        let from = match entry {
            EntryId::Note(n) => n.folder(),
            EntryId::Folder(f) => f.parent().unwrap_or_default(),
        };
        let moved = self.store.update(cx, |store, cx| store.move_into(entry, folder, cx));
        if moved == *entry {
            return;
        }
        self.follow(entry, &moved, cx);
        self.reveal_in_sidebar(folder);
        let to = if folder.is_root() { "Home" } else { folder.name() };
        self.notices.show(format!("Moved “{title}” to {to}"), Some(Undo::Move(moved, from)));
        cx.notify();
    }

    /// Keeps the page, the open note and open branches on an entry that moved.
    fn follow(&mut self, from: &EntryId, to: &EntryId, cx: &mut Context<Self>) {
        use crate::library::model::rebase;
        let (from, to) = (from.path().to_owned(), to.path().to_owned());
        if from == to {
            return;
        }
        if let Some(note) = &self.open
            && let Some(path) = rebase(&note.0, &from, &to)
        {
            let moved = NoteId(path);
            self.open = Some(moved.clone());
            self.shown = Some(moved.clone());
            self.editor.update(cx, |editor, cx| editor.follow(moved, cx));
        }
        if let Location::Folder(folder) = &self.location
            && !folder.is_root()
            && let Some(path) = rebase(&folder.0, &from, &to)
        {
            self.location = Location::Folder(FolderId(path));
        }
        self.expanded = self.expanded.drain().map(|f| rebase(&f.0, &from, &to).map_or(f, FolderId)).collect();
        cx.notify();
    }

    fn title_of(&self, entry: &EntryId, cx: &App) -> String {
        match entry {
            EntryId::Note(id) => self.store.read(cx).note(id).map_or_else(|| id.stem().to_owned(), |n| n.title.clone()),
            EntryId::Folder(id) => id.name().to_owned(),
        }
    }

    fn reveal(&self, entry: &EntryId, cx: &App) {
        let root = self.store.read(cx).root().to_owned();
        let path = if entry.path().is_empty() { root } else { root.join(entry.path()) };
        let _ = nd::system::reveal_path(&path);
    }

    fn start_rename(&mut self, entry: EntryId, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.title_of(&entry, cx);
        let state = cx.new(|cx| TextFieldState::new(window, cx));
        state.update(cx, |s, cx| s.set_text(current, window, cx));
        self.rename = Some((entry, state));
        cx.notify();
        window.focus_id(cx, RENAME_FIELD);
    }

    fn commit_rename(&mut self, cx: &mut Context<Self>) {
        let Some((entry, state)) = self.rename.take() else { return };
        let name = state.read(cx).text().to_string();
        let moved = match &entry {
            EntryId::Note(id) => EntryId::Note(self.store.update(cx, |s, cx| s.retitle(id, &name, cx))),
            EntryId::Folder(id) => EntryId::Folder(self.store.update(cx, |s, cx| s.rename_folder(id, &name, cx))),
        };
        self.follow(&entry, &moved, cx);
        cx.notify();
    }

    fn empty_trash(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.store.read(cx).library().trash.len();
        if count == 0 {
            return;
        }
        let this = cx.weak_entity();
        let alert = Alert::confirm("Empty the Trash?", "Empty Trash")
            .message(format!("{} will be deleted for good.", super::format::items(count)));
        window.alert(cx, alert, move |answer, cx| {
            if answer.0 == 0 {
                let _ = this.update(cx, |this, cx| {
                    this.store.update(cx, |store, cx| store.empty_trash(cx));
                    this.notices.show("Trash emptied", None);
                    cx.notify();
                });
            }
        });
    }

    fn set_sort(order: SortOrder, cx: &mut App) {
        crate::settings::set(cx, "sort", &order);
    }

    /// Follows the settings: the library folder and the theme.
    fn apply_settings(&mut self, cx: &mut Context<Self>) {
        let settings = cx.settings::<NotesSettings>();
        if !self.fixed_root {
            let root = settings.root();
            if self.store.read(cx).root() != root {
                self.close_note(cx);
                self.location = Location::home();
                self.store.update(cx, |store, cx| store.open(root, cx));
            }
        }
        if self.theme != Some(settings.theme) {
            self.theme = Some(settings.theme);
            let appearance = match settings.theme {
                ThemeChoice::System => Appearance::System,
                ThemeChoice::Light => Appearance::Light,
                ThemeChoice::Dark => Appearance::Dark,
            };
            nd::theme::set_app_theme(cx, ThemeOverride::new().appearance(appearance));
        }
    }

    // Palette.

    fn open_palette(&mut self, mode: Mode, cx: &mut Context<Self>) {
        self.palette = Some(mode);
        self.palette_query = SharedString::default();
        cx.notify();
    }

    fn run(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        match command {
            Command::Open(entry) => self.open_entry(&entry, window, cx),
            Command::NewNote => self.new_note_in(&self.current_folder(), window, cx),
            Command::NewFolder => self.new_folder_in(&self.current_folder(), window, cx),
            Command::Home => self.go(Location::home(), cx),
            Command::Search => self.open_palette(Mode::Search, cx),
            Command::Trash => self.go(Location::Trash, cx),
            Command::MoveToTrash => {
                if let Some(id) = self.open.clone() {
                    self.trash(&EntryId::Note(id), cx);
                }
            }
            Command::ToggleSidebar => cx.window_command(window.id(), nd::backend::WindowCommand::ToggleSidebar),
            Command::Settings => self.open_settings(cx),
            Command::Ask => self.toggle_ask(window, cx),
            Command::Organize => self.organize_open_note(cx),
        }
    }

    /// The Settings window, as a Mac app keeps its preferences: one window, brought forward
    /// when it is already open.
    fn open_settings(&mut self, cx: &mut Context<Self>) {
        crate::settings::open_window(cx);
    }

    /// What the window is showing, for its title: the note, the folder, Home or the trash.
    fn window_title(&self, cx: &App) -> String {
        if let Some(note) = self.open.as_ref().and_then(|id| self.store.read(cx).note(id)) {
            return note.title.clone();
        }
        match &self.location {
            Location::Folder(folder) if folder.is_root() => "Home".to_owned(),
            Location::Folder(folder) => folder.name().to_owned(),
            Location::Trash => "Trash".to_owned(),
        }
    }

    /// Every action the window answers, on the scaffold that holds them.
    fn actions(&self, scaffold: Scaffold, cx: &mut Context<Self>) -> Scaffold {
        let entry = |key: &str| parse_entry(key);
        scaffold
            .key_context(CONTEXT)
            .on_action(cx.listener(|this, _: &NewNote, window, cx| this.run(Command::NewNote, window, cx)))
            .on_action(cx.listener(|this, _: &NewFolder, window, cx| this.run(Command::NewFolder, window, cx)))
            .on_action(cx.listener(|this, _: &OpenPalette, _, cx| this.open_palette(Mode::Commands, cx)))
            .on_action(cx.listener(|this, _: &AskNotes, window, cx| this.toggle_ask(window, cx)))
            .on_action(cx.listener(|this, _: &OrganizeNote, _, cx| this.organize_open_note(cx)))
            .on_action(cx.listener(|this, _: &Search, _, cx| this.open_palette(Mode::Search, cx)))
            .on_action(cx.listener(|this, _: &GoHome, _, cx| this.go(Location::home(), cx)))
            .on_action(cx.listener(|this, _: &GoBack, _, cx| this.back(cx)))
            .on_action(cx.listener(|this, _: &OpenTrash, _, cx| this.go(Location::Trash, cx)))
            .on_action(cx.listener(|this, _: &EmptyTrash, window, cx| this.empty_trash(window, cx)))
            .on_action(cx.listener(|this, _: &MoveToTrash, window, cx| this.run(Command::MoveToTrash, window, cx)))
            .on_action(cx.listener(|this, _: &OpenSettings, _, cx| this.open_settings(cx)))
            .on_action(cx.listener(|_, _: &ReportIssue, _, _| report_issue()))
            .on_action(cx.listener(move |this, a: &OpenEntry, window, cx| {
                if let Some(e) = entry(&a.0) {
                    this.open_entry(&e, window, cx);
                }
            }))
            .on_action(cx.listener(move |this, a: &RenameEntry, window, cx| {
                if let Some(e) = entry(&a.0) {
                    this.start_rename(e, window, cx);
                }
            }))
            .on_action(cx.listener(move |this, a: &RevealEntry, _, cx| {
                if let Some(e) = entry(&a.0) {
                    this.reveal(&e, cx);
                }
            }))
            .on_action(cx.listener(move |this, a: &TrashEntry, _, cx| {
                if let Some(e) = entry(&a.0) {
                    this.trash(&e, cx);
                }
            }))
            .on_action(cx.listener(move |this, a: &NewNoteIn, window, cx| {
                if let Some(EntryId::Folder(f)) = entry(&a.0) {
                    this.new_note_in(&f, window, cx);
                }
            }))
            .on_action(cx.listener(move |this, a: &NewFolderIn, window, cx| {
                if let Some(EntryId::Folder(f)) = entry(&a.0) {
                    this.new_folder_in(&f, window, cx);
                }
            }))
            .on_action(cx.listener(|this, a: &RestoreTrashed, _, cx| this.restore(&a.0, cx)))
            .on_action(cx.listener(move |this, a: &MoveEntry, _, cx| {
                if let Some(e) = entry(&a.0) {
                    this.move_entry(&e, &FolderId(a.1.clone()), cx);
                }
            }))
            .on_action(cx.listener(|_, a: &SetSort, _, cx| Self::set_sort(a.0, cx)))
    }

    fn render_palette(&self, cx: &mut Context<Self>) -> CommandPalette {
        let placeholder = match self.palette {
            Some(Mode::Search) => "Search notes",
            _ => "Search notes, folders and commands",
        };
        command_palette("palette")
            .open(self.palette.is_some())
            .placeholder(placeholder)
            .query(self.palette_query.clone())
            .items(self.palette_rows(cx))
            .on_query_change(cx.listener(|this, e: &PaletteQueryChanged, _, cx| {
                this.palette_query = e.text.clone();
                cx.notify();
            }))
            .on_activate(cx.listener(|this, e: &PaletteActivated, window, cx| {
                this.palette = None;
                if let Some(command) = Command::parse(&e.id) {
                    this.run(command, window, cx);
                }
                cx.notify();
            }))
            .on_submit(cx.listener(|this, _: &PaletteSubmitted, _, cx| {
                this.palette = None;
                cx.notify();
            }))
            .on_cancel(cx.listener(|this, _: &PaletteCancelled, _, cx| {
                this.palette = None;
                cx.notify();
            }))
    }

    fn render_rename(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (entry, state) = self.rename.as_ref()?;
        let theme = cx.theme().clone();
        let name_len = state.read(cx).text().chars().count();
        let what = match entry {
            EntryId::Note(_) => "Rename Note",
            EntryId::Folder(_) => "Rename Folder",
        };
        Some(
            panel("rename")
                .w(px(360.))
                .p(theme.spacing.lg)
                .gap(theme.spacing.md)
                .child(text(what).font_weight(FontWeight::SEMIBOLD).text_color(theme.text.primary))
                .child(
                    text_field(RENAME_FIELD)
                        .bind(state)
                        .label("Name")
                        .select_on_focus(0..name_len)
                        .on_submit(cx.listener(|this, _: &TextSubmitted, _, cx| this.commit_rename(cx))),
                )
                .child(
                    h_flex()
                        .justify_end()
                        .gap(theme.spacing.sm)
                        .child(button("rename-cancel", "Cancel").on_click(cx.listener(
                            |this, _: &ClickEvent, _, cx| {
                                this.rename = None;
                                cx.notify();
                            },
                        )))
                        .child(
                            button("rename-save", "Rename")
                                .primary()
                                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.commit_rename(cx))),
                        ),
                )
                .on_dismiss(cx.listener(|this, _: &Dismissed, _, cx| {
                    this.rename = None;
                    cx.notify();
                }))
                .into_any_element(),
        )
    }
}

impl Render for FormalNotes {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.apply_settings(cx);
        let theme = cx.theme().clone();
        let page = self.render_page(window, cx);
        let note = self.render_note(cx);
        let sidebar = self.render_sidebar(cx);
        let trash = self.render_trash(cx);
        let palette = self.render_palette(cx);
        let rename = self.render_rename(cx);
        let open = self.open.clone();
        self.automations.update(cx, |a, _| a.set_open(open));
        let toasts = toast_overlay("toasts")
            .toasts(self.notices.toasts().to_vec())
            .on_dismiss(cx.listener(|this, e: &ToastDismissed, window, cx| {
                if let Some(entry) = this.notices.dismissed(&e.id, e.reason == DismissReason::User) {
                    this.open_entry(&entry, window, cx);
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, e: &ToastActionClicked, _, cx| match this.notices.take_undo(&e.id) {
                Some(Undo::Restore(name)) => this.restore(&name, cx),
                Some(Undo::Trash(entry)) => this.trash(&entry, cx),
                Some(Undo::Move(entry, back)) => this.move_entry(&entry, &back, cx),
                Some(Undo::Organized(applied)) => this.undo_organized(&applied, cx),
                None => {}
            }));
        let content = div()
            .size_full()
            .relative()
            .bg(theme.surface.raised)
            .on_resize({
                let this = cx.weak_entity();
                move |e, _, cx| {
                    let _ = this.update(cx, |this, cx| {
                        if this.region != Some((e.width, e.height)) {
                            this.region = Some((e.width, e.height));
                            cx.notify();
                        }
                    });
                }
            })
            .child(presentation("note", &self.presented).base(page).content(note))
            .child(toasts)
            .children(rename);
        // The page and the note carry their own headings, so the toolbar shows no title; the
        // window still takes one for the Window menu, Mission Control and VoiceOver.
        let scaffold = scaffold()
            .toolbar(
                toolbar()
                    .title(self.window_title(cx))
                    .hide_title()
                    .sidebar_toggle()
                    .flexible_space()
                    .item(icon_button("new-folder", Icon::FolderPlus).tooltip("New Folder").dispatch(NewFolder))
                    .item(icon_button("new-note", Icon::SquarePen).tooltip("New Note").dispatch(NewNote))
                    .item(icon_button("search", Icon::Search).tooltip("Search Notes").dispatch(Search))
                    .item(icon_button("ask", Icon::Lightbulb).tooltip("Ask Your Notes").dispatch(AskNotes)),
            )
            .sidebar(
                nd::components::sidebar()
                    .resizable()
                    .width(px(self.sidebar_width))
                    .min_w(px(super::metrics::SIDEBAR_MIN))
                    .max_w(px(super::metrics::SIDEBAR_MAX))
                    .on_resize(cx.listener(|this, e: &nd::components::PaneResized, _, cx| {
                        this.sidebar_width = e.width;
                        cx.notify();
                    }))
                    .child(sidebar)
                    .footer(trash),
            )
            .content(content)
            .content_min_width(super::metrics::CONTENT_MIN)
            .when_some(self.render_ask(cx), |scaffold, ask| scaffold.inspector(ask))
            .overlay(palette);
        self.actions(scaffold, cx)
    }
}
