//! A note's properties: the block of `name: value` lines between `---` fences at the very start
//! of its text (Obsidian's YAML front matter). Pure text in, text out. Reading models the flat
//! values `docs/protocol.md` lists; writing one property replaces only that property's lines and
//! leaves every other byte of the note as it was.

use std::ops::Range;

use jiff::civil::Date;

/// What a property holds, as a folder's column or the note's own editor reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PropType {
    Text,
    Number,
    Checkbox,
    Date,
    Select,
    MultiSelect,
    Url,
}

impl PropType {
    pub const ALL: [PropType; 7] = [
        PropType::Text,
        PropType::Number,
        PropType::Checkbox,
        PropType::Date,
        PropType::Select,
        PropType::MultiSelect,
        PropType::Url,
    ];

    /// The name `.folder.json` uses.
    pub fn as_str(self) -> &'static str {
        match self {
            PropType::Text => "text",
            PropType::Number => "number",
            PropType::Checkbox => "checkbox",
            PropType::Date => "date",
            PropType::Select => "select",
            PropType::MultiSelect => "multi_select",
            PropType::Url => "url",
        }
    }

    pub fn parse(name: &str) -> Option<PropType> {
        PropType::ALL.into_iter().find(|t| t.as_str() == name)
    }

    pub fn label(self) -> &'static str {
        match self {
            PropType::Text => "Text",
            PropType::Number => "Number",
            PropType::Checkbox => "Checkbox",
            PropType::Date => "Date",
            PropType::Select => "Select",
            PropType::MultiSelect => "Multi-select",
            PropType::Url => "URL",
        }
    }

    /// Select and multi-select pick from a column's options.
    pub fn has_options(self) -> bool {
        matches!(self, PropType::Select | PropType::MultiSelect)
    }
}

/// A property's value as written, before a type reads it.
#[derive(Clone, Debug, PartialEq)]
pub enum Raw {
    /// A single value; `quoted` when it was written in quotes, which makes it text.
    Scalar { text: String, quoted: bool },
    /// `[a, b]`, or a block list of `- a` lines.
    List(Vec<String>),
    /// YAML this app does not model: a nested map, a multi-line string. Kept, never edited.
    Other,
}

impl Raw {
    /// The value as one line of text, for filtering and for a text cell.
    pub fn display(&self) -> String {
        match self {
            Raw::Scalar { text, .. } => text.clone(),
            Raw::List(items) => items.join(", "),
            Raw::Other => String::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Raw::Scalar { text, .. } => text.is_empty(),
            Raw::List(items) => items.is_empty(),
            Raw::Other => false,
        }
    }

    /// The type a property outside any column reads as.
    pub fn infer(&self) -> Option<PropType> {
        match self {
            Raw::List(_) => Some(PropType::MultiSelect),
            Raw::Other => None,
            Raw::Scalar { text, quoted: true } if is_url(text) => Some(PropType::Url),
            Raw::Scalar { quoted: true, .. } => Some(PropType::Text),
            Raw::Scalar { text, quoted: false } => Some(if parse_bool(text).is_some() {
                PropType::Checkbox
            } else if parse_number(text).is_some() {
                PropType::Number
            } else if parse_date(text).is_some() {
                PropType::Date
            } else if is_url(text) {
                PropType::Url
            } else {
                PropType::Text
            }),
        }
    }
}

/// A typed value to write.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// Text, a select's option or a URL.
    Text(String),
    Number(f64),
    Checkbox(bool),
    Date(Date),
    List(Vec<String>),
}

