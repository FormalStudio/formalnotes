//! Automations: a note that matches a trigger goes to the model with the shape of the library,
//! and comes back as a plan of notes the app files through the store. "Organize brain dumps"
//! is the one that ships; another is a new [`Automation`] value.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use jiff::{SignedDuration, Timestamp};
use nd::prelude::*;
use serde_json::{Value, json};

use super::chat::{AiError, Cancel, Choice, Message, Request, ToolSpec, Transport};
use crate::library::model::{compose, file_stem_for, split};
use crate::library::properties::{self, PropType, Raw, clean_name};
use crate::library::view::Column;
use crate::library::{EntryId, FolderId, Library, Note, NoteId, Store};
use crate::settings::NotesSettings;

/// Something the app does with a note on its own, through the model.
pub struct Automation {
    pub id: &'static str,
    pub name: &'static str,
    /// The model's instructions. The user's extra instructions follow them.
    pub instructions: &'static str,
    /// What it watches under the settings in effect; `None` while it is off.
    pub trigger: fn(&NotesSettings) -> Option<Trigger>,
}

/// Which notes an automation picks up: those resting directly in `folder`, unedited for `idle`.
#[derive(Clone, Debug, PartialEq)]
pub struct Trigger {
    pub folder: FolderId,
    pub idle: SignedDuration,
    pub extra: String,
}

pub const ORGANIZE_BRAIN_DUMPS: Automation = Automation {
    id: "organize-brain-dumps",
    name: "Organize brain dumps",
    instructions: ORGANIZE_PROMPT,
    trigger: |settings| {
        let dumps = &settings.brain_dumps;
        let folder = dumps.inbox_folder.trim().trim_matches('/');
        (dumps.organize && !folder.is_empty()).then(|| Trigger {
            folder: FolderId(folder.to_owned()),
            idle: SignedDuration::from_mins(i64::from(dumps.idle_minutes.max(1))),
            extra: dumps.extra_instructions.trim().to_owned(),
        })
    },
};

pub const AUTOMATIONS: [&Automation; 1] = [&ORGANIZE_BRAIN_DUMPS];

const ORGANIZE_PROMPT: &str = "\
You organize a brain dump from the user's notes app into tidy notes in their library.

You get the dump and the library's folders, with the titles of the notes in each folder and the \
properties (columns) a folder's notes carry. Call the file_notes tool exactly once with the notes \
to write. Do not answer in text.

- Split the dump into one note per distinct subject. A dump about one thing stays one note.
- Put each note in the existing folder it belongs with. Make a new folder only when nothing \
fits, and name it the way the existing folders are named. Use \"\" for the top level.
- Keep every fact, name, number, date, link and task from the dump, and keep the author's own \
wording. Fix only obvious typos. Do not add facts, advice or commentary.
- Give each note a short title in the author's voice. The body is Markdown without the title \
heading: tasks stay \"- [ ]\" items, lists stay lists. Do not reuse the title of a note that \
already exists.
- Fill a folder's columns as properties when the dump says the value. Leave out what it does \
not say.
- Never file into the inbox folder the dump came from.";

/// Whether a note is due for the trigger: directly in its folder, not empty, not open in the
/// editor, saved, and unedited for the trigger's idle time.
pub fn due(note: &Note, trigger: &Trigger, open: Option<&NoteId>, dirty: bool, now: Timestamp) -> bool {
    note.id.folder() == trigger.folder
        && !note.text.trim().is_empty()
        && open != Some(&note.id)
        && !dirty
        && now.duration_since(note.modified) >= trigger.idle
}

