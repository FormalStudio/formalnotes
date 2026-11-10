#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Tier 1: AI without the network. Both wire formats against recorded streams, the tool loop
//! against a scripted provider, the organizer filing a plan through the store, and the
//! automations picking up a brain dump.

use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, SystemTime};

use formalnotes::ai::agent::{self, MAX_STEPS, Progress};
use formalnotes::ai::chat::{self, AiError, Block, Cancel, Choice, Message, Provider, Reply, Request, Role, Stop};
use formalnotes::ai::organize::{self, Job, ORGANIZE_BRAIN_DUMPS};
use formalnotes::ai::tools::LibraryTools;
use formalnotes::ai::{Automations, Connector, Outcome};
use formalnotes::library::{FolderId, NoteId, Store};
use formalnotes::settings::NotesSettings;
use jiff::{SignedDuration, Timestamp};
use nd::prelude::*;
use nd::test::TestApp;
use serde_json::{Value, json};
use support::{Fake, Script, answer, call};

mod support;

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ai").join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn no_cancel() -> Cancel {
    Arc::new(AtomicBool::new(false))
}

fn parse_text(provider: Provider, text: &str) -> (Result<Reply, AiError>, String) {
    let mut streamed = String::new();
    let reply = chat::read_stream(provider, Cursor::new(text.to_owned()), &mut |t| streamed.push_str(t), &no_cancel());
    (reply, streamed)
}

fn parse(provider: Provider, name: &str) -> (Result<Reply, AiError>, String) {
    parse_text(provider, &fixture(name))
}

#[test]
fn anthropic_text_streams_and_ends_the_turn() {
    let (reply, streamed) = parse(Provider::Anthropic, "anthropic_text.sse");
    let reply = reply.unwrap();
    assert_eq!(reply.stop, Stop::EndTurn);
    assert!(streamed.starts_with("Hello! How are you?"), "{streamed}");
    assert_eq!(reply.message.text(), streamed, "what streamed is the answer");
    assert_eq!(reply.message.role, Role::Assistant);
}

#[test]
fn anthropic_tool_use_is_rebuilt_from_its_json_deltas() {
    let (reply, streamed) = parse(Provider::Anthropic, "anthropic_tool.sse");
    let reply = reply.unwrap();
    assert_eq!(reply.stop, Stop::ToolUse);
    assert!(streamed.is_empty());
    let calls: Vec<_> =
        reply.message.tool_uses().map(|(id, name, input)| (id.to_owned(), name.to_owned(), input.clone())).collect();
    assert_eq!(
        calls,
        [("toolu_018XX5atzWwt6iA6S4QJB2oV".into(), "search_notes".into(), json!({ "query": "beta ship date" }))]
    );
}

#[test]
fn thinking_blocks_go_back_exactly_as_they_came() {
    let (reply, streamed) = parse(Provider::Anthropic, "anthropic_thinking_tool.sse");
    let reply = reply.unwrap();
    assert_eq!(streamed, "Let me search.", "thinking never shows as answer text");
    assert_eq!(reply.stop, Stop::ToolUse);
    let request = Request {
        system: "s".into(),
        messages: vec![
            Message::user("When does the beta ship?"),
            reply.message.clone(),
            Message {
                role: Role::User,
                content: vec![Block::ToolResult { id: "toolu_2".into(), content: "the 14th".into(), is_error: false }],
            },
        ],
        tools: Vec::new(),
        choice: Choice::Auto,
        max_tokens: 100,
    };
    let body = chat::body(Provider::Anthropic, &request, "claude-sonnet-5-5");
    assert_eq!(
        body["messages"][1]["content"],
        json!([
            { "type": "thinking", "thinking": "Look for the beta date.", "signature": "c2lnbmF0dXJl" },
            { "type": "text", "text": "Let me search." },
            { "type": "tool_use", "id": "toolu_2", "name": "read_note", "input": { "path": "Projects/Meeting notes.md" } },
        ])
    );
    assert_eq!(
        body["messages"][2]["content"][0],
        json!({ "type": "tool_result", "tool_use_id": "toolu_2", "content": "the 14th", "is_error": false })
    );
    assert_eq!(body["stream"], json!(true));
    assert_eq!(body["max_tokens"], json!(100));
    let openai = chat::body(Provider::OpenAi, &request, "gpt");
    assert!(!openai.to_string().contains("thinking"), "another wire format never sees Anthropic's blocks");
}