impl Value {
    /// `raw` read as `ty`; `None` when it is empty or does not read as that type.
    pub fn read(raw: &Raw, ty: PropType) -> Option<Value> {
        let scalar = match raw {
            Raw::Scalar { text, .. } => Some(text.as_str()),
            Raw::List(_) | Raw::Other => None,
        };
        let value = match ty {
            PropType::Text | PropType::Select | PropType::Url => Value::Text(raw.display()),
            PropType::Number => Value::Number(parse_number(scalar?)?),
            PropType::Checkbox => Value::Checkbox(parse_bool(scalar?)?),
            PropType::Date => Value::Date(parse_date(scalar?)?),
            PropType::MultiSelect => match raw {
                Raw::List(items) => Value::List(items.clone()),
                Raw::Scalar { text, .. } => Value::List(vec![text.clone()]),
                Raw::Other => return None,
            },
        };
        (!value.is_empty()).then_some(value)
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Value::Text(s) => s.is_empty(),
            Value::List(items) => items.iter().all(|i| i.is_empty()),
            Value::Number(n) => !n.is_finite(),
            Value::Checkbox(_) | Value::Date(_) => false,
        }
    }

    /// What follows `name: ` in the file, or `None` for an empty value, which removes the line.
    pub fn yaml(&self) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        Some(match self {
            Value::Text(s) => scalar(s, false),
            Value::Number(n) => format!("{n}"),
            Value::Checkbox(b) => b.to_string(),
            Value::Date(d) => d.to_string(),
            Value::List(items) => {
                let items: Vec<String> = items.iter().filter(|i| !i.is_empty()).map(|i| scalar(i, true)).collect();
                format!("[{}]", items.join(", "))
            }
        })
    }
}

pub fn parse_bool(text: &str) -> Option<bool> {
    match text {
        "true" | "True" | "TRUE" => Some(true),
        "false" | "False" | "FALSE" => Some(false),
        _ => None,
    }
}

