//! A folder shown as a table: one row per note, the title first, then the folder's property
//! columns, then when it was modified. Cells edit in place by type; a header click sorts, the
//! trailing `+` adds a column, and the row at the bottom adds a note. Subfolders stay reachable
//! from the header. The window presents a note from its row the way it does from a card.

use std::cmp::Ordering;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use jiff::Timestamp;
use nd::prelude::*;

use super::fields::{self, Field, FieldEdit, FieldHost, Target};
use super::format;
use super::metrics::{HEADER_HEIGHT, PAGE_PAD_X};
use super::page::{crumbs, move_targets, note_menu};
use crate::actions::{AddColumn, ColumnAction, ColumnOp, NewNoteIn, OpenEntry};
use crate::library::properties::{self, PropType, Raw, Value};
use crate::library::view::{self, Column as PropColumn, FolderView, SortBy};
use crate::library::{EntryId, FolderId, Note, NoteId, Store};
use crate::nav::{ENTRY_DRAG, folder_key, frame_id, note_key};
use crate::settings::NotesSettings;

pub const TABLE: &str = "table";
pub const FILTER: &str = "table-filter";
pub const ADD_ROW: &str = "table-add-row";
const ADD_COLUMN: &str = "+add";
const COLUMN_NAME: &str = "column-name";
const ROW_HEIGHT: f32 = 32.;
const TITLE_WIDTH: f32 = 210.;
const MODIFIED_WIDTH: f32 = 110.;
const PLUS_WIDTH: f32 = 36.;

/// A row of the table: a note, or the row that adds one.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Row {
    Note(NoteId),
    Add,
}

#[derive(Clone, Debug, PartialEq)]
enum Col {
    Title,
    Prop(PropColumn),
    Modified,
    Add,
}

/// The panel that names a new column, or renames one.
struct ColumnPanel {
    renaming: Option<String>,
    name: Entity<TextFieldState>,
    kind: usize,
    taken: bool,
}

pub struct TableView {
    store: Entity<Store>,
    folder: FolderId,
    list: ListState<Row>,
    filter: Entity<TextFieldState>,
    edit: FieldEdit,
    cols: Vec<Col>,
    /// The built-in columns' widths, which `.folder.json` does not keep.
    title_width: f32,
    modified_width: f32,
    /// Folded into every row's version: the cells take their widths from the columns.
    layout: u64,
    panel: Option<ColumnPanel>,
}

impl FieldHost for TableView {
    fn fields(&mut self) -> &mut FieldEdit {
        &mut self.edit
    }

    fn store(&self) -> Entity<Store> {
        self.store.clone()
    }

    fn refresh(&mut self, note: &NoteId, cx: &mut Context<Self>) {
        self.list.invalidate(&Row::Note(note.clone()), cx);
        cx.notify();
    }
}