#[test]
fn anthropic_stream_errors_and_cut_streams_read_as_errors() {
    let (reply, _) = parse(Provider::Anthropic, "anthropic_overloaded.sse");
    assert_eq!(reply, Err(AiError::Server { status: 529, message: "Overloaded".into() }));
    let text = fixture("anthropic_text.sse");
    let cut = &text[..text.find("event: message_delta").unwrap()];
    assert!(matches!(parse_text(Provider::Anthropic, cut).0, Err(AiError::BadReply(_))));
}

#[test]
fn openai_text_and_tool_calls_stream() {
    let (reply, streamed) = parse(Provider::OpenAi, "openai_text.sse");
    let reply = reply.unwrap();
    assert_eq!(reply.stop, Stop::EndTurn);
    assert!(!streamed.is_empty());
    assert_eq!(reply.message.text(), streamed);

    let (reply, _) = parse(Provider::OpenAi, "openai_tool.sse");
    let reply = reply.unwrap();
    assert_eq!(reply.stop, Stop::ToolUse);
    let calls: Vec<_> =
        reply.message.tool_uses().map(|(id, name, input)| (id.to_owned(), name.to_owned(), input.clone())).collect();
    assert_eq!(
        calls,
        [("call_81on5mghfH1cB29vD9xdn648".into(), "search_notes".into(), json!({ "query": "beta ship" }))]
    );
}

#[test]
fn openai_requests_carry_tool_calls_results_and_a_forced_choice() {
    let request = Request {
        system: "Be brief.".into(),
        messages: vec![
            Message::user("q"),
            Message {
                role: Role::Assistant,
                content: vec![Block::ToolUse {
                    id: "c1".into(),
                    name: "search_notes".into(),
                    input: json!({ "query": "x" }),
                }],
            },
            Message {
                role: Role::User,
                content: vec![Block::ToolResult { id: "c1".into(), content: "[]".into(), is_error: false }],
            },
        ],
        tools: vec![formalnotes::ai::chat::ToolSpec {
            name: "file_notes",
            description: "d",
            schema: json!({ "type": "object" }),
        }],
        choice: Choice::Required("file_notes"),
        max_tokens: 10,
    };
    let body = chat::body(Provider::OpenAi, &request, "gpt-5.5");
    assert_eq!(body["messages"][0], json!({ "role": "system", "content": "Be brief." }));
    assert_eq!(
        body["messages"][2]["tool_calls"][0]["function"],
        json!({ "name": "search_notes", "arguments": "{\"query\":\"x\"}" })
    );
    assert_eq!(body["messages"][2]["content"], Value::Null);
    assert_eq!(body["messages"][3], json!({ "role": "tool", "tool_call_id": "c1", "content": "[]" }));
    assert_eq!(body["tool_choice"], json!({ "type": "function", "function": { "name": "file_notes" } }));
    assert_eq!(body["tools"][0]["function"]["parameters"], json!({ "type": "object" }));
    let anthropic = chat::body(Provider::Anthropic, &request, "claude-sonnet-5-5");
    assert_eq!(anthropic["tool_choice"], json!({ "type": "auto" }), "current Claude models refuse a forced choice");
    assert_eq!(anthropic["tools"][0]["input_schema"], json!({ "type": "object" }));
}

fn demo_store(cx: &mut TestApp, root: &Path) -> Entity<Store> {
    formalnotes::demo::seed_into(root).unwrap();
    let store = cx.new(|cx| Store::new(root.to_owned(), cx));
    store.update(cx, |s, cx| s.settle(cx));
    store
}