pub fn parse_number(text: &str) -> Option<f64> {
    let t = text.trim();
    let looks_numeric = t.chars().next().is_some_and(|c| c.is_ascii_digit() || matches!(c, '-' | '+' | '.'))
        && t.chars().all(|c| c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E'));
    t.parse::<f64>().ok().filter(|n| looks_numeric && n.is_finite())
}

/// `2026-11-14`, or the date of `2026-11-14T09:30`.
pub fn parse_date(text: &str) -> Option<Date> {
    let t = text.trim();
    let day = t.get(..10)?;
    let rest = &t[10..];
    if !(rest.is_empty() || rest.starts_with(['T', ' '])) {
        return None;
    }
    day.parse::<Date>().ok()
}

fn is_url(text: &str) -> bool {
    text.starts_with("https://") || text.starts_with("http://")
}

/// A string as YAML that reads back as the same string: plain when it can be, double-quoted
/// when plain would read as something else. `in_list` also quotes what a flow list would split.
fn scalar(s: &str, in_list: bool) -> String {
    let first = s.chars().next().unwrap_or(' ');
    let needs = parse_bool(s).is_some()
        || matches!(s, "null" | "Null" | "NULL" | "~")
        || parse_number(s).is_some()
        || parse_date(s).is_some()
        || first.is_whitespace()
        || s.ends_with(char::is_whitespace)
        || matches!(
            first,
            '[' | ']' | '{' | '}' | '"' | '\'' | '#' | '-' | '?' | ',' | '&' | '*' | '!' | '|' | '>' | '%' | '@' | '`'
        )
        || s.contains(": ")
        || s.ends_with(':')
        || s.contains(" #")
        || s.chars().any(char::is_control)
        || (in_list && s.contains([',', '[', ']', '{', '}']));
    if !needs {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The block's place in the text and its properties' places in the block.
struct Block {
    /// From the opening fence to the end of the closing fence's line.
    range: Range<usize>,
    /// The lines between the fences.
    inner: Range<usize>,
    newline: &'static str,
    entries: Vec<Entry>,
}

struct Entry {
    name: String,
    /// The name as written, quotes included.
    key: Range<usize>,
    /// The property's line and the indented or `- ` lines that continue it.
    lines: Range<usize>,
    raw: Raw,
}

/// Lines of `text` from `start`, each as (line without its ending, range with it).
fn lines_from(text: &str, start: usize) -> impl Iterator<Item = (&str, Range<usize>)> {
    let mut at = start;
    std::iter::from_fn(move || {
        if at >= text.len() {
            return None;
        }
        let end = text[at..].find('\n').map_or(text.len(), |i| at + i + 1);
        let line = text[at..end].trim_end_matches('\n').trim_end_matches('\r');
        let range = at..end;
        at = end;
        Some((line, range))
    })
}

fn block(text: &str) -> Option<Block> {
    let mut lines = lines_from(text, 0);
    let (first, first_range) = lines.next()?;
    if first != "---" || first_range.end == text.len() && !text.ends_with('\n') {
        return None;
    }
    let newline = if text[..first_range.end].ends_with("\r\n") { "\r\n" } else { "\n" };
    let close = lines.find(|(line, _)| *line == "---")?;
    let inner = first_range.end..close.1.start;
    let entries = entries(text, inner.clone());
    Some(Block { range: 0..close.1.end, inner, newline, entries })
}

fn entries(text: &str, inner: Range<usize>) -> Vec<Entry> {
    let lines: Vec<(&str, Range<usize>)> = lines_from(&text[..inner.end], inner.start).collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let (line, ref range) = lines[i];
        i += 1;
        let Some((key, name, value)) = key_line(line) else { continue };
        let start = range.start;
        let mut end = range.end;
        let empty_value = value.trim().is_empty();
        let mut continued = Vec::new();
        while let Some((next, next_range)) = lines.get(i) {
            let continues = next.starts_with([' ', '\t']) || (empty_value && next.starts_with('-'));
            if !continues || next.trim().is_empty() {
                break;
            }
            continued.push(*next);
            end = next_range.end;
            i += 1;
        }
        let raw = read_value(value, &continued);
        out.push(Entry { name, key: start + key.start..start + key.end, lines: start..end, raw });
    }
    out
}

/// `name: value` at the top level: the key's place in the line, the name, and the value text.
fn key_line(line: &str) -> Option<(Range<usize>, String, &str)> {
    if line.is_empty() || line.starts_with([' ', '\t', '#', '-']) {
        return None;
    }
    if let Some(q) = line.chars().next().filter(|c| matches!(c, '"' | '\'')) {
        let close = line[1..].find(q)? + 1;
        let rest = &line[close + 1..];
        let value = rest.strip_prefix(':').filter(|v| v.is_empty() || v.starts_with([' ', '\t']))?;
        return Some((0..close + 1, line[1..close].to_owned(), value));
    }
    let colon = line.match_indices(':').map(|(i, _)| i).find(|&i| {
        let after = &line[i + 1..];
        after.is_empty() || after.starts_with([' ', '\t'])
    })?;
    let name = line[..colon].trim_end();
    if name.is_empty() {
        return None;
    }
    Some((0..name.len(), name.to_owned(), &line[colon + 1..]))
}

fn read_value(value: &str, continued: &[&str]) -> Raw {
    let v = strip_comment(value.trim());
    if v.is_empty() {
        if continued.is_empty() {
            return Raw::Scalar { text: String::new(), quoted: false };
        }
        let items: Option<Vec<String>> = continued
            .iter()
            .map(|line| {
                let item = line.trim_start().strip_prefix('-')?;
                (item.is_empty() || item.starts_with([' ', '\t'])).then(|| unquote(strip_comment(item.trim())).0)
            })
            .collect();
        return items.map_or(Raw::Other, Raw::List);
    }
    if !continued.is_empty() || v.starts_with(['{', '|', '>', '&', '*', '!']) {
        return Raw::Other;
    }
    if let Some(inner) = v.strip_prefix('[') {
        return match inner.strip_suffix(']') {
            Some(inner) => Raw::List(split_flow(inner).into_iter().filter(|s| !s.is_empty()).collect()),
            None => Raw::Other,
        };
    }
    let (text, quoted) = unquote(v);
    Raw::Scalar { text, quoted }
}

/// A plain value without a trailing ` # comment`; quoted values keep their `#`.
fn strip_comment(v: &str) -> &str {
    if v.starts_with(['"', '\'', '[']) {
        return v;
    }
    match v.find(" #") {
        Some(i) => v[..i].trim_end(),
        None if v.starts_with('#') => "",
        None => v,
    }
}

/// The string a scalar holds, and whether it was quoted.
fn unquote(v: &str) -> (String, bool) {
    if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
        let mut out = String::new();
        let mut chars = v[1..v.len() - 1].chars();
        while let Some(c) = chars.next() {
            if c != '\\' {
                out.push(c);
                continue;
            }
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('u') => {
                    let hex: String = chars.by_ref().take(4).collect();
                    if let Some(c) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                        out.push(c);
                    }
                }
                Some(other) => out.push(other),
                None => {}
            }
        }
        return (out, true);
    }
    if v.len() >= 2 && v.starts_with('\'') && v.ends_with('\'') {
        return (v[1..v.len() - 1].replace("''", "'"), true);
    }
    (v.to_owned(), false)
}

