//! Note properties and folder columns as `docs/protocol.md` ("Files in a library") defines
//! them, in the small shape the organizer needs. The library will model both; until then the
//! organizer writes a properties block through [`with_properties`] and reads columns from
//! [`FolderColumns`], which the app leaves empty.

use std::collections::HashMap;

use crate::library::FolderId;

/// A folder's column: a property its notes carry, with its type and, for selects, its options.
#[derive(Clone, Debug, PartialEq)]
pub struct Column {
    pub name: String,
    /// `text`, `select`, `multi_select`, `number`, `checkbox`, `date` or `url`.
    pub kind: String,
    pub options: Vec<String>,
}

/// The columns of each folder that has any.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FolderColumns(pub HashMap<FolderId, Vec<Column>>);

impl FolderColumns {
    pub fn of(&self, folder: &FolderId) -> &[Column] {
        self.0.get(folder).map_or(&[], Vec::as_slice)
    }
}

/// `text` under a properties block holding `properties` (name, value), each written as its
/// column's type says; a property no column names is text. Empty values are left out, and no
/// properties leave the text as it is.
pub fn with_properties(text: &str, properties: &[(String, String)], columns: &[Column]) -> String {
    let lines: Vec<String> = properties
        .iter()
        .filter(|(name, value)| !name.trim().is_empty() && !value.trim().is_empty())
        .map(|(name, value)| {
            let kind = columns.iter().find(|c| c.name == *name).map_or("text", |c| c.kind.as_str());
            format!("{}: {}", name.trim(), typed(value.trim(), kind))
        })
        .collect();
    if lines.is_empty() {
        return text.to_owned();
    }
    format!("---\n{}\n---\n{text}", lines.join("\n"))
}

fn typed(value: &str, kind: &str) -> String {
    match kind {
        "number" if value.parse::<f64>().is_ok() => value.to_owned(),
        "checkbox" if matches!(value, "true" | "false") => value.to_owned(),
        "date" if jiff::civil::Date::strptime("%Y-%m-%d", value).is_ok() => value.to_owned(),
        "multi_select" => {
            let items: Vec<String> = value
                .trim_start_matches('[')
                .trim_end_matches(']')
                .split(',')
                .map(str::trim)
                .filter(|i| !i.is_empty())
                .map(|i| quoted(i, true))
                .collect();
            format!("[{}]", items.join(", "))
        }
        _ => quoted(value, false),
    }
}

/// The string, double-quoted when it would not read back as a string.
fn quoted(value: &str, in_list: bool) -> String {
    let needs = matches!(value, "true" | "false")
        || value.parse::<f64>().is_ok()
        || value != value.trim()
        || value.contains(": ")
        || value.starts_with(['[', '"', '#', '-'])
        || (in_list && value.contains([',', ']']));
    if needs { format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\"")) } else { value.to_owned() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column(name: &str, kind: &str) -> Column {
        Column { name: name.into(), kind: kind.into(), options: Vec::new() }
    }

    #[test]
    fn a_block_goes_above_the_text_typed_by_its_columns() {
        let props = [
            ("status".to_owned(), "Doing".to_owned()),
            ("due".to_owned(), "2026-11-14".to_owned()),
            ("tags".to_owned(), "work, q4".to_owned()),
            ("done".to_owned(), "false".to_owned()),
            ("note".to_owned(), "12".to_owned()),
            ("empty".to_owned(), " ".to_owned()),
        ];
        let columns = [column("due", "date"), column("tags", "multi_select"), column("done", "checkbox")];
        assert_eq!(
            with_properties("# Plan\n", &props, &columns),
            "---\nstatus: Doing\ndue: 2026-11-14\ntags: [work, q4]\ndone: false\nnote: \"12\"\n---\n# Plan\n"
        );
    }

    #[test]
    fn no_properties_leave_the_text_alone() {
        assert_eq!(with_properties("# Plan\n", &[], &[]), "# Plan\n");
        assert_eq!(quoted("- dash", false), "\"- dash\"");
        assert_eq!(quoted("a: b", false), "\"a: b\"");
    }
}