#[nd::test]
fn the_tool_loop_searches_reads_and_answers() {
    let dir = tempfile::tempdir().unwrap();
    let mut cx = TestApp::new();
    let store = demo_store(&mut cx, dir.path());
    let library = Arc::new(store.read(&cx).library().clone());
    let script = Script::new([
        call("c1", "search_notes", json!({ "query": "beta ship" })),
        call("c2", "read_note", json!({ "path": "Projects/Meeting notes.md" })),
        answer("The beta ships on the 14th [[Projects/Meeting notes.md]]."),
    ]);
    let request = formalnotes::ai::ask::request(&[], "When does the beta ship?", "Monday");
    let mut progress = Vec::new();
    let added = agent::run(&Fake(script.clone()), request, &mut LibraryTools::new(library), &no_cancel(), &mut |p| {
        progress.push(p)
    })
    .unwrap();
    assert_eq!(added.len(), 5, "call, results, call, results, answer");
    assert_eq!(added.last().unwrap().text(), "The beta ships on the 14th [[Projects/Meeting notes.md]].");
    let seen = script.seen();
    assert_eq!(seen.len(), 3);
    assert_eq!(seen[0].tools.iter().map(|t| t.name).collect::<Vec<_>>(), ["search_notes", "read_note", "list_folders"]);
    let result = |request: &Request| match &request.messages.last().unwrap().content[0] {
        Block::ToolResult { content, is_error, .. } => (content.clone(), *is_error),
        other => panic!("{other:?}"),
    };
    let (hits, error) = result(&seen[1]);
    assert!(!error && hits.contains("Projects/Meeting notes.md"), "{hits}");
    let (read, _) = result(&seen[2]);
    assert!(read.contains("Ship the beta on the 14th"), "{read}");
    assert!(
        progress.contains(&Progress::Tool {
            name: "read_note".into(),
            input: json!({ "path": "Projects/Meeting notes.md" })
        })
    );
}

#[nd::test]
fn the_tool_loop_reports_bad_tools_stops_at_its_limit_and_on_refusal() {
    let dir = tempfile::tempdir().unwrap();
    let mut cx = TestApp::new();
    let store = demo_store(&mut cx, dir.path());
    let library = Arc::new(store.read(&cx).library().clone());
    let request = || formalnotes::ai::ask::request(&[], "q", "Monday");

    let script = Script::new([call("c1", "read_note", json!({ "path": "Nope.md" })), answer("Your notes do not say.")]);
    agent::run(&Fake(script.clone()), request(), &mut LibraryTools::new(library.clone()), &no_cancel(), &mut |_| {})
        .unwrap();
    let last = script.seen()[1].messages.last().unwrap().content[0].clone();
    assert!(matches!(last, Block::ToolResult { is_error: true, .. }), "{last:?}");

    let forever = Script::new((0..MAX_STEPS + 1).map(|i| call(&format!("c{i}"), "list_folders", json!({}))));
    let out = agent::run(&Fake(forever), request(), &mut LibraryTools::new(library.clone()), &no_cancel(), &mut |_| {});
    assert!(matches!(out, Err(AiError::BadReply(_))));

    let refusal = Ok(Reply { message: Message { role: Role::Assistant, content: Vec::new() }, stop: Stop::Refusal });
    let out = agent::run(
        &Fake(Script::new([refusal])),
        request(),
        &mut LibraryTools::new(library),
        &no_cancel(),
        &mut |_| {},
    );
    assert_eq!(out, Err(AiError::Refused));
}

const DUMP: &str =
    "call the plumber about the leak tuesday\nbasil and tomatoes for the balcony garden, plant after the frost\n";

fn plan_reply() -> Result<Reply, AiError> {
    call(
        "f1",
        "file_notes",
        json!({ "notes": [
            { "folder": "Journal", "title": "Plumber", "body": "- [ ] Call the plumber about the leak, Tuesday", "properties": [{ "name": "status", "value": "Todo" }] },
            { "folder": "Projects/Garden", "title": "Balcony garden", "body": "# Balcony garden\n\nBasil and tomatoes, plant after the frost." },
        ]}),
    )
}

fn write_dump(root: &Path, minutes_ago: u64) {
    std::fs::create_dir_all(root.join("Inbox")).unwrap();
    let file = root.join("Inbox/Brain dump.md");
    std::fs::write(&file, DUMP).unwrap();
    formalnotes::library::disk::set_modified(&file, SystemTime::now() - Duration::from_secs(minutes_ago * 60)).unwrap();
}