impl TableView {
    pub fn new(store: Entity<Store>, folder: FolderId, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| TextFieldState::new(window, cx).placeholder("Filter"));
        cx.observe(&store, |this, _, cx| this.reload(cx)).detach();
        cx.observe(&filter, |this, _, cx| this.reload(cx)).detach();
        let list = ListState::new(cx).estimated_row_height(px(ROW_HEIGHT));
        let mut this = Self {
            store,
            folder,
            list,
            filter,
            edit: FieldEdit::new("cell", window, cx),
            cols: Vec::new(),
            title_width: TITLE_WIDTH,
            modified_width: MODIFIED_WIDTH,
            layout: 0,
            panel: None,
        };
        this.reload(cx);
        this
    }

    pub fn folder(&self) -> &FolderId {
        &self.folder
    }

    /// The notes in the order the table shows them, filtered.
    pub fn rows(&self) -> Vec<NoteId> {
        self.list
            .items()
            .into_iter()
            .filter_map(|r| match r {
                Row::Note(id) => Some(id),
                Row::Add => None,
            })
            .collect()
    }

    fn view(&self, cx: &App) -> FolderView {
        self.store.read(cx).view(&self.folder)
    }

    fn set_view(&self, view: FolderView, cx: &mut Context<Self>) {
        let folder = self.folder.clone();
        self.store.update(cx, |s, cx| s.set_view(&folder, view, cx));
    }

    /// Reads the folder again: its columns, and its notes filtered and sorted.
    fn reload(&mut self, cx: &mut Context<Self>) {
        let view = self.view(cx);
        let mut cols = vec![Col::Title];
        cols.extend(view.columns.iter().cloned().map(Col::Prop));
        cols.extend([Col::Modified, Col::Add]);
        if cols != self.cols {
            self.cols = cols;
            self.layout += 1;
        }
        let query = self.filter.read(cx).text().trim().to_lowercase();
        let store = self.store.read(cx);
        let mut notes: Vec<&Note> = store.library().notes_in(&self.folder, cx.settings::<NotesSettings>().sort.into());
        if !query.is_empty() {
            notes.retain(|n| matches(n, &query));
        }
        if let Some(sort) = &view.sort {
            let kind = view.column(&sort.column).map(PropColumn::prop_type);
            notes.sort_by(|a, b| compare(a, b, &sort.column, kind, sort.descending));
        }
        let layout = self.layout;
        let rows: Vec<(Row, u64)> = notes
            .iter()
            .map(|n| {
                let mut h = DefaultHasher::new();
                (&*n.text, &n.title, n.modified, layout).hash(&mut h);
                (Row::Note(n.id.clone()), h.finish())
            })
            .chain([(Row::Add, layout)])
            .collect();
        self.list.set_items_versioned(rows, cx);
        cx.notify();
    }

    fn sort_by(&mut self, e: &SortChanged, window: &mut Window, cx: &mut Context<Self>) {
        let column = match e.column.as_ref() {
            ADD_COLUMN => return self.start_add(window, cx),
            id => id.strip_prefix("prop:").unwrap_or(id).to_owned(),
        };
        let mut view = self.view(cx);
        view.sort = Some(SortBy::new(column, e.direction == SortDirection::Descending));
        self.set_view(view, cx);
    }

    fn resized(&mut self, e: &ColumnResized, cx: &mut Context<Self>) {
        match self.cols.get(e.column) {
            Some(Col::Title) => self.title_width = e.width,
            Some(Col::Modified) => self.modified_width = e.width,
            Some(Col::Prop(c)) => {
                let mut view = self.view(cx);
                if let Some(c) = view.column_mut(&c.name)
                    && c.width.is_none_or(|w| (w - e.width).abs() >= 1.)
                {
                    c.width = Some(e.width.round());
                    self.set_view(view, cx);
                }
                return;
            }
            Some(Col::Add) | None => return,
        }
        self.layout += 1;
        self.reload(cx);
    }

    /// Opens the panel that names a new column.
    pub fn start_add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_panel(None, String::new(), window, cx);
    }

    /// Opens the panel that renames `column`.
    pub fn start_rename(&mut self, column: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.open_panel(Some(column.to_owned()), column.to_owned(), window, cx);
    }

    fn open_panel(&mut self, renaming: Option<String>, name: String, window: &mut Window, cx: &mut Context<Self>) {
        let state = cx.new(|cx| TextFieldState::new(window, cx).placeholder("Name"));
        state.update(cx, |s, cx| s.set_text(name, window, cx));
        self.panel = Some(ColumnPanel { renaming, name: state, kind: 0, taken: false });
        cx.notify();
        window.focus_id(cx, COLUMN_NAME);
    }

    fn commit_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut view = self.view(cx);
        let Some(panel) = self.panel.as_mut() else { return };
        let name = properties::clean_name(panel.name.read(cx).text().as_ref());
        if panel.renaming.as_deref() == Some(name.as_str()) {
            self.panel = None;
            cx.notify();
            return;
        }
        if !view.name_free(&name) {
            panel.taken = true;
            cx.notify();
            return;
        }
        let renaming = panel.renaming.clone();
        let kind = PropType::ALL.get(panel.kind).copied().unwrap_or(PropType::Text);
        self.panel = None;
        match renaming {
            Some(from) => {
                let action = ColumnAction(folder_key(&self.folder).to_string(), from, ColumnOp::RenameTo(name));
                cx.dispatch_action(window.id(), &action);
            }
            None => {
                view.columns.push(PropColumn::new(name, kind));
                self.set_view(view, cx);
            }
        }
        cx.notify();
    }

    fn cell(&mut self, row: Row, col: usize, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let cell = || div().h(px(ROW_HEIGHT)).px(theme.spacing.sm).flex().items_center().min_w_0();
        let Row::Note(id) = row else {
            if !matches!(self.cols.get(col), Some(Col::Title)) {
                return cell().into_any_element();
            }
            let key = folder_key(&self.folder).to_string();
            return cell()
                .id(ADD_ROW)
                .gap(theme.spacing.sm)
                .child(icon(Icon::Plus, 12.).text_color(theme.text.tertiary))
                .child(text("New note").text_color(theme.text.secondary))
                .on_tap(move |_: &TapEvent, window, cx| {
                    cx.dispatch_action(window.id(), &NewNoteIn(key.clone()));
                })
                .into_any_element();
        };
        let library = self.store.read(cx).library().clone();
        let Some(note) = library.note(&id) else { return cell().into_any_element() };
        let menu = note_menu(note_key(&id).to_string(), move_targets(&library, &self.folder));
        let content = match self.cols.get(col) {
            Some(Col::Title) => {
                let key = note_key(&id).to_string();
                return cell()
                    .id(frame_id(&EntryId::Note(id.clone())))
                    .gap(theme.spacing.sm)
                    .child(icon(Icon::FileText, 13.).text_color(theme.text.tertiary))
                    .child(
                        text(note.title.clone())
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text.primary)
                            .truncate(),
                    )
                    .on_tap(move |_: &TapEvent, window, cx| {
                        cx.dispatch_action(window.id(), &OpenEntry(key.clone()));
                    })
                    .context_menu(menu)
                    .into_any_element();
            }
            Some(Col::Prop(c)) => {
                let ty = c.prop_type();
                fields::editor(
                    &self.edit,
                    Field {
                        target: Target { note: id.clone(), name: c.name.clone() },
                        folder: self.folder.clone(),
                        ty,
                        raw: properties::get(&note.text, &c.name),
                        options: c.options.clone(),
                        align_end: ty == PropType::Number,
                        placeholder: None,
                    },
                    cx,
                )
            }
            Some(Col::Modified) => text(format::relative(note.modified, Timestamp::now()))
                .tabular_nums()
                .text_color(theme.text.secondary)
                .truncate()
                .into_any_element(),
            Some(Col::Add) | None => div().into_any_element(),
        };
        cell()
            .child(div().size_full().min_w_0().flex().items_center().child(content))
            .context_menu(menu)
            .into_any_element()
    }

    fn columns(&self, cx: &App) -> Vec<Column> {
        let sort = self.view(cx).sort;
        let sorted = |c: Column, name: &str| match &sort {
            Some(s) if s.column == name => {
                c.sorted(if s.descending { SortDirection::Descending } else { SortDirection::Ascending })
            }
            _ => c.sortable(),
        };
        self.cols
            .iter()
            .map(|col| match col {
                Col::Title => sorted(column(view::TITLE, "Title").width(self.title_width).min_width(120.), view::TITLE),
                Col::Prop(c) => sorted(
                    column(format!("prop:{}", c.name), c.name.clone())
                        .width(c.width.unwrap_or_else(|| default_width(c.prop_type())))
                        .min_width(60.),
                    &c.name,
                ),
                Col::Modified => {
                    sorted(column(view::MODIFIED, "Modified").width(self.modified_width).min_width(80.), view::MODIFIED)
                }
                Col::Add => column(ADD_COLUMN, "+").width(PLUS_WIDTH).fixed().sortable(),
            })
            .collect()
    }

    /// Crumbs, the count, the subfolders, the filter and the columns menu.
    fn header(&self, count: usize, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme().clone();
        let library = self.store.read(cx).library().clone();
        let subfolders: Vec<FolderId> = library
            .subfolders(&self.folder, cx.settings::<NotesSettings>().sort.into())
            .iter()
            .map(|f| f.id.clone())
            .collect();
        let key = folder_key(&self.folder).to_string();
        let view = self.view(cx);
        let columns_menu = menu_button("table-columns", Icon::Ellipsis, move |menu, _, _| {
            let mut menu = menu.item("Add Property…", AddColumn(key.clone()));
            if !view.columns.is_empty() {
                menu = menu.separator();
            }
            let last = view.columns.len().saturating_sub(1);
            for (i, c) in view.columns.iter().enumerate() {
                let act = |op: ColumnOp| ColumnAction(key.clone(), c.name.clone(), op);
                let types = Menu::new("Type").items(PropType::ALL.map(|t| {
                    MenuItem::action(t.label(), act(ColumnOp::SetType(t.as_str().to_owned())))
                        .checked(c.prop_type() == t)
                }));
                menu = menu.submenu(Menu::new(c.name.clone()).items([
                    MenuItem::action("Rename…", act(ColumnOp::Rename)),
                    MenuItem::submenu(types),
                    MenuItem::separator(),
                    MenuItem::action("Move Left", act(ColumnOp::MoveLeft)).enabled(i > 0),
                    MenuItem::action("Move Right", act(ColumnOp::MoveRight)).enabled(i < last),
                    MenuItem::separator(),
                    MenuItem::action("Delete Property", act(ColumnOp::Delete)).destructive(),
                ]));
            }
            menu
        })
        .tooltip("Properties");
        h_flex()
            .flex_none()
            .h(px(HEADER_HEIGHT))
            .px(px(PAGE_PAD_X + 2.))
            .gap(theme.spacing.md)
            .items_center()
            .child(crumbs(&theme, &self.folder))
            .child(text(format::notes(count)).id("page-count").text_color(theme.text.secondary))
            .child(h_flex().gap(theme.spacing.xs).min_w_0().overflow_hidden().children(subfolders.into_iter().map(
                |f| {
                    button(format!("subfolder:{}", f.0), f.name().to_owned())
                        .icon(Icon::Folder)
                        .bezel(Bezel::Borderless)
                        .dispatch(OpenEntry(folder_key(&f).to_string()))
                },
            )))
            .child(div().flex_1())
            .child(search_field(FILTER, &self.filter).label("Filter notes").w(px(200.)))
            .child(columns_menu)
    }

    fn render_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let state = self.panel.as_ref()?;
        let theme = cx.theme().clone();
        let title = if state.renaming.is_some() { "Rename Property" } else { "New Property" };
        let len = state.name.read(cx).text().chars().count();
        let kind = state.renaming.is_none().then(|| {
            select("column-type", PropType::ALL.map(PropType::label)).selected(state.kind).label("Type").on_change(
                cx.listener(|this, e: &SelectChanged, _, cx| {
                    if let Some(p) = this.panel.as_mut() {
                        p.kind = e.index;
                    }
                    cx.notify();
                }),
            )
        });
        let taken = state.taken.then(|| text("That name is taken.").text_color(theme.text.danger));
        Some(
            panel("column-panel")
                .w(px(360.))
                .p(theme.spacing.lg)
                .gap(theme.spacing.md)
                .child(text(title).font_weight(FontWeight::SEMIBOLD).text_color(theme.text.primary))
                .child(
                    text_field(COLUMN_NAME)
                        .bind(&state.name)
                        .label("Name")
                        .select_on_focus(0..len)
                        .on_submit(cx.listener(|this, _: &TextSubmitted, window, cx| this.commit_panel(window, cx))),
                )
                .children(kind)
                .children(taken)
                .child(
                    h_flex()
                        .justify_end()
                        .gap(theme.spacing.sm)
                        .child(button("column-cancel", "Cancel").on_click(cx.listener(
                            |this, _: &ClickEvent, _, cx| {
                                this.panel = None;
                                cx.notify();
                            },
                        )))
                        .child(
                            button("column-save", if state.renaming.is_some() { "Rename" } else { "Add" })
                                .primary()
                                .on_click(
                                    cx.listener(|this, _: &ClickEvent, window, cx| this.commit_panel(window, cx)),
                                ),
                        ),
                )
                .on_dismiss(cx.listener(|this, _: &Dismissed, _, cx| {
                    this.panel = None;
                    cx.notify();
                }))
                .into_any_element(),
        )
    }
}