/// The items of a flow list's inside, split on commas outside quotes.
fn split_flow(inner: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for c in inner.chars() {
        match quote {
            Some(q) => {
                current.push(c);
                if escaped {
                    escaped = false;
                } else if c == '\\' && q == '"' {
                    escaped = true;
                } else if c == q {
                    quote = None;
                }
            }
            None if c == ',' => items.push(std::mem::take(&mut current)),
            None => {
                if matches!(c, '"' | '\'') && current.trim().is_empty() {
                    quote = Some(c);
                }
                current.push(c);
            }
        }
    }
    items.push(current);
    items.into_iter().map(|item| unquote(item.trim()).0).collect()
}

/// How many bytes the properties block takes at the start of `text`, closing fence included.
pub fn block_len(text: &str) -> usize {
    block(text).map_or(0, |b| b.range.end)
}

/// The text after the properties block: what the title, the preview and the editor read.
pub fn body(text: &str) -> &str {
    &text[block_len(text)..]
}

/// A note's properties in the order the block lists them.
pub fn read(text: &str) -> Vec<(String, Raw)> {
    block(text).map(|b| b.entries.into_iter().map(|e| (e.name, e.raw)).collect()).unwrap_or_default()
}

pub fn get(text: &str, name: &str) -> Option<Raw> {
    block(text)?.entries.into_iter().find(|e| e.name == name).map(|e| e.raw)
}

/// `text` with `name` set to `value`; an empty value removes it.
pub fn set(text: &str, name: &str, value: &Value) -> String {
    match value.yaml() {
        Some(yaml) => put(text, name, Some(&format!("{name}: {yaml}"))),
        None => put(text, name, None),
    }
}

/// `text` without the property `name`.
pub fn remove(text: &str, name: &str) -> String {
    put(text, name, None)
}

/// The property's lines exactly as written, for putting them back later.
pub fn entry(text: &str, name: &str) -> Option<String> {
    let block = block(text)?;
    let e = block.entries.iter().find(|e| e.name == name)?;
    Some(text[e.lines.clone()].trim_end_matches('\n').trim_end_matches('\r').to_owned())
}

/// `text` with the property `name` written as `lines` (no line ending at the end), or removed.
/// A property already there is replaced where it stands; a new one goes last in the block, and a
/// note without a block gets one.
pub fn put(text: &str, name: &str, lines: Option<&str>) -> String {
    let Some(block) = block(text) else {
        return match lines {
            Some(lines) => format!("---\n{lines}\n---\n{text}"),
            None => text.to_owned(),
        };
    };
    let nl = block.newline;
    let existing = block.entries.iter().find(|e| e.name == name);
    let mut out = String::with_capacity(text.len() + 32);
    match (existing, lines) {
        (Some(e), Some(lines)) => {
            out.push_str(&text[..e.lines.start]);
            out.push_str(&lines.replace('\n', nl));
            out.push_str(nl);
            out.push_str(&text[e.lines.end..]);
        }
        (None, Some(lines)) => {
            out.push_str(&text[..block.inner.end]);
            out.push_str(&lines.replace('\n', nl));
            out.push_str(nl);
            out.push_str(&text[block.inner.end..]);
        }
        (Some(e), None) => {
            let left = format!("{}{}", &text[block.inner.start..e.lines.start], &text[e.lines.end..block.inner.end]);
            if left.trim().is_empty() {
                out.push_str(&text[block.range.end..]);
            } else {
                out.push_str(&text[..e.lines.start]);
                out.push_str(&text[e.lines.end..]);
            }
        }
        (None, None) => out.push_str(text),
    }
    out
}

/// `text` with the property `from` named `to`, its value left as written. Nothing changes when
/// the note has no `from`, or already has a `to`.
pub fn rename(text: &str, from: &str, to: &str) -> String {
    let Some(block) = block(text) else { return text.to_owned() };
    if block.entries.iter().any(|e| e.name == to) {
        return text.to_owned();
    }
    let Some(e) = block.entries.iter().find(|e| e.name == from) else { return text.to_owned() };
    format!("{}{to}{}", &text[..e.key.start], &text[e.key.end..])
}