pub fn text_hash(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// How much of the folder list (paths, titles, columns) the model sees, in characters.
const CONTEXT_BUDGET: usize = 8_000;
const TITLES_PER_FOLDER: usize = 40;

/// The library's folders for the model: each with its columns and the titles of its notes,
/// trimmed to the budget. The inbox itself is left out.
pub fn library_outline(library: &Library, inbox: &FolderId) -> String {
    let mut folders: Vec<FolderId> = vec![FolderId::root()];
    folders.extend(library.folders.keys().filter(|f| !inbox.contains(&f.0) || inbox.is_root()).cloned());
    let mut out = String::from("Folders, each with the titles of the notes in it:\n");
    for folder in folders {
        let mut titles: Vec<&str> = library
            .notes
            .values()
            .filter(|n| n.id.folder() == folder && !(folder == *inbox))
            .map(|n| n.title.as_str())
            .collect();
        titles.sort_unstable();
        let more = titles.len().saturating_sub(TITLES_PER_FOLDER);
        titles.truncate(TITLES_PER_FOLDER);
        let name = if folder.is_root() { "\"\" (top level)".to_owned() } else { format!("\"{}\"", folder.0) };
        let mut entry =
            format!("- {name}: {}", if titles.is_empty() { "(empty)".to_owned() } else { titles.join("; ") });
        if more > 0 {
            entry.push_str(&format!("; and {more} more"));
        }
        let cols = library.view(&folder).map_or(&[][..], |v| v.columns.as_slice());
        if !cols.is_empty() {
            let described: Vec<String> = cols
                .iter()
                .map(|c| {
                    if c.options.is_empty() {
                        format!("{} ({})", c.name, c.kind)
                    } else {
                        format!("{} ({}: {})", c.name, c.kind, c.options.join(", "))
                    }
                })
                .collect();
            entry.push_str(&format!("\n  columns: {}", described.join(", ")));
        }
        if out.len() + entry.len() > CONTEXT_BUDGET {
            out.push_str("- (more folders not shown)\n");
            break;
        }
        out.push_str(&entry);
        out.push('\n');
    }
    out
}

fn file_notes_tool() -> ToolSpec {
    ToolSpec {
        name: "file_notes".into(),
        description: "Write the organized notes. Each goes into a folder (existing or new) with a title, a Markdown \
                      body and the properties the dump gives values for."
            .into(),
        schema: json!({
            "type": "object",
            "properties": {
                "notes": {
                    "type": "array",
                    "minItems": 1,
                    "items": {
                        "type": "object",
                        "properties": {
                            "folder": { "type": "string", "description": "Folder path like \"Projects/Launch\", or \"\" for the top level." },
                            "title": { "type": "string" },
                            "body": { "type": "string", "description": "Markdown, without the title heading." },
                            "properties": {
                                "type": "array",
                                "items": {
                                    "type": "object",
                                    "properties": { "name": { "type": "string" }, "value": { "type": "string" } },
                                    "required": ["name", "value"],
                                },
                            },
                        },
                        "required": ["folder", "title", "body"],
                    },
                },
            },
            "required": ["notes"],
        }),
    }
}

/// What the model asked for.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub notes: Vec<Planned>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Planned {
    pub folder: FolderId,
    pub title: String,
    pub body: String,
    pub properties: Vec<(String, String)>,
}

/// Reads and checks the `file_notes` input. Folder paths are made safe; a note aimed at the
/// inbox goes to the top level instead, so it is never picked up again.
pub fn parse_plan(input: &Value, inbox: &FolderId) -> Result<Plan, String> {
    let items = input.get("notes").and_then(Value::as_array).ok_or("no notes")?;
    let mut notes = Vec::new();
    for item in items {
        let field = |name: &str| item.get(name).and_then(Value::as_str).unwrap_or_default().trim().to_owned();
        let title = field("title");
        let body = field("body");
        if title.is_empty() || body.is_empty() {
            return Err("a note without a title or a body".into());
        }
        let folder = safe_folder(&field("folder"))?;
        let folder = if within(&folder, inbox) { FolderId::root() } else { folder };
        let properties = item
            .get("properties")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|p| {
                let name = p.get("name")?.as_str()?.trim().to_owned();
                let value = match p.get("value")? {
                    Value::String(s) => s.trim().to_owned(),
                    other => other.to_string(),
                };
                (!name.is_empty()).then_some((name, value))
            })
            .collect();
        notes.push(Planned { folder, title, body, properties });
    }
    if notes.is_empty() {
        return Err("no notes".into());
    }
    Ok(Plan { notes })
}

