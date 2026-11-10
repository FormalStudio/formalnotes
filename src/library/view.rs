//! `.folder.json`: how a folder shows its notes, cards or a table, and the table's columns and
//! sort. Keys this app does not know are kept through a rewrite, at every level.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::properties::PropType;

pub const VIEW_FILE: &str = ".folder.json";

/// The built-in columns `sort.column` may name besides the folder's own.
pub const TITLE: &str = "title";
pub const CREATED: &str = "created";
pub const MODIFIED: &str = "modified";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FolderView {
    #[serde(default = "version")]
    pub v: u64,
    /// `cards` or `table`; anything else shows as cards and is written back as it was.
    #[serde(default = "cards")]
    pub view: String,
    #[serde(default)]
    pub columns: Vec<Column>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<SortBy>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    /// One of [`PropType`]'s names; an unknown one reads as text and is written back as it was.
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", serialize_with = "points")]
    pub width: Option<f32>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SortBy {
    pub column: String,
    #[serde(default)]
    pub descending: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Widths are written as whole points when they are whole: `140`, not `140.0`.
fn points<S: serde::Serializer>(width: &Option<f32>, s: S) -> Result<S::Ok, S::Error> {
    match width {
        Some(w) if w.fract() == 0. && w.abs() < 1e9 => s.serialize_i64(*w as i64),
        Some(w) => s.serialize_f32(*w),
        None => s.serialize_none(),
    }
}

fn version() -> u64 {
    1
}

fn cards() -> String {
    "cards".to_owned()
}

impl Default for FolderView {
    fn default() -> Self {
        Self { v: 1, view: cards(), columns: Vec::new(), sort: None, extra: Map::new() }
    }
}

impl Column {
    pub fn new(name: impl Into<String>, kind: PropType) -> Self {
        Self { name: name.into(), kind: kind.as_str().to_owned(), options: Vec::new(), width: None, extra: Map::new() }
    }

    pub fn prop_type(&self) -> PropType {
        PropType::parse(&self.kind).unwrap_or(PropType::Text)
    }
}

impl SortBy {
    pub fn new(column: impl Into<String>, descending: bool) -> Self {
        Self { column: column.into(), descending, extra: Map::new() }
    }
}

impl FolderView {
    pub fn parse(bytes: &[u8]) -> Option<FolderView> {
        serde_json::from_slice(bytes).ok()
    }

    pub fn to_json(&self) -> Vec<u8> {
        let mut out = serde_json::to_vec_pretty(self).unwrap_or_default();
        out.push(b'\n');
        out
    }

    pub fn is_table(&self) -> bool {
        self.view == "table"
    }

    pub fn set_table(&mut self, table: bool) {
        self.view = if table { "table" } else { "cards" }.to_owned();
    }

    pub fn column(&self, name: &str) -> Option<&Column> {
        self.columns.iter().find(|c| c.name == name)
    }

    pub fn column_mut(&mut self, name: &str) -> Option<&mut Column> {
        self.columns.iter_mut().find(|c| c.name == name)
    }

    /// Whether `name` is free for a new column: not a built-in and not taken.
    pub fn name_free(&self, name: &str) -> bool {
        !name.is_empty() && ![TITLE, CREATED, MODIFIED].contains(&name) && self.column(name).is_none()
    }

    /// A free column name: `base`, then `base 2` and so on.
    pub fn free_name(&self, base: &str) -> String {
        (1..)
            .map(|n| if n == 1 { base.to_owned() } else { format!("{base} {n}") })
            .find(|name| self.name_free(name))
            .unwrap_or_default()
    }

    /// Renames the column, and the sort when it was on it. False when `to` is taken.
    pub fn rename_column(&mut self, from: &str, to: &str) -> bool {
        if !self.name_free(to) {
            return false;
        }
        let Some(column) = self.column_mut(from) else { return false };
        column.name = to.to_owned();
        if let Some(sort) = self.sort.as_mut().filter(|s| s.column == from) {
            sort.column = to.to_owned();
        }
        true
    }

    pub fn remove_column(&mut self, name: &str) {
        self.columns.retain(|c| c.name != name);
        if self.sort.as_ref().is_some_and(|s| s.column == name) {
            self.sort = None;
        }
    }

    /// Moves the column `by` places, staying inside the row.
    pub fn move_column(&mut self, name: &str, by: isize) {
        let Some(at) = self.columns.iter().position(|c| c.name == name) else { return };
        let to = at.saturating_add_signed(by).min(self.columns.len().saturating_sub(1));
        let column = self.columns.remove(at);
        self.columns.insert(to, column);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = r#"{
  "v": 1,
  "view": "table",
  "columns": [
    { "name": "status", "type": "select", "options": ["Todo", "Doing", "Done"], "width": 140 },
    { "name": "due", "type": "date" }
  ],
  "sort": { "column": "modified", "descending": true }
}"#;

    #[test]
    fn the_documented_file_reads() {
        let view = FolderView::parse(EXAMPLE.as_bytes()).unwrap();
        assert!(view.is_table());
        assert_eq!(view.columns.len(), 2);
        assert_eq!(view.columns[0].prop_type(), PropType::Select);
        assert_eq!(view.columns[0].options, ["Todo", "Doing", "Done"]);
        assert_eq!(view.columns[0].width, Some(140.));
        assert_eq!(view.columns[1].prop_type(), PropType::Date);
        assert_eq!(view.sort, Some(SortBy::new(MODIFIED, true)));
        let back: Value = serde_json::from_slice(&view.to_json()).unwrap();
        assert_eq!(back, serde_json::from_str::<Value>(EXAMPLE).unwrap(), "written back as documented");
        assert!(FolderView::parse(b"{}").is_some_and(|v| !v.is_table() && v.columns.is_empty()));
        assert!(FolderView::parse(b"not json").is_none());
    }

    #[test]
    fn unknown_keys_survive_a_rewrite() {
        let text = r#"{"v":1,"view":"board","group":"status","columns":[{"name":"a","type":"rating","color":"red"}],"sort":{"column":"a","descending":false,"nulls":"last"}}"#;
        let mut view = FolderView::parse(text.as_bytes()).unwrap();
        assert!(!view.is_table(), "an unknown view shows as cards");
        assert_eq!(view.columns[0].prop_type(), PropType::Text, "an unknown type reads as text");
        view.move_column("a", 1);
        let back: Value = serde_json::from_slice(&view.to_json()).unwrap();
        let original: Value = serde_json::from_str(text).unwrap();
        assert_eq!(back, original);
    }

    #[test]
    fn columns_rename_move_and_go() {
        let mut view = FolderView::parse(EXAMPLE.as_bytes()).unwrap();
        view.sort = Some(SortBy::new("status", false));
        assert!(!view.rename_column("status", "due"), "taken");
        assert!(!view.rename_column("status", "title"), "built in");
        assert!(view.rename_column("status", "state"));
        assert_eq!(view.sort.as_ref().map(|s| s.column.as_str()), Some("state"));
        view.move_column("state", 1);
        assert_eq!(view.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["due", "state"]);
        view.move_column("state", 1);
        assert_eq!(view.columns[1].name, "state", "stays at the end");
        view.move_column("state", -5);
        assert_eq!(view.columns[0].name, "state");
        view.remove_column("state");
        assert!(view.sort.is_none());
        assert_eq!(view.free_name("due"), "due 2");
    }
}