/// A property name as typed into the app: one line, no leading or trailing space, nothing that
/// would make the line read as something other than `name: value`.
pub fn clean_name(name: &str) -> String {
    let one_line: String = name.chars().map(|c| if c.is_control() || c == ':' { ' ' } else { c }).collect();
    one_line
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_start_matches(['#', '-', '"', '\''])
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE: &str = "---\nstatus: Doing\ndue: 2026-11-14\ntags: [work, q4]\ndone: false\n---\n# Plan the release\n";

    fn scalar(text: &str) -> Raw {
        Raw::Scalar { text: text.into(), quoted: false }
    }

    #[test]
    fn the_block_reads_into_typed_values() {
        let props = read(NOTE);
        assert_eq!(props.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["status", "due", "tags", "done"]);
        assert_eq!(get(NOTE, "status"), Some(scalar("Doing")));
        assert_eq!(get(NOTE, "tags"), Some(Raw::List(vec!["work".into(), "q4".into()])));
        let due = get(NOTE, "due").unwrap();
        assert_eq!(Value::read(&due, PropType::Date), Some(Value::Date(jiff::civil::date(2026, 11, 14))));
        assert_eq!(Value::read(&get(NOTE, "done").unwrap(), PropType::Checkbox), Some(Value::Checkbox(false)));
        assert_eq!(due.infer(), Some(PropType::Date));
        assert_eq!(get(NOTE, "tags").unwrap().infer(), Some(PropType::MultiSelect));
        assert_eq!(body(NOTE), "# Plan the release\n");
    }

    #[test]
    fn no_block_without_both_fences() {
        assert_eq!(block_len("---\nstatus: open\n# No close"), 0);
        assert_eq!(block_len("text\n---\na: b\n---\n"), 0, "only at the very start");
        assert_eq!(block_len("--- \na: b\n---\n"), 0, "a fence is exactly three dashes");
        assert!(read("# Title\n\nbody").is_empty());
    }

    #[test]
    fn quoted_and_odd_values_read_back_as_written() {
        let text = "---\na: \"12\"\nb: 'it''s'\nc: \"say \\\"hi\\\"\"\nd: plain # comment\ne:\nf: [\"x, y\", z]\n---\n";
        assert_eq!(get(text, "a"), Some(Raw::Scalar { text: "12".into(), quoted: true }));
        assert_eq!(get(text, "a").unwrap().infer(), Some(PropType::Text), "quoted numbers are text");
        assert_eq!(get(text, "b"), Some(Raw::Scalar { text: "it's".into(), quoted: true }));
        assert_eq!(get(text, "c"), Some(Raw::Scalar { text: "say \"hi\"".into(), quoted: true }));
        assert_eq!(get(text, "d"), Some(scalar("plain")));
        assert_eq!(get(text, "e"), Some(scalar("")));
        assert_eq!(get(text, "f"), Some(Raw::List(vec!["x, y".into(), "z".into()])));
    }

    #[test]
    fn block_lists_read_and_nested_maps_are_kept_unmodelled() {
        let text = "---\ntags:\n  - one\n  - two\nmeta:\n  author: me\nnext: 1\n---\nbody";
        assert_eq!(get(text, "tags"), Some(Raw::List(vec!["one".into(), "two".into()])));
        assert_eq!(get(text, "meta"), Some(Raw::Other));
        assert_eq!(get(text, "next"), Some(scalar("1")));
    }

    #[test]
    fn writing_one_property_touches_only_its_line() {
        let text = "---\n# a comment\nstatus: Doing\nmeta:\n  nested: [1, 2]\nlist:\n- a\n- b\n---\n# Plan\n";
        let out = set(text, "status", &Value::Text("Done".into()));
        assert_eq!(out, text.replace("status: Doing", "status: Done"));
        let out = set(text, "list", &Value::List(vec!["c".into()]));
        assert_eq!(out, "---\n# a comment\nstatus: Doing\nmeta:\n  nested: [1, 2]\nlist: [c]\n---\n# Plan\n");
        let out = set(text, "due", &Value::Date(jiff::civil::date(2026, 1, 2)));
        assert_eq!(out, text.replace("- b\n---", "- b\ndue: 2026-01-02\n---"), "new ones go last");
    }

    #[test]
    fn crlf_blocks_stay_crlf() {
        let text = "---\r\na: 1\r\n---\r\nbody";
        assert_eq!(set(text, "b", &Value::Checkbox(true)), "---\r\na: 1\r\nb: true\r\n---\r\nbody");
        assert_eq!(set(text, "a", &Value::Number(2.)), "---\r\na: 2\r\n---\r\nbody");
    }

    #[test]
    fn an_empty_value_removes_the_line_and_an_empty_block_goes() {
        let text = "---\na: 1\nb: 2\n---\n# T\n";
        assert_eq!(set(text, "a", &Value::Text(String::new())), "---\nb: 2\n---\n# T\n");
        assert_eq!(remove(&remove(text, "a"), "b"), "# T\n");
        let added = set("# T\n", "a", &Value::Number(-3.5));
        assert_eq!(added, "---\na: -3.5\n---\n# T\n");
        assert_eq!(remove(&added, "a"), "# T\n", "adding then removing gives the note back byte for byte");
        let kept = "---\n# keep me\na: 1\n---\nx";
        assert_eq!(remove(kept, "a"), "---\n# keep me\n---\nx", "a comment is a line left");
    }

    #[test]
    fn strings_are_quoted_when_plain_would_read_as_something_else() {
        let yaml = |s: &str| Value::Text(s.into()).yaml().unwrap();
        assert_eq!(yaml("Doing"), "Doing");
        assert_eq!(yaml("true"), "\"true\"");
        assert_eq!(yaml("12"), "\"12\"");
        assert_eq!(yaml("2026-11-14"), "\"2026-11-14\"");
        assert_eq!(yaml(" padded"), "\" padded\"");
        assert_eq!(yaml("a: b"), "\"a: b\"");
        assert_eq!(yaml("[x"), "\"[x\"");
        assert_eq!(yaml("#tag"), "\"#tag\"");
        assert_eq!(yaml("- item"), "\"- item\"");
        assert_eq!(yaml("say \"hi\""), "say \"hi\"");
        assert_eq!(yaml("\"hi\""), "\"\\\"hi\\\"\"");
        assert_eq!(
            Value::List(vec!["one".into(), "a, b".into(), "x]".into()]).yaml().unwrap(),
            "[one, \"a, b\", \"x]\"]"
        );
        assert_eq!(Value::Number(12.).yaml().unwrap(), "12");
        for s in ["true", "12", " x", "a: b", "#t", "-", "x, y", "\"q\"", "tab\there"] {
            let text = set("", "p", &Value::Text(s.into()));
            assert_eq!(get(&text, "p").map(|r| r.display()), Some(s.to_owned()), "{s:?} reads back from {text:?}");
            let text = set("", "p", &Value::List(vec![s.into(), "z".into()]));
            assert_eq!(get(&text, "p"), Some(Raw::List(vec![s.into(), "z".into()])), "{s:?} in a list: {text:?}");
        }
    }

    #[test]
    fn renaming_keeps_the_value_bytes() {
        let text = "---\nstatus:   \"Doing\"  # why\nother: x\n---\n";
        assert_eq!(rename(text, "status", "state"), "---\nstate:   \"Doing\"  # why\nother: x\n---\n");
        assert_eq!(rename(text, "status", "other"), text, "never into a name the note already has");
        assert_eq!(rename(text, "missing", "x"), text);
    }

    #[test]
    fn an_entry_goes_back_exactly_as_it_was() {
        let text = "---\ntags:\n  - a\n  - b\nx: 1\n---\n";
        let saved = entry(text, "tags").unwrap();
        assert_eq!(saved, "tags:\n  - a\n  - b");
        let changed = set(text, "tags", &Value::List(vec!["c".into()]));
        assert_eq!(put(&changed, "tags", Some(&saved)), text);
        let removed = remove(text, "tags");
        assert_eq!(removed, "---\nx: 1\n---\n");
    }

    #[test]
    fn names_from_the_app_stay_on_their_line() {
        assert_eq!(clean_name("  due: date \n"), "due date");
        assert_eq!(clean_name("# tag"), "tag");
        assert_eq!(clean_name(""), "");
    }
}