/// Whether `folder` is the inbox or inside it, compared as the default macOS and Windows file
/// systems compare names.
fn within(folder: &FolderId, inbox: &FolderId) -> bool {
    let (folder, inbox) = (folder.0.to_lowercase(), inbox.0.to_lowercase());
    !inbox.is_empty() && (folder == inbox || folder.starts_with(&format!("{inbox}/")))
}

fn safe_folder(path: &str) -> Result<FolderId, String> {
    let mut parts = Vec::new();
    for part in path.split(['/', '\\']).map(str::trim).filter(|p| !p.is_empty()) {
        if part.starts_with('.') {
            return Err(format!("folder \"{path}\" is not allowed"));
        }
        parts.push(file_stem_for(part));
    }
    Ok(FolderId(parts.join("/")))
}

/// Everything a run sends, taken on the main thread.
pub struct Job {
    pub automation: &'static Automation,
    pub note: NoteId,
    pub text: Arc<str>,
    pub hash: u64,
    pub inbox: FolderId,
    pub extra: String,
    pub outline: String,
}

impl Job {
    pub fn new(
        automation: &'static Automation,
        note: &Note,
        library: &Library,
        inbox: FolderId,
        extra: String,
    ) -> Self {
        Self {
            automation,
            note: note.id.clone(),
            text: note.text.clone(),
            hash: text_hash(&note.text),
            outline: library_outline(library, &inbox),
            inbox,
            extra,
        }
    }

    pub fn request(&self) -> Request {
        let mut system = self.automation.instructions.to_owned();
        if !self.extra.is_empty() {
            system.push_str("\n\nThe user's own instructions, which win over the ones above:\n");
            system.push_str(&self.extra);
        }
        let user = format!(
            "The dump, from \"{}\" in the inbox folder \"{}\":\n\n<dump>\n{}\n</dump>\n\n{}",
            self.note.0, self.inbox.0, self.text, self.outline
        );
        Request {
            system,
            messages: vec![Message::user(user)],
            tools: vec![file_notes_tool()],
            choice: Choice::Required("file_notes"),
            max_tokens: 32_000,
        }
    }

    /// Asks the model for the plan. Runs off the main thread.
    pub fn run(&self, transport: &dyn Transport, cancel: &Cancel) -> Result<Plan, AiError> {
        let reply = transport.send(&self.request(), &mut |_| {}, cancel)?;
        let input = reply
            .message
            .tool_uses()
            .find(|(_, name, _)| *name == "file_notes")
            .map(|(_, _, input)| input.clone())
            .ok_or_else(|| AiError::BadReply("the model did not file the note".into()))?;
        parse_plan(&input, &self.inbox).map_err(AiError::BadReply)
    }
}

/// What a plan did to the library, for its notice and its undo.
#[derive(Clone, Debug, PartialEq)]
pub struct Applied {
    pub source: NoteId,
    pub source_title: String,
    /// The source's name in the trash.
    pub trashed: String,
    pub created: Vec<NoteId>,
    pub new_folders: Vec<FolderId>,
}

/// Files the plan's notes and moves the source to the trash, all through the store. Refuses
/// when the source is gone or was edited since it was sent.
pub fn apply(
    store: &mut Store,
    source: &NoteId,
    hash: u64,
    plan: &Plan,
    cx: &mut Context<Store>,
) -> Result<Applied, String> {
    let note = store.note(source).ok_or("the note is gone")?;
    if text_hash(&note.text) != hash {
        return Err("the note changed while it was being organized".into());
    }
    let source_title = note.title.clone();
    let mut created = Vec::new();
    let mut new_folders = Vec::new();
    for planned in &plan.notes {
        let folder = ensure_folder(store, &planned.folder, &mut new_folders, cx);
        let id = store.create_note(&folder, cx);
        let id = store.retitle(&id, &planned.title, cx);
        let body = split(&planned.body).body;
        let text = format!("{}\n", compose("", Some(&planned.title), body.trim_end()));
        let columns = store.view(&folder).columns;
        let text = with_properties(text, &planned.properties, &columns);
        store.edit(&id, text, cx);
        created.push(id);
    }
    let trashed = store.trash(&EntryId::Note(source.clone()), cx).ok_or("the note could not be moved to the trash")?;
    Ok(Applied { source: source.clone(), source_title, trashed, created, new_folders })
}

