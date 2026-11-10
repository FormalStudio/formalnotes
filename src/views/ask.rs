//! The Ask pane: questions about the notes and their answers, streaming in, each answer
//! followed by the notes it cites. It sits in the window's inspector.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use nd::prelude::*;

use crate::actions::{OpenEntry, OpenSettings};
use crate::ai::ask::{self, AskEvent, Piece};
use crate::ai::chat::{AiError, Cancel, Message};
use crate::ai::settings::connector;
use crate::library::{EntryId, Library, NoteId, Store};
use crate::nav::entry_key;
use crate::settings::NotesSettings;

pub const FIELD: &str = "ask-field";

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Working,
    Done,
    Stopped,
    Failed(AiError),
}

#[derive(Clone, Debug)]
pub struct Turn {
    pub id: u64,
    pub question: String,
    /// The answer as the model writes it, citations included.
    pub answer: String,
    /// What the model is doing before the answer starts.
    pub activity: Option<String>,
    pub sources: Vec<NoteId>,
    pub status: Status,
    version: u64,
}

struct Running {
    turn: u64,
    cancel: Cancel,
}

pub struct Ask {
    store: Entity<Store>,
    turns: Vec<Turn>,
    list: ListState<u64>,
    /// The conversation so far as the model saw it: follow-ups carry it.
    history: Vec<Message>,
    draft: SharedString,
    running: Option<Running>,
    next: u64,
    events_tx: flume::Sender<(u64, AskEvent)>,
    events_rx: flume::Receiver<(u64, AskEvent)>,
}

impl Ask {
    pub fn new(store: Entity<Store>, cx: &mut Context<Self>) -> Self {
        let (events_tx, events_rx) = flume::unbounded();
        if crate::pump::live() {
            crate::pump::pump(events_rx.clone(), cx.weak_entity(), |this: &mut Self, batch, cx| {
                for (turn, event) in batch {
                    this.event(turn, event, cx);
                }
                true
            });
        }
        Self {
            store,
            turns: Vec::new(),
            list: ListState::new(cx).follow(Follow::Tail).estimated_row_height(px(160.)),
            history: Vec::new(),
            draft: SharedString::default(),
            running: None,
            next: 0,
            events_tx,
            events_rx,
        }
    }

    pub fn turns(&self) -> &[Turn] {
        &self.turns
    }

    pub fn is_running(&self) -> bool {
        self.running.is_some()
    }

    /// Asks `question`, following on from the conversation so far.
    pub fn ask(&mut self, question: &str, cx: &mut Context<Self>) {
        let question = question.trim();
        if question.is_empty() || self.running.is_some() {
            return;
        }
        self.next += 1;
        let id = self.next;
        self.turns.push(Turn {
            id,
            question: question.to_owned(),
            answer: String::new(),
            activity: None,
            sources: Vec::new(),
            status: Status::Working,
            version: 0,
        });
        let cancel: Cancel = Arc::new(AtomicBool::new(false));
        self.running = Some(Running { turn: id, cancel: cancel.clone() });
        let today = jiff::Zoned::now().strftime("%A %-d %B %Y").to_string();
        let request = ask::request(&self.history, question, &today);
        let library = Arc::new(self.store.read(cx).library().clone());
        let settings = cx.settings::<NotesSettings>().assistant.clone();
        let tx = self.events_tx.clone();
        ask::spawn(connector(cx), settings, library, request, cancel, move |event| {
            let _ = tx.send((id, event));
        });
        self.sync(cx);
    }

    /// Stops the answer in flight; what came so far stays.
    pub fn stop(&mut self, cx: &mut Context<Self>) {
        let Some(running) = self.running.take() else { return };
        running.cancel.store(true, Ordering::Relaxed);
        self.update_turn(running.turn, |turn| {
            turn.status = Status::Stopped;
            turn.activity = None;
        });
        self.sync(cx);
    }

