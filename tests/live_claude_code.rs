#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Claude Code for real: the tool loop over the demo library through the `claude` on this
//! computer. `cargo test --test live_claude_code -- --ignored`.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use formalnotes::ai::agent::{self, Progress};
use formalnotes::ai::chat::{Choice, Message, Request};
use formalnotes::ai::claude_code::ClaudeCode;
use formalnotes::ai::tools::LibraryTools;
use formalnotes::library::disk;

#[test]
#[ignore = "runs the claude command on this computer's account"]
fn claude_code_answers_from_the_notes_through_the_tools() {
    let root = formalnotes::demo::seed().unwrap();
    let library = Arc::new(disk::scan(&disk::Root::new(root), &mut disk::ScanCache::default()).unwrap());
    let mut tools = LibraryTools::new(library);
    let request = Request {
        system: "You answer questions from the user's notes. Search them with the tools before answering, and \
                 answer in one sentence."
            .into(),
        messages: vec![Message::user("How long does the focaccia bake, and at what temperature?")],
        tools: Vec::new(),
        choice: Choice::Auto,
        max_tokens: 2000,
    };
    let mut called = Vec::new();
    let mut text = String::new();
    let added = agent::run(
        &ClaudeCode { model: String::new() },
        request,
        &mut tools,
        &Arc::new(AtomicBool::new(false)),
        &mut |progress| match progress {
            Progress::Text(t) => text.push_str(&t),
            Progress::Tool { name, .. } => called.push(name),
            Progress::Restart => text.clear(),
        },
    )
    .unwrap();
    let answer = added.last().unwrap().text();
    eprintln!("tools: {called:?}\nanswer: {answer}");
    assert!(called.iter().any(|name| name == "search_notes"), "it searched: {called:?}");
    assert!(answer.contains("230") && answer.contains("22"), "{answer}");
    assert_eq!(text, answer, "the streamed text is the answer");
}