/// `text` with each property (name, value) written as its column's type says; a property no
/// column names, or a value that does not read as its column's type, is text.
fn with_properties(mut text: String, properties: &[(String, String)], columns: &[Column]) -> String {
    for (name, value) in properties {
        let (name, value) = (clean_name(name), value.trim());
        if name.is_empty() || value.is_empty() {
            continue;
        }
        let kind = columns.iter().find(|c| c.name == name).map_or(PropType::Text, Column::prop_type);
        let raw = if kind == PropType::MultiSelect {
            let items = value.trim_start_matches('[').trim_end_matches(']').split(',');
            Raw::List(items.map(|i| i.trim().to_owned()).filter(|i| !i.is_empty()).collect())
        } else {
            Raw::Scalar { text: value.to_owned(), quoted: false }
        };
        let value = properties::Value::read(&raw, kind).unwrap_or_else(|| properties::Value::Text(value.to_owned()));
        text = properties::set(&text, &name, &value);
    }
    text
}

/// The folder at `path`, matched case-insensitively to one that exists, made where missing.
fn ensure_folder(store: &mut Store, path: &FolderId, made: &mut Vec<FolderId>, cx: &mut Context<Store>) -> FolderId {
    let mut at = FolderId::root();
    for part in path.0.split('/').filter(|p| !p.is_empty()) {
        let wanted = at.join(part).to_lowercase();
        let existing = store.library().folders.keys().find(|f| f.0.to_lowercase() == wanted).cloned();
        at = match existing {
            Some(folder) => folder,
            None => {
                let fresh = store.create_folder(&at, cx);
                let named = store.rename_folder(&fresh, part, cx);
                made.push(named.clone());
                named
            }
        };
    }
    at
}