    /// Starts over with no conversation.
    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.stop(cx);
        self.turns.clear();
        self.history.clear();
        self.sync(cx);
    }

    fn update_turn(&mut self, id: u64, f: impl FnOnce(&mut Turn)) {
        if let Some(turn) = self.turns.iter_mut().find(|t| t.id == id) {
            f(turn);
            turn.version += 1;
        }
    }

    fn event(&mut self, turn: u64, event: AskEvent, cx: &mut Context<Self>) {
        if self.running.as_ref().is_none_or(|r| r.turn != turn) {
            return;
        }
        let library = self.store.read(cx).library().clone();
        match event {
            AskEvent::Text(text) => self.update_turn(turn, |t| {
                t.activity = None;
                t.answer.push_str(&text);
            }),
            AskEvent::Restart => self.update_turn(turn, |t| t.answer.clear()),
            AskEvent::Activity(activity) => self.update_turn(turn, |t| t.activity = Some(activity)),
            AskEvent::Done(messages) => {
                self.running = None;
                let answer = messages.last().map(Message::text).unwrap_or_default();
                let question = self.turns.iter().find(|t| t.id == turn).map(|t| t.question.clone());
                self.history.push(Message::user(question.unwrap_or_default()));
                self.history.extend(messages);
                self.update_turn(turn, |t| {
                    t.sources = ask::sources(&library, &answer);
                    t.answer = answer;
                    t.activity = None;
                    t.status = Status::Done;
                });
            }
            AskEvent::Failed(error) => {
                self.running = None;
                self.update_turn(turn, |t| {
                    t.activity = None;
                    t.status = Status::Failed(error);
                });
            }
        }
        self.sync(cx);
    }

    fn sync(&mut self, cx: &mut Context<Self>) {
        let rows: Vec<(u64, u64)> = self.turns.iter().map(|t| (t.id, t.version)).collect();
        self.list.set_items_versioned(rows, cx);
        cx.notify();
    }

    /// Blocks until the answer in flight has finished and applies everything it reported.
    /// Tier 1 tests have no host to pump the events, so they settle instead.
    pub fn settle(&mut self, cx: &mut Context<Self>) {
        while self.running.is_some() {
            let Ok((turn, event)) = self.events_rx.recv_timeout(Duration::from_secs(10)) else { return };
            self.event(turn, event, cx);
        }
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if self.running.is_some() {
            return;
        }
        let question = self.draft.to_string();
        self.draft = SharedString::default();
        self.ask(&question, cx);
    }

    fn row(&mut self, id: u64, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let Some(turn) = self.turns.iter().find(|t| t.id == id).cloned() else { return div().into_any_element() };
        let library = self.store.read(cx).library().clone();
        let working = turn.status == Status::Working;
        let answer = (!turn.answer.trim().is_empty()).then(|| {
            rich_text(answer_spans(&turn.answer, &turn.sources, &library))
                .id(format!("answer-{id}"))
                .text_color(theme.text.primary)
                .on_link(|e: &LinkClicked, window, cx| {
                    cx.dispatch_action(window.id(), &OpenEntry(e.link.to_string()));
                })
        });
        let activity = match (&turn.activity, working, turn.answer.is_empty()) {
            (Some(activity), true, _) => Some(activity.clone()),
            (None, true, true) => Some("Thinking".to_owned()),
            _ => None,
        };
        let status_line = match &turn.status {
            Status::Stopped => Some(text("Stopped.").text_color(theme.text.secondary).into_any_element()),
            Status::Failed(error) => Some(
                v_flex()
                    .gap(theme.spacing.sm)
                    .items_start()
                    .child(text(error.line()).text_color(theme.text.primary))
                    .when(error.wants_settings(), |d| {
                        d.child(button(format!("ask-settings-{id}"), "Open Settings").dispatch(OpenSettings))
                    })
                    .into_any_element(),
            ),
            _ => None,
        };
        v_flex()
            .id(format!("turn-{id}"))
            .px(theme.spacing.lg)
            .py(theme.spacing.md)
            .gap(theme.spacing.sm)
            .child(text(turn.question.clone()).font_weight(FontWeight::SEMIBOLD).text_color(theme.text.primary))
            .children(activity.map(|line| {
                h_flex()
                    .gap(theme.spacing.sm)
                    .child(spinner().label("Working"))
                    .child(text(line).text_size(theme.type_scale.footnote).text_color(theme.text.secondary))
            }))
            .children(answer)
            .children(status_line)
            .when(!turn.sources.is_empty(), |d| {
                d.child(
                    v_flex().pt(theme.spacing.xs).children(
                        turn.sources.iter().enumerate().map(|(i, note)| source_row(id, i, note, &library, cx)),
                    ),
                )
            })
            .into_any_element()
    }
}

/// A cited note: its number, title and folder. A click opens it.
fn source_row(turn: u64, index: usize, id: &NoteId, library: &Library, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let title = library.note(id).map_or_else(|| id.stem().to_owned(), |n| n.title.clone());
    let folder = id.folder();
    let place = if folder.is_root() { "Home".to_owned() } else { folder.0.replace('/', " / ") };
    let key = entry_key(&EntryId::Note(id.clone()));
    h_flex()
        .id(format!("source-{turn}-{index}"))
        .gap(theme.spacing.sm)
        .px(theme.spacing.sm)
        .py(theme.spacing.xs)
        .rounded(theme.radius.md)
        .hover(|s| s.bg(theme.surface.raised_hover))
        .child(
            text(format!("{}", index + 1))
                .tabular_nums()
                .text_size(theme.type_scale.footnote)
                .text_color(theme.text.secondary),
        )
        .child(icon(Icon::FileText, 13.).text_color(theme.text.secondary))
        .child(text(title).text_color(theme.text.primary).truncate())
        .child(text(place).text_size(theme.type_scale.footnote).text_color(theme.text.secondary).truncate())
        .on_tap(move |_: &TapEvent, window, cx| {
            cx.dispatch_action(window.id(), &OpenEntry(key.to_string()));
        })
        .into_any_element()
}

