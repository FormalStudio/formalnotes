//! The library as tools the model can call: search, read a note, list the folders. They read a
//! snapshot taken when the question was asked.

use std::sync::Arc;

use serde_json::{Value, json};

use super::agent::Tools;
use super::chat::ToolSpec;
use super::search;
use crate::library::{FolderId, Library, NoteId};

const SEARCH_HITS: usize = 8;
/// The longest note text a read returns, in characters; the rest is cut with a marker.
const READ_LIMIT: usize = 24_000;

pub struct LibraryTools {
    library: Arc<Library>,
}

impl LibraryTools {
    pub fn new(library: Arc<Library>) -> Self {
        Self { library }
    }

    fn search(&self, input: &Value) -> Result<String, String> {
        let query = input.get("query").and_then(Value::as_str).unwrap_or_default();
        if query.trim().is_empty() {
            return Err("query is empty".into());
        }
        let hits = search::search(&self.library, query, SEARCH_HITS);
        if hits.is_empty() {
            return Ok(format!("No notes match \"{query}\". Try other words, or list_folders."));
        }
        let hits: Vec<Value> =
            hits.into_iter().map(|h| json!({ "path": h.id.0, "title": h.title, "snippet": h.snippet })).collect();
        Ok(Value::Array(hits).to_string())
    }

    fn read(&self, input: &Value) -> Result<String, String> {
        let path = input.get("path").and_then(Value::as_str).unwrap_or_default().trim().trim_start_matches('/');
        let note = self.library.note(&NoteId(path.to_owned())).ok_or_else(|| format!("No note at \"{path}\"."))?;
        let text: String = note.text.chars().take(READ_LIMIT).collect();
        let cut = note.text.chars().count() > READ_LIMIT;
        Ok(format!(
            "path: {}\ntitle: {}\nmodified: {}\n\n{}{}",
            note.id.0,
            note.title,
            note.modified.strftime("%Y-%m-%d"),
            text,
            if cut { "\n\n[cut here: the note goes on]" } else { "" }
        ))
    }

    fn folders(&self) -> String {
        let count = |folder: &FolderId| self.library.notes.keys().filter(|n| n.folder() == *folder).count();
        let mut lines = vec![format!("(Home): {} notes", count(&FolderId::root()))];
        lines.extend(self.library.folders.keys().map(|f| format!("{}: {} notes", f.0, count(f))));
        lines.join("\n")
    }
}

impl Tools for LibraryTools {
    fn specs(&self) -> Vec<ToolSpec> {
        vec![
            ToolSpec {
                name: "search_notes".into(),
                description:
                    "Search the user's notes by words or an exact phrase. Returns the best matches with their \
                              path, title and a snippet. Search more than once with different words when the first \
                              results miss."
                        .into(),
                schema: json!({
                    "type": "object",
                    "properties": { "query": { "type": "string", "description": "Words or a phrase to look for." } },
                    "required": ["query"],
                }),
            },
            ToolSpec {
                name: "read_note".into(),
                description: "Read one note in full by the path search_notes returned.".into(),
                schema: json!({
                    "type": "object",
                    "properties": { "path": { "type": "string", "description": "The note's path, ending in .md." } },
                    "required": ["path"],
                }),
            },
            ToolSpec {
                name: "list_folders".into(),
                description: "List every folder in the library with how many notes it holds.".into(),
                schema: json!({ "type": "object", "properties": {} }),
            },
        ]
    }

    fn run(&mut self, name: &str, input: &Value) -> Result<String, String> {
        match name {
            "search_notes" => self.search(input),
            "read_note" => self.read(input),
            "list_folders" => Ok(self.folders()),
            other => Err(format!("There is no tool named {other}.")),
        }
    }
}