/// Takes a plan back: its notes and the folders it made (once empty) go to the trash, and the
/// source comes back. Returns where the source is now.
pub fn undo(store: &mut Store, applied: &Applied, cx: &mut Context<Store>) -> Option<EntryId> {
    for id in &applied.created {
        store.trash(&EntryId::Note(id.clone()), cx);
    }
    for folder in applied.new_folders.iter().rev() {
        if store.library().has_folder(folder) && store.library().count(folder) == 0 {
            store.trash(&EntryId::Folder(folder.clone()), cx);
        }
    }
    store.restore(&applied.trashed, cx)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(path: &str, text: &str, modified: Timestamp) -> Note {
        Note::new(NoteId(path.into()), Arc::from(text), modified, modified)
    }

    fn trigger() -> Trigger {
        Trigger { folder: FolderId("Inbox".into()), idle: SignedDuration::from_mins(5), extra: String::new() }
    }

    #[test]
    fn a_dump_is_due_once_it_has_rested() {
        let edited = Timestamp::from_second(1_000_000).unwrap_or(Timestamp::UNIX_EPOCH);
        let dump = note("Inbox/dump.md", "call the bank", edited);
        let at = |mins: i64| edited + SignedDuration::from_mins(mins);
        assert!(!due(&dump, &trigger(), None, false, at(4)), "too fresh");
        assert!(due(&dump, &trigger(), None, false, at(5)));
        assert!(!due(&dump, &trigger(), Some(&dump.id), false, at(60)), "open in the editor");
        assert!(!due(&dump, &trigger(), None, true, at(60)), "unsaved edits");
        assert!(!due(&note("Inbox/empty.md", "  \n", edited), &trigger(), None, false, at(60)), "empty");
        assert!(!due(&note("Inbox/Sub/x.md", "x", edited), &trigger(), None, false, at(60)), "not directly inside");
        assert!(!due(&note("Ideas.md", "x", edited), &trigger(), None, false, at(60)), "elsewhere");
    }

    #[test]
    fn plans_are_checked_and_kept_out_of_the_inbox() {
        let inbox = FolderId("Inbox".into());
        let input = json!({ "notes": [
            { "folder": "/Projects//Launch/", "title": "Plan", "body": "Ship it", "properties": [{ "name": "status", "value": "Todo" }, { "name": "n", "value": 3 }] },
            { "folder": "inbox/Later", "title": "Later", "body": "x" },
        ]});
        let plan = parse_plan(&input, &inbox).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(plan.notes[0].folder, FolderId("Projects/Launch".into()));
        assert_eq!(plan.notes[0].properties, [("status".into(), "Todo".into()), ("n".into(), "3".into())]);
        assert!(plan.notes[1].folder.is_root(), "inside the inbox, whatever the case");
        let into_inbox = json!({ "notes": [{ "folder": "Inbox", "title": "T", "body": "B" }] });
        assert!(parse_plan(&into_inbox, &inbox).is_ok_and(|p| p.notes[0].folder.is_root()));
        assert!(parse_plan(&json!({ "notes": [] }), &inbox).is_err());
        assert!(parse_plan(&json!({ "notes": [{ "folder": "../x", "title": "T", "body": "B" }] }), &inbox).is_err());
        assert!(parse_plan(&json!({ "notes": [{ "folder": "", "title": "", "body": "B" }] }), &inbox).is_err());
    }

    #[test]
    fn properties_are_written_as_their_columns_type() {
        let props = [
            ("status".to_owned(), "Doing".to_owned()),
            ("due".to_owned(), "2026-11-14".to_owned()),
            ("tags".to_owned(), "work, q4".to_owned()),
            ("done".to_owned(), "false".to_owned()),
            ("note".to_owned(), "12".to_owned()),
            ("empty".to_owned(), " ".to_owned()),
        ];
        let columns = [
            Column::new("due", PropType::Date),
            Column::new("tags", PropType::MultiSelect),
            Column::new("done", PropType::Checkbox),
        ];
        assert_eq!(
            with_properties("# Plan\n".into(), &props, &columns),
            "---\nstatus: Doing\ndue: 2026-11-14\ntags: [work, q4]\ndone: false\nnote: \"12\"\n---\n# Plan\n"
        );
        assert_eq!(with_properties("# Plan\n".into(), &[], &[]), "# Plan\n");
    }

    #[test]
    fn the_outline_lists_folders_titles_and_columns_within_budget() {
        let mut library = Library::default();
        for folder in ["Projects", "Inbox"] {
            library.insert_folder(crate::library::Folder {
                id: FolderId(folder.into()),
                created: Timestamp::UNIX_EPOCH,
                modified: Timestamp::UNIX_EPOCH,
            });
        }
        library.insert_note(note("Projects/Roadmap.md", "# Roadmap", Timestamp::UNIX_EPOCH));
        library.insert_note(note("Inbox/dump.md", "secret dump", Timestamp::UNIX_EPOCH));
        let mut status = Column::new("status", PropType::Select);
        status.options = vec!["Todo".into(), "Done".into()];
        let view = crate::library::FolderView { columns: vec![status], ..Default::default() };
        library.views.insert(FolderId("Projects".into()), view);
        let outline = library_outline(&library, &FolderId("Inbox".into()));
        assert!(outline.contains("\"Projects\": Roadmap"), "{outline}");
        assert!(outline.contains("status (select: Todo, Done)"), "{outline}");
        assert!(!outline.contains("Inbox"), "the inbox is not a destination: {outline}");
        for i in 0..2000 {
            library.insert_note(note(&format!("Projects/Note {i}.md"), "x", Timestamp::UNIX_EPOCH));
        }
        assert!(library_outline(&library, &FolderId("Inbox".into())).len() <= CONTEXT_BUDGET + 200);
    }
}
