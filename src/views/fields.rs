//! One property's value, shown and edited by its type. The table's cells and the presented
//! note's properties both use these: text, numbers and URLs edit in a field that stands in for
//! the value while it has focus, select, multi-select and dates pick in a popover, a checkbox
//! toggles where it is.

use nd::prelude::*;

use crate::library::properties::parse_number;
use crate::library::{FolderId, NoteId, PropType, Raw, Store, Value};

/// A property of a note.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Target {
    pub note: NoteId,
    pub name: String,
}

/// Which value is being edited, and the fields that edit it. One per view that shows values.
pub struct FieldEdit {
    editing: Option<(Target, PropType)>,
    picking: Option<Target>,
    text: Entity<TextFieldState>,
    option: Entity<TextFieldState>,
    /// Keeps the ids of two views' fields apart.
    prefix: &'static str,
}

impl FieldEdit {
    pub fn new<T: 'static>(prefix: &'static str, window: &mut Window, cx: &mut Context<T>) -> Self {
        Self {
            editing: None,
            picking: None,
            text: cx.new(|cx| TextFieldState::new(window, cx)),
            option: cx.new(|cx| TextFieldState::new(window, cx).placeholder("Add an option")),
            prefix,
        }
    }

    pub fn field_id(&self) -> String {
        format!("{}-field", self.prefix)
    }

    fn value_id(&self, target: &Target) -> String {
        format!("{}:{}:{}", self.prefix, target.name, target.note.0)
    }

    /// Stops editing whatever was open, without writing it.
    pub fn reset(&mut self) {
        self.editing = None;
        self.picking = None;
    }
}

/// A view that shows editable values: it owns a [`FieldEdit`] and re-renders a note's values
/// when their editing state changes.
pub trait FieldHost: Sized + 'static {
    fn fields(&mut self) -> &mut FieldEdit;
    fn store(&self) -> Entity<Store>;
    fn refresh(&mut self, note: &NoteId, cx: &mut Context<Self>);
}

/// What one editor shows.
pub struct Field {
    pub target: Target,
    pub folder: FolderId,
    pub ty: PropType,
    pub raw: Option<Raw>,
    /// A select's options, from the folder's column.
    pub options: Vec<String>,
    /// Numbers sit at the trailing edge of a table column.
    pub align_end: bool,
    /// Shown for an empty value; a table leaves its empty cells blank.
    pub placeholder: Option<&'static str>,
}

pub fn editor<T: FieldHost>(edit: &FieldEdit, field: Field, cx: &mut Context<T>) -> AnyElement {
    let theme = cx.theme().clone();
    let value = field.raw.as_ref().and_then(|raw| Value::read(raw, field.ty));
    let id = edit.value_id(&field.target);
    let editing = edit.editing.as_ref().is_some_and(|(t, _)| *t == field.target);
    if editing {
        return text_field(edit.field_id())
            .bind(&edit.text)
            .label(field.target.name.clone())
            .on_submit(cx.listener(|this: &mut T, _: &TextSubmitted, _, cx| commit(this, cx)))
            .on_end_editing(cx.listener(|this: &mut T, _: &TextEditingEnded, _, cx| commit(this, cx)))
            .on_cancel(cx.listener(|this: &mut T, _: &TextCancelled, _, cx| {
                if let Some((target, _)) = this.fields().editing.take() {
                    this.refresh(&target.note, cx);
                }
            }))
            .w_full()
            .into_any_element();
    }
    let empty = || match field.placeholder {
        Some(p) => text(p).text_color(theme.text.tertiary).truncate().into_any_element(),
        None => div().into_any_element(),
    };
    let tap_to_edit = |content: AnyElement, cx: &mut Context<T>| {
        let (target, ty) = (field.target.clone(), field.ty);
        let initial = field.raw.as_ref().map(Raw::display).unwrap_or_default();
        h_flex()
            .id(id.clone())
            .size_full()
            .min_w_0()
            .items_center()
            .when(field.align_end, |d| d.justify_end())
            .child(content)
            .on_tap(cx.listener(move |this: &mut T, _: &TapEvent, window, cx| {
                start(this, target.clone(), ty, initial.clone(), window, cx)
            }))
            .into_any_element()
    };
    match field.ty {
        PropType::Text => {
            let content = match &value {
                Some(Value::Text(s)) => text(s.clone()).text_color(theme.text.primary).truncate().into_any_element(),
                _ => empty(),
            };
            tap_to_edit(content, cx)
        }
        PropType::Number => {
            let content = match &value {
                Some(Value::Number(n)) => {
                    text(format!("{n}")).tabular_nums().text_color(theme.text.primary).truncate().into_any_element()
                }
                _ => empty(),
            };
            tap_to_edit(content, cx)
        }
        PropType::Url => {
            let link = match &value {
                Some(Value::Text(url)) => link(format!("{id}:open"), url.clone()).url(url.clone()).into_any_element(),
                _ => empty(),
            };
            let rest = tap_to_edit(div().into_any_element(), cx);
            h_flex()
                .size_full()
                .min_w_0()
                .items_center()
                .child(div().flex_shrink_0().max_w_full().min_w_0().child(link))
                .child(div().flex_1().h_full().child(rest))
                .into_any_element()
        }
        PropType::Checkbox => {
            let checked = matches!(value, Some(Value::Checkbox(true)));
            let target = field.target.clone();
            h_flex()
                .size_full()
                .items_center()
                .child(checkbox(id, "").checked(checked).on_toggle(cx.listener(
                    move |this: &mut T, e: &Toggled, _, cx| {
                        set(this, &target, Some(Value::Checkbox(e.checked)), cx);
                    },
                )))
                .into_any_element()
        }
        PropType::Date | PropType::Select | PropType::MultiSelect => {
            let content = match &value {
                Some(Value::Date(d)) => text(d.strftime("%b %-d, %Y").to_string())
                    .tabular_nums()
                    .text_color(theme.text.primary)
                    .into_any_element(),
                Some(Value::Text(s)) => pill(&theme, s),
                Some(Value::List(items)) => h_flex()
                    .gap(theme.spacing.xs)
                    .min_w_0()
                    .overflow_hidden()
                    .children(items.iter().map(|i| pill(&theme, i)))
                    .into_any_element(),
                _ => empty(),
            };
            let picking = edit.picking.as_ref() == Some(&field.target);
            let target = field.target.clone();
            let open = h_flex().id(id.clone()).size_full().min_w_0().items_center().child(content).on_tap(cx.listener(
                move |this: &mut T, _: &TapEvent, _, cx| {
                    this.fields().editing = None;
                    this.fields().picking = Some(target.clone());
                    this.refresh(&target.note, cx);
                },
            ));
            let popover = picking.then(|| picker(edit, &field, value, cx));
            open.children(popover).into_any_element()
        }
    }
}