/// The answer as rich text: Markdown bold, italics, code and list markers drawn, each citation
/// a link with its source's number.
fn answer_spans(answer: &str, sources: &[NoteId], library: &Library) -> Vec<Span> {
    let mut spans = Vec::new();
    let lines: Vec<&str> = answer.trim().lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            spans.push(span("\n"));
        }
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        let (marker, rest) = if let Some(rest) = trimmed.strip_prefix("- [ ] ") {
            ("☐ ", rest)
        } else if let Some(rest) = trimmed.strip_prefix("- [x] ").or_else(|| trimmed.strip_prefix("- [X] ")) {
            ("☑ ", rest)
        } else if let Some(rest) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")) {
            ("•  ", rest)
        } else {
            ("", trimmed.trim_start_matches('#').trim_start())
        };
        let heading = trimmed.starts_with('#');
        if !marker.is_empty() {
            spans.push(span(format!("{}{marker}", "    ".repeat(indent / 2))));
        }
        for piece in ask::pieces(rest) {
            match piece {
                Piece::Text(text) => spans.extend(inline(text, heading)),
                Piece::Cite(cite) => {
                    let Some(id) = ask::resolve(library, cite) else { continue };
                    let number =
                        sources.iter().position(|s| *s == id).map_or_else(|| "·".to_owned(), |n| (n + 1).to_string());
                    let key = entry_key(&EntryId::Note(id));
                    spans.push(span(format!("[{number}]")).link(key).scale(0.85));
                }
            }
        }
    }
    spans
}

/// `**bold**`, `*italic*` and `` `code` `` in one line.
fn inline(text: &str, bold: bool) -> Vec<Span> {
    let mut out = Vec::new();
    let (mut strong, mut em, mut code) = (bold, false, false);
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut Vec<Span>, strong: bool, em: bool, code: bool| {
        if run.is_empty() {
            return;
        }
        let mut s = span(std::mem::take(run));
        if strong {
            s = s.semibold();
        }
        if em {
            s = s.italic();
        }
        if code {
            s = s.mono();
        }
        out.push(s);
    };
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '`' => {
                flush(&mut run, &mut out, strong, em, code);
                code = !code;
            }
            '*' if !code && chars.peek() == Some(&'*') => {
                chars.next();
                flush(&mut run, &mut out, strong, em, code);
                strong = !strong;
            }
            '*' if !code => {
                flush(&mut run, &mut out, strong, em, code);
                em = !em;
            }
            _ => run.push(c),
        }
    }
    flush(&mut run, &mut out, strong, em, code);
    out
}

impl Render for Ask {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let running = self.running.is_some();
        let provider = cx.settings::<NotesSettings>().assistant.provider.name();
        let body = if self.turns.is_empty() {
            v_flex()
                .flex_1()
                .justify_center()
                .items_center()
                .gap(theme.spacing.sm)
                .px(theme.spacing.xl)
                .child(icon(Icon::Lightbulb, 28.).text_color(theme.text.secondary))
                .child(
                    text("Ask anything your notes know")
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text.primary),
                )
                .child(
                    text(format!(
                        "Answers come from your notes only, through your own {provider} account, and link the notes they use."
                    ))
                    .text_size(theme.type_scale.footnote)
                    .text_color(theme.text.secondary)
                    .text_center(),
                )
                .into_any_element()
        } else {
            div().flex_1().min_h_0().child(list("ask-turns", &self.list, cx.processor(Self::row))).into_any_element()
        };
        let send = if running {
            icon_button("ask-stop", Icon::Square)
                .tooltip("Stop")
                .bezel(Bezel::Circle)
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.stop(cx)))
        } else {
            icon_button("ask-send", Icon::ArrowUp)
                .tooltip("Ask")
                .bezel(Bezel::Circle)
                .disabled(self.draft.trim().is_empty())
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.submit(cx)))
        };
        v_flex()
            .id("ask")
            .size_full()
            .child(
                h_flex()
                    .px(theme.spacing.lg)
                    .py(theme.spacing.sm)
                    .gap(theme.spacing.sm)
                    .child(text("Ask your notes").font_weight(FontWeight::SEMIBOLD).text_color(theme.text.primary))
                    .child(div().flex_1())
                    .child(
                        icon_button("ask-new", Icon::SquarePen)
                            .tooltip("New Conversation")
                            .disabled(self.turns.is_empty())
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.clear(cx))),
                    ),
            )
            .child(body)
            .child(
                h_flex()
                    .items_end()
                    .gap(theme.spacing.sm)
                    .p(theme.spacing.md)
                    .child(
                        text_area(FIELD)
                            .text(self.draft.clone())
                            .placeholder(if self.turns.is_empty() { "Ask about your notes" } else { "Ask a follow-up" })
                            .label("Question")
                            .min_lines(1)
                            .max_lines(6)
                            .submit_on_return()
                            .flex_1()
                            .on_change(cx.listener(|this, e: &TextChanged, _, cx| {
                                this.draft = e.text.clone();
                                cx.notify();
                            }))
                            .on_submit(cx.listener(|this, _: &TextSubmitted, _, cx| this.submit(cx))),
                    )
                    .child(send),
            )
    }
}