fn read(root: &Path, path: &str) -> String {
    std::fs::read_to_string(root.join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

#[nd::test]
fn the_organizer_files_a_plan_through_the_store_and_undo_takes_it_back() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut cx = TestApp::new();
    write_dump(root, 10);
    let store = demo_store(&mut cx, root);
    let dump = NoteId("Inbox/Brain dump.md".into());

    let script = Script::new([plan_reply()]);
    let job = {
        let store = store.read(&cx);
        let note = store.note(&dump).unwrap();
        Job::new(
            &ORGANIZE_BRAIN_DUMPS,
            note,
            store.library(),
            FolderId("Inbox".into()),
            "Titles in sentence case.".into(),
        )
    };
    let plan = job.run(&Fake(script.clone()), &no_cancel()).unwrap();
    let sent = &script.seen()[0];
    assert!(sent.system.ends_with("Titles in sentence case."), "the extra instructions follow the prompt");
    assert_eq!(sent.choice, Choice::Required("file_notes"));
    let user = sent.messages[0].text();
    assert!(user.contains(DUMP.trim()) && user.contains("\"Recipes\": Focaccia; Lentil soup; Pancakes"), "{user}");

    let applied = store.update(&mut cx, |s, cx| organize::apply(s, &dump, job.hash, &plan, cx)).unwrap();
    store.update(&mut cx, |s, cx| {
        s.flush();
        s.settle(cx);
    });
    assert_eq!(
        applied.created,
        [NoteId("Journal/Plumber.md".into()), NoteId("Projects/Garden/Balcony garden.md".into())]
    );
    assert_eq!(applied.new_folders, [FolderId("Projects/Garden".into())]);
    assert_eq!(
        read(root, "Journal/Plumber.md"),
        "---\nstatus: Todo\n---\n# Plumber\n\n- [ ] Call the plumber about the leak, Tuesday\n"
    );
    assert_eq!(
        read(root, "Projects/Garden/Balcony garden.md"),
        "# Balcony garden\n\nBasil and tomatoes, plant after the frost.\n"
    );
    assert!(!root.join("Inbox/Brain dump.md").exists(), "the dump went to the trash");
    assert_eq!(read(root, &format!(".trash/{}", applied.trashed)), DUMP);

    store.update(&mut cx, |s, cx| organize::undo(s, &applied, cx));
    store.update(&mut cx, |s, cx| s.settle(cx));
    assert_eq!(read(root, "Inbox/Brain dump.md"), DUMP, "undo puts the dump back");
    assert!(!root.join("Journal/Plumber.md").exists() && !root.join("Projects/Garden").exists());
}

#[nd::test]
fn an_edited_dump_is_not_filed_from_its_old_text() {
    let dir = tempfile::tempdir().unwrap();
    let mut cx = TestApp::new();
    write_dump(dir.path(), 10);
    let store = demo_store(&mut cx, dir.path());
    let dump = NoteId("Inbox/Brain dump.md".into());
    let hash = organize::text_hash(DUMP);
    store.update(&mut cx, |s, cx| s.edit(&dump, format!("{DUMP}one more thing\n"), cx));
    let plan = organize::parse_plan(
        &json!({ "notes": [{ "folder": "", "title": "T", "body": "B" }] }),
        &FolderId("Inbox".into()),
    )
    .unwrap();
    let out = store.update(&mut cx, |s, cx| organize::apply(s, &dump, hash, &plan, cx));
    assert!(out.is_err());
    assert!(store.read(&cx).note(&dump).is_some());
}

/// Settings with the organizer on, as `config.toml` would set them.
fn organizer_on(cx: &mut TestApp) {
    let dir = std::env::temp_dir().join("formalnotes-ai-test-no-settings");
    let options = nd::settings::Options::new("formalnotes")
        .paths(nd::settings::Paths::in_dir(&dir))
        .env(Vec::new())
        .cli(vec![("brain_dumps.organize".into(), "true".into())]);
    let handle = nd::settings::SettingsHandle::<NotesSettings>::load(options).unwrap();
    nd::components::forms::settings::provide(cx, handle);
}

fn automations(cx: &mut TestApp, store: &Entity<Store>) -> Entity<Automations> {
    let store = store.clone();
    cx.new(|cx| Automations::new(store, cx))
}

#[nd::test]
fn a_rested_dump_is_picked_up_once_and_filed() {
    let dir = tempfile::tempdir().unwrap();
    let mut cx = TestApp::new();
    organizer_on(&mut cx);
    let script = Script::new([plan_reply()]);
    cx.set_global(Connector(Arc::new(Fake(script.clone()))));
    write_dump(dir.path(), 2);
    let store = demo_store(&mut cx, dir.path());
    let auto = automations(&mut cx, &store);
    let modified = store.read(&cx).note(&NoteId("Inbox/Brain dump.md".into())).unwrap().modified;

    auto.update(&mut cx, |a, cx| a.check(modified + SignedDuration::from_mins(4), cx));
    assert!(!auto.read(&cx).is_running(), "not yet: five minutes of rest by default");
    auto.update(&mut cx, |a, _| a.set_open(Some(NoteId("Inbox/Brain dump.md".into()))));
    auto.update(&mut cx, |a, cx| a.check(modified + SignedDuration::from_mins(6), cx));
    assert!(!auto.read(&cx).is_running(), "never the note open in the editor");
    auto.update(&mut cx, |a, _| a.set_open(None));
    auto.update(&mut cx, |a, cx| a.check(modified + SignedDuration::from_mins(6), cx));
    assert!(auto.read(&cx).is_running());
    auto.update(&mut cx, |a, cx| a.settle(cx));
    store.update(&mut cx, |s, cx| s.settle(cx));
    let outcomes = auto.update(&mut cx, |a, _| a.take_outcomes());
    let [Outcome::Organized(applied)] = outcomes.as_slice() else { panic!("{outcomes:?}") };
    assert_eq!(applied.source_title, "Brain dump");
    assert!(dir.path().join("Journal/Plumber.md").exists());

    // Undone, the same text is back in the inbox and stays there.
    store.update(&mut cx, |s, cx| organize::undo(s, applied, cx));
    store.update(&mut cx, |s, cx| s.settle(cx));
    auto.update(&mut cx, |a, cx| a.check(Timestamp::now() + SignedDuration::from_hours(1), cx));
    assert!(!auto.read(&cx).is_running(), "a note is never picked up twice for the same text");
    assert_eq!(script.seen().len(), 1);
}

#[nd::test]
fn a_failed_run_says_so_once_and_backs_off() {
    let dir = tempfile::tempdir().unwrap();
    let mut cx = TestApp::new();
    organizer_on(&mut cx);
    let script = Script::new([Err(AiError::RateLimited), Err(AiError::RateLimited), plan_reply()]);
    cx.set_global(Connector(Arc::new(Fake(script.clone()))));
    write_dump(dir.path(), 10);
    let store = demo_store(&mut cx, dir.path());
    let auto = automations(&mut cx, &store);
    let now = Timestamp::now();
    let run = |cx: &mut TestApp, at: Timestamp| {
        auto.update(cx, |a, cx| a.check(at, cx));
        let started = auto.read(cx).is_running();
        auto.update(cx, |a, cx| a.settle(cx));
        (started, auto.update(cx, |a, _| a.take_outcomes()))
    };

    let (started, outcomes) = run(&mut cx, now);
    assert!(started);
    assert!(matches!(outcomes.as_slice(), [Outcome::Failed { error: AiError::RateLimited, .. }]), "{outcomes:?}");
    assert_eq!(run(&mut cx, now + SignedDuration::from_mins(1)), (false, Vec::new()), "backing off");
    let (started, outcomes) = run(&mut cx, now + SignedDuration::from_mins(6));
    assert!(started && outcomes.is_empty(), "tried again, and quiet about it: {outcomes:?}");
    let (started, outcomes) = run(&mut cx, now + SignedDuration::from_mins(17));
    assert!(started && matches!(outcomes.as_slice(), [Outcome::Organized(_)]), "{outcomes:?}");
}