/// A select's value: a small pill.
pub fn pill(theme: &Theme, label: &str) -> AnyElement {
    div()
        .flex_none()
        .px(theme.spacing.xs + px(2.))
        .rounded(theme.radius.sm)
        .bg(theme.accent_container.bg)
        .child(
            text(label.to_owned())
                .text_size(theme.type_scale.footnote)
                .text_color(theme.accent_container.fg)
                .truncate(),
        )
        .into_any_element()
}

/// The popover a select, a multi-select or a date picks its value in.
fn picker<T: FieldHost>(edit: &FieldEdit, field: &Field, value: Option<Value>, cx: &mut Context<T>) -> AnyElement {
    let theme = cx.theme().clone();
    let target = field.target.clone();
    let has_value = value.is_some();
    let mut body = v_flex().gap(theme.spacing.xs);
    if field.ty == PropType::Date {
        let today = jiff::Zoned::now().date();
        let day = match value {
            Some(Value::Date(d)) => d,
            _ => today,
        };
        let t = target.clone();
        body = body.child(
            date_picker(
                format!("{}-date", edit.prefix),
                CivilDateTime::date(i32::from(day.year()), day.month() as u8, day.day() as u8),
            )
            .presentation(DatePickerStyle::Graphical)
            .label(field.target.name.clone())
            .on_change(cx.listener(move |this: &mut T, e: &DateChanged, _, cx| {
                let v = e.value;
                if let Ok(date) = jiff::civil::Date::new(v.year as i16, v.month as i8, v.day as i8) {
                    set(this, &t, Some(Value::Date(date)), cx);
                }
            })),
        );
    } else {
        let chosen: Vec<String> = match &value {
            Some(Value::Text(s)) => vec![s.clone()],
            Some(Value::List(items)) => items.clone(),
            _ => Vec::new(),
        };
        let mut options = field.options.clone();
        options.extend(chosen.iter().filter(|c| !field.options.contains(c)).cloned());
        let multi = field.ty == PropType::MultiSelect;
        for option in options {
            let selected = chosen.contains(&option);
            let (t, o, current) = (target.clone(), option.clone(), chosen.clone());
            body = body.child(
                h_flex()
                    .id(format!("{}-option:{option}", edit.prefix))
                    .h(px(26.))
                    .px(theme.spacing.xs)
                    .gap(theme.spacing.sm)
                    .items_center()
                    .rounded(theme.radius.sm)
                    .hover(|s| s.bg(theme.surface.raised_hover))
                    .child(pill(&theme, &option))
                    .child(div().flex_1())
                    .when(selected, |d| d.child(icon(Icon::Check, 12.).text_color(theme.text.secondary)))
                    .on_tap(cx.listener(move |this: &mut T, _: &TapEvent, _, cx| {
                        pick(this, &t, &o, multi, &current, cx);
                    })),
            );
        }
        let (t, folder) = (target.clone(), field.folder.clone());
        let current = chosen.clone();
        body = body.child(
            text_field(format!("{}-new-option", edit.prefix)).bind(&edit.option).label("New option").on_submit(
                cx.listener(move |this: &mut T, _: &TextSubmitted, window, cx| {
                    let state = this.fields().option.clone();
                    let option = state.read(cx).text().trim().to_owned();
                    if option.is_empty() {
                        return;
                    }
                    state.update(cx, |s, cx| s.set_text(String::new(), window, cx));
                    let store = this.store();
                    store.update(cx, |s, cx| s.add_option(&folder, &t.name, &option, cx));
                    if !current.contains(&option) {
                        pick(this, &t, &option, multi, &current, cx);
                    }
                }),
            ),
        );
    }
    let t = target.clone();
    let clear = has_value.then(|| {
        h_flex().justify_end().child(button(format!("{}-clear", edit.prefix), "Clear").on_click(cx.listener(
            move |this: &mut T, _: &ClickEvent, _, cx| {
                this.fields().picking = None;
                set(this, &t, None, cx);
            },
        )))
    });
    let t = target;
    popover(format!("{}-picker", edit.prefix))
        .p(theme.spacing.sm)
        .gap(theme.spacing.xs)
        .min_w(px(200.))
        .child(body)
        .children(clear)
        .on_dismiss(cx.listener(move |this: &mut T, _: &Dismissed, _, cx| {
            this.fields().picking = None;
            this.refresh(&t.note, cx);
        }))
        .into_any_element()
}

