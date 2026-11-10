//! Ask: a question about the notes goes to the model with the library as tools, and the answer
//! comes back streaming, citing the notes it rests on as `[[path]]`.

use std::sync::Arc;

use serde_json::Value;

use super::agent::{self, Progress};
use super::chat::{AiError, Cancel, Choice, Message, Request};
use super::settings::{AssistantSettings, Connect};
use super::tools::LibraryTools;
use crate::library::{Library, NoteId};

const ASK_PROMPT: &str = "\
You answer questions about the user's notes, using only what the notes say.

Search with search_notes, more than once with other words when the first results miss, and read \
the notes that look relevant with read_note before you answer.

- Answer from the notes only. When they do not hold the answer, say plainly that your notes do \
not say, and name what you found that comes close, if anything. Never fill a gap from general \
knowledge.
- Right after each claim, cite the note it comes from as [[path]], with the exact path \
search_notes returned, like [[Projects/Meeting notes.md]].
- Be brief: a sentence or two, or a short list. Markdown bold and lists are fine; no headings.";

/// What a run reports while it works and when it ends.
#[derive(Clone, Debug, PartialEq)]
pub enum AskEvent {
    Text(String),
    /// What the model is doing between answers: searching, reading.
    Activity(String),
    /// The text so far led up to a tool call; the answer starts over.
    Restart,
    /// The run's new messages (the model's turns and tool results), the answer last.
    Done(Vec<Message>),
    Failed(AiError),
}

pub fn request(history: &[Message], question: &str, today: &str) -> Request {
    let mut messages = history.to_vec();
    messages.push(Message::user(question));
    Request {
        system: format!("{ASK_PROMPT}\n\nToday is {today}."),
        messages,
        tools: Vec::new(),
        choice: Choice::Auto,
        max_tokens: 16_000,
    }
}

/// Runs the question on its own thread; every event goes to `send`.
pub fn spawn(
    connect: Arc<dyn Connect>,
    settings: AssistantSettings,
    library: Arc<Library>,
    request: Request,
    cancel: Cancel,
    send: impl Fn(AskEvent) + Send + 'static,
) {
    let _ = std::thread::Builder::new().name("formalnotes-ask".into()).spawn(move || {
        let titles = library.clone();
        let result = connect.connect(&settings).and_then(|transport| {
            let mut tools = LibraryTools::new(library);
            agent::run(transport.as_ref(), request, &mut tools, &cancel, &mut |progress| {
                send(match progress {
                    Progress::Text(text) => AskEvent::Text(text),
                    Progress::Restart => AskEvent::Restart,
                    Progress::Tool { name, input } => AskEvent::Activity(activity(&name, &input, &titles)),
                })
            })
        });
        send(match result {
            Ok(messages) => AskEvent::Done(messages),
            Err(error) => AskEvent::Failed(error),
        });
    });
}

fn activity(tool: &str, input: &Value, library: &Library) -> String {
    let field = |name: &str| input.get(name).and_then(Value::as_str).unwrap_or_default().trim().to_owned();
    match tool {
        "search_notes" => format!("Searching for “{}”", field("query")),
        "read_note" => {
            let path = field("path");
            let title = library.note(&NoteId(path.clone())).map_or(path, |n| n.title.clone());
            format!("Reading “{title}”")
        }
        _ => "Looking through your folders".to_owned(),
    }
}

/// A piece of an answer: text, or a citation as the model wrote it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Piece<'a> {
    Text(&'a str),
    Cite(&'a str),
}

/// The answer split at its `[[...]]` citations. An unclosed one at the end (still streaming)
/// is held back.
pub fn pieces(text: &str) -> Vec<Piece<'_>> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find("[[") {
        if open > 0 {
            out.push(Piece::Text(&rest[..open]));
        }
        let after = &rest[open + 2..];
        match after.find("]]") {
            Some(close) => {
                out.push(Piece::Cite(after[..close].trim()));
                rest = &after[close + 2..];
            }
            None => return out,
        }
    }
    if !rest.is_empty() {
        out.push(Piece::Text(rest));
    }
    out
}

/// The note a citation names: its exact path, the path without `.md`, or a title.
pub fn resolve(library: &Library, cite: &str) -> Option<NoteId> {
    let cite = cite.trim().trim_start_matches('/');
    let exact = NoteId(cite.to_owned());
    if library.note(&exact).is_some() {
        return Some(exact);
    }
    let with_ext = NoteId(format!("{cite}.md"));
    if library.note(&with_ext).is_some() {
        return Some(with_ext);
    }
    let lower = cite.trim_end_matches(".md").to_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    library
        .notes
        .values()
        .find(|n| n.title.to_lowercase() == name || n.id.stem().to_lowercase() == name)
        .map(|n| n.id.clone())
}

/// The notes an answer cites that exist, in the order it first cites them.
pub fn sources(library: &Library, answer: &str) -> Vec<NoteId> {
    let mut out: Vec<NoteId> = Vec::new();
    for piece in pieces(answer) {
        if let Piece::Cite(cite) = piece
            && let Some(id) = resolve(library, cite)
            && !out.contains(&id)
        {
            out.push(id);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use jiff::Timestamp;

    use super::*;
    use crate::library::Note;

    #[test]
    fn citations_split_out_and_a_half_one_waits() {
        assert_eq!(
            pieces("Soon [[a.md]] and [[ b/c.md ]]."),
            [Piece::Text("Soon "), Piece::Cite("a.md"), Piece::Text(" and "), Piece::Cite("b/c.md"), Piece::Text(".")]
        );
        assert_eq!(pieces("Soon [[a.m"), [Piece::Text("Soon ")]);
    }

    #[test]
    fn sources_resolve_paths_names_and_titles_once_each() {
        let mut library = Library::default();
        for (path, text) in [("Projects/Meeting notes.md", "# Meeting notes"), ("Ideas.md", "# Big ideas")] {
            let note = Note::new(NoteId(path.into()), Arc::from(text), Timestamp::UNIX_EPOCH, Timestamp::UNIX_EPOCH);
            library.insert_note(note);
        }
        let answer = "A [[Projects/Meeting notes.md]] B [[Ideas]] C [[Meeting notes]] D [[Missing.md]] E [[big ideas]]";
        assert_eq!(sources(&library, answer), [NoteId("Projects/Meeting notes.md".into()), NoteId("Ideas.md".into())]);
    }
}
