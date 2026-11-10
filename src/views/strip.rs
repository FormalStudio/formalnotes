//! The presented note's properties, above its body: the folder's columns first, then the other
//! properties the note has, each edited by its type, and a way to add one. A property added here
//! waits in the strip until it gets a value, since an empty value is no line in the file.

use nd::prelude::*;

use super::fields::{self, Field, FieldEdit, FieldHost, Target};
use crate::actions::RemoveProperty;
use crate::library::properties::{self, clean_name};
use crate::library::{NoteId, PropType, Raw, Store, Value};
use crate::nav::note_key;

pub const ADD_PROPERTY: &str = "add-property";
const ADD_NAME: &str = "add-property-name";
/// The names' column: the values line up beside it.
const NAME_WIDTH: f32 = 140.;
const ROW_HEIGHT: f32 = 28.;

/// One row of the strip.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub name: String,
    pub ty: PropType,
    pub raw: Option<Raw>,
    pub options: Vec<String>,
}

pub struct PropertyStrip {
    store: Entity<Store>,
    note: Option<NoteId>,
    /// Added here, waiting for a value.
    pending: Vec<(String, PropType)>,
    edit: FieldEdit,
    adding: bool,
    add_name: Entity<TextFieldState>,
    add_type: usize,
}

impl FieldHost for PropertyStrip {
    fn fields(&mut self) -> &mut FieldEdit {
        &mut self.edit
    }

    fn store(&self) -> Entity<Store> {
        self.store.clone()
    }

    fn refresh(&mut self, _note: &NoteId, cx: &mut Context<Self>) {
        cx.notify();
    }
}

impl PropertyStrip {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Self {
            store,
            note: None,
            pending: Vec::new(),
            edit: FieldEdit::new("prop", window, cx),
            adding: false,
            add_name: cx.new(|cx| TextFieldState::new(window, cx).placeholder("Name")),
            add_type: 0,
        }
    }

    /// Shows `note`'s properties; another note starts with nothing pending or open.
    pub fn show(&mut self, note: Option<NoteId>, cx: &mut Context<Self>) {
        if self.note == note {
            return;
        }
        self.note = note;
        self.pending.clear();
        self.edit.reset();
        self.adding = false;
        cx.notify();
    }

    /// The rows the strip shows for its note.
    pub fn rows(&self, cx: &App) -> Vec<Row> {
        let Some(note) = self.note.as_ref().and_then(|id| self.store.read(cx).note(id)) else { return Vec::new() };
        let view = self.store.read(cx).view(&note.id.folder());
        let own = properties::read(&note.text);
        let mut rows: Vec<Row> = view
            .columns
            .iter()
            .map(|c| Row {
                name: c.name.clone(),
                ty: c.prop_type(),
                raw: own.iter().find(|(n, _)| *n == c.name).map(|(_, r)| r.clone()),
                options: c.options.clone(),
            })
            .collect();
        for (name, raw) in own {
            if rows.iter().any(|r| r.name == name) {
                continue;
            }
            let Some(ty) = raw.infer() else { continue };
            rows.push(Row { name, ty, raw: Some(raw), options: Vec::new() });
        }
        for (name, ty) in &self.pending {
            if !rows.iter().any(|r| r.name == *name) {
                rows.push(Row { name: name.clone(), ty: *ty, raw: None, options: Vec::new() });
            }
        }
        rows
    }

    fn start_adding(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.edit.reset();
        self.adding = true;
        self.add_type = 0;
        self.add_name.update(cx, |s, cx| s.set_text(String::new(), window, cx));
        cx.notify();
        window.focus_id(cx, ADD_NAME);
    }

    fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = clean_name(self.add_name.read(cx).text().as_ref());
        let ty = PropType::ALL.get(self.add_type).copied().unwrap_or(PropType::Text);
        let Some(note) = self.note.clone() else { return };
        if name.is_empty() || self.rows(cx).iter().any(|r| r.name == name) {
            return;
        }
        self.adding = false;
        if ty == PropType::Checkbox {
            self.store.update(cx, |s, cx| s.set_property(&note, &name, Some(&Value::Checkbox(false)), cx));
        } else {
            self.pending.push((name.clone(), ty));
            fields::begin(self, Target { note, name }, ty, window, cx);
        }
        cx.notify();
    }

    fn render_add(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let popover = self.adding.then(|| {
            popover("add-property-popover")
                .p(theme.spacing.md)
                .gap(theme.spacing.sm)
                .w(px(240.))
                .child(
                    text_field(ADD_NAME)
                        .bind(&self.add_name)
                        .label("Property name")
                        .on_submit(cx.listener(|this, _: &TextSubmitted, window, cx| this.add(window, cx))),
                )
                .child(
                    select("add-property-type", PropType::ALL.map(PropType::label))
                        .selected(self.add_type)
                        .label("Type")
                        .on_change(cx.listener(|this, e: &SelectChanged, _, cx| {
                            this.add_type = e.index;
                            cx.notify();
                        })),
                )
                .child(
                    h_flex().justify_end().child(
                        button("add-property-save", "Add")
                            .primary()
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.add(window, cx))),
                    ),
                )
                .on_dismiss(cx.listener(|this, _: &Dismissed, _, cx| {
                    this.adding = false;
                    cx.notify();
                }))
        });
        div()
            .child(
                button(ADD_PROPERTY, "Add property")
                    .icon(Icon::Plus)
                    .bezel(Bezel::Borderless)
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.start_adding(window, cx))),
            )
            .children(popover)
            .into_any_element()
    }
}

impl Render for PropertyStrip {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let Some(note) = self.note.clone() else { return div().id("properties").into_any_element() };
        let rows = self.rows(cx);
        let folder = note.folder();
        let mut strip = v_flex().id("properties").w_full();
        for row in rows {
            let key = note_key(&note).to_string();
            let name = row.name.clone();
            let label = div()
                .id(format!("property-name:{}", row.name))
                .flex_none()
                .w(px(NAME_WIDTH))
                .child(text(row.name.clone()).text_color(theme.text.secondary).truncate())
                .context_menu(move |menu, _, _| {
                    menu.item_destructive("Remove Property", RemoveProperty(key.clone(), name.clone()))
                });
            let field = Field {
                target: Target { note: note.clone(), name: row.name.clone() },
                folder: folder.clone(),
                ty: row.ty,
                raw: row.raw,
                options: row.options,
                align_end: false,
                placeholder: Some("Empty"),
            };
            let value = div().flex_1().min_w_0().h_full().child(fields::editor(&self.edit, field, cx));
            strip =
                strip.child(h_flex().h(px(ROW_HEIGHT)).gap(theme.spacing.md).items_center().child(label).child(value));
        }
        strip.child(div().pt(theme.spacing.xs).child(self.render_add(cx))).into_any_element()
    }
}