/// Opens the editor of a value that has none yet: the field for text, numbers and URLs, the
/// popover for the others.
pub fn begin<T: FieldHost>(this: &mut T, target: Target, ty: PropType, window: &mut Window, cx: &mut Context<T>) {
    match ty {
        PropType::Text | PropType::Number | PropType::Url => start(this, target, ty, String::new(), window, cx),
        PropType::Checkbox => {}
        PropType::Date | PropType::Select | PropType::MultiSelect => {
            this.fields().picking = Some(target.clone());
            this.refresh(&target.note, cx);
        }
    }
}

fn start<T: FieldHost>(
    this: &mut T,
    target: Target,
    ty: PropType,
    initial: String,
    window: &mut Window,
    cx: &mut Context<T>,
) {
    let edit = this.fields();
    edit.picking = None;
    edit.editing = Some((target.clone(), ty));
    let (state, id) = (edit.text.clone(), edit.field_id());
    state.update(cx, |s, cx| s.set_text(initial, window, cx));
    this.refresh(&target.note, cx);
    window.focus_id(cx, &id);
}

/// Writes what the field holds. A number that does not read as one is not taken.
fn commit<T: FieldHost>(this: &mut T, cx: &mut Context<T>) {
    let Some((target, ty)) = this.fields().editing.take() else { return };
    let typed = this.fields().text.read(cx).text().trim().to_owned();
    let value = match ty {
        _ if typed.is_empty() => None,
        PropType::Number => match parse_number(&typed) {
            Some(n) => Some(Value::Number(n)),
            None => {
                this.refresh(&target.note, cx);
                return;
            }
        },
        _ => Some(Value::Text(typed)),
    };
    set(this, &target, value, cx);
}

fn set<T: FieldHost>(this: &mut T, target: &Target, value: Option<Value>, cx: &mut Context<T>) {
    let store = this.store();
    store.update(cx, |s, cx| s.set_property(&target.note, &target.name, value.as_ref(), cx));
    this.refresh(&target.note, cx);
}

/// A select takes the option, or lets go of it when it had it, and closes; a multi-select adds
/// or drops it and stays open.
fn pick<T: FieldHost>(
    this: &mut T,
    target: &Target,
    option: &str,
    multi: bool,
    current: &[String],
    cx: &mut Context<T>,
) {
    let value = if multi {
        let mut items = current.to_vec();
        if let Some(at) = items.iter().position(|i| i == option) {
            items.remove(at);
        } else {
            items.push(option.to_owned());
        }
        Some(Value::List(items))
    } else {
        this.fields().picking = None;
        (current.first().map(String::as_str) != Some(option)).then(|| Value::Text(option.to_owned()))
    };
    set(this, target, value, cx);
}