impl Render for TableView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.list.len().saturating_sub(1);
        let header = self.header(count, cx);
        let this = cx.weak_entity();
        let table = table(TABLE, &self.list, self.columns(cx), move |row, col, window, cx| {
            this.update(cx, |t, cx| t.cell(row, col, window, cx)).unwrap_or_else(|| div().into_any_element())
        })
        .selection(Selection::single())
        .selectable(|row, _| matches!(row, Row::Note(_)))
        .drag_rows(ENTRY_DRAG, |row| match row {
            Row::Note(id) => Some(note_key(id)),
            Row::Add => None,
        })
        .on_activate(|e: &ActivateEvent<Row>, window, cx| {
            if let Row::Note(id) = &e.key {
                cx.dispatch_action(window.id(), &OpenEntry(note_key(id).to_string()));
            }
        })
        .on_sort(cx.listener(|this, e: &SortChanged, window, cx| this.sort_by(e, window, cx)))
        .on_column_resize(cx.listener(|this, e: &ColumnResized, _, cx| this.resized(e, cx)))
        .flex_1()
        .min_h_0();
        let panel = self.render_panel(cx);
        v_flex()
            .id("table-page")
            .size_full()
            .child(header)
            .child(div().flex_1().min_h_0().px(px(PAGE_PAD_X)).flex().flex_col().child(table))
            .children(panel)
    }
}

fn default_width(ty: PropType) -> f32 {
    match ty {
        PropType::Checkbox => 90.,
        PropType::Number => 110.,
        PropType::Date => 140.,
        PropType::Select => 150.,
        PropType::MultiSelect | PropType::Text | PropType::Url => 200.,
    }
}

/// Whether the note's title or any of its property values holds `query` (lowercase).
fn matches(note: &Note, query: &str) -> bool {
    note.title.to_lowercase().contains(query)
        || properties::read(&note.text).iter().any(|(_, raw)| raw.display().to_lowercase().contains(query))
}

/// Two notes by a column: a built-in one, or a property read as its column's type. Empty values
/// go last whichever way the column sorts.
fn compare(a: &Note, b: &Note, column: &str, kind: Option<PropType>, descending: bool) -> Ordering {
    let flip = |o: Ordering| if descending { o.reverse() } else { o };
    match column {
        view::TITLE => flip(a.title.to_lowercase().cmp(&b.title.to_lowercase())),
        view::CREATED => flip(a.created.cmp(&b.created)),
        view::MODIFIED => flip(a.modified.cmp(&b.modified)),
        name => {
            let kind = kind.unwrap_or(PropType::Text);
            let read = |n: &Note| properties::get(&n.text, name).and_then(|r: Raw| Value::read(&r, kind));
            match (read(a), read(b)) {
                (Some(x), Some(y)) => flip(compare_values(&x, &y)),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            }
        }
    }
    .then_with(|| a.id.cmp(&b.id))
}

fn compare_values(a: &Value, b: &Value) -> Ordering {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.total_cmp(y),
        (Value::Checkbox(x), Value::Checkbox(y)) => y.cmp(x),
        (Value::Date(x), Value::Date(y)) => x.cmp(y),
        (Value::Text(x), Value::Text(y)) => x.to_lowercase().cmp(&y.to_lowercase()),
        (Value::List(x), Value::List(y)) => x.join(", ").to_lowercase().cmp(&y.join(", ").to_lowercase()),
        _ => Ordering::Equal,
    }
}
