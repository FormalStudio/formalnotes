//! Claude Code as the model: the `claude` command on this computer, on the account it is signed
//! in to, in place of an API key.
//!
//! Claude Code runs its own tool loop, so one conversation is one `claude` process. Its built-in
//! tools are off; the request's tools are offered to it over a loopback MCP server. When the
//! model calls one, the turn is handed back as a normal reply and the process waits inside that
//! call. The next request carries the tool results, which answer the waiting call, and the same
//! process goes on to the next turn.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::anthropic;
use super::chat::{AiError, Block, Cancel, Message, Reply, Request, Role, Stop, StreamParser, Transport, cancelled};
use super::sse;

/// The MCP server's name, which Claude Code puts in front of every tool it offers.
const SERVER: &str = "notes";
/// A process whose tool results never came is stopped after this long.
const PARKED: Duration = Duration::from_secs(600);
const POLL: Duration = Duration::from_millis(100);
/// The largest MCP message read, in bytes.
const MESSAGE_LIMIT: u64 = 1 << 20;

pub struct ClaudeCode {
    /// Empty leaves the choice to Claude Code.
    pub model: String,
}

impl Transport for ClaudeCode {
    fn send(&self, request: &Request, on_text: &mut dyn FnMut(&str), cancel: &Cancel) -> Result<Reply, AiError> {
        let bridge = bridge()?;
        let mut session = match bridge.resume(request) {
            Some(session) => session,
            None => Session::start(bridge, request, &self.model)?,
        };
        let reply = clean(read_turn(&session.events, on_text, cancel).map_err(|e| session.explain(e))?);
        let calls: Vec<Call> =
            reply.message.tool_uses().map(|(id, name, _)| Call { id: id.to_owned(), name: name.to_owned() }).collect();
        if reply.stop == Stop::ToolUse && !calls.is_empty() {
            session.shared.lock().open = calls;
            // nd-allow(clock-in-ui): a request thread's own deadline, not view code
            session.parked = Instant::now();
            lock(&bridge.parked).push(session);
        }
        Ok(reply)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A tool call the model made and is waiting on.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Call {
    id: String,
    name: String,
}

#[derive(Default)]
struct Calls {
    /// The calls of the turn last handed back.
    open: Vec<Call>,
    /// Calls a waiting MCP request has taken, so two calls of one tool pair up in order.
    claimed: Vec<String>,
    results: HashMap<String, (String, bool)>,
    closed: bool,
}

/// What the MCP server sees of a session.
struct Shared {
    tools: Vec<Value>,
    calls: Mutex<Calls>,
    changed: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Calls> {
        lock(&self.calls)
    }

    /// Waits for the result of a call: the one Claude Code names, or the first unclaimed call of
    /// `name` when it names none.
    fn result(&self, id: Option<&str>, name: &str) -> Option<(String, bool)> {
        // nd-allow(clock-in-ui): a request thread's own deadline, not view code
        let deadline = Instant::now() + PARKED;
        let mut calls = self.lock();
        let mut id = id.map(str::to_owned);
        loop {
            if calls.closed {
                return None;
            }
            if id.is_none() {
                let free = calls.open.iter().find(|c| c.name == name && !calls.claimed.contains(&c.id));
                id = free.map(|c| c.id.clone());
                if let Some(id) = &id {
                    calls.claimed.push(id.clone());
                }
            }
            if let Some(result) = id.as_ref().and_then(|id| calls.results.remove(id)) {
                return Some(result);
            }
            // nd-allow(clock-in-ui): a request thread's own deadline, not view code
            let left = deadline.checked_duration_since(Instant::now())?;
            calls = self.changed.wait_timeout(calls, left).unwrap_or_else(PoisonError::into_inner).0;
        }
    }
}

struct Bridge {
    port: u16,
    sessions: Mutex<HashMap<String, Arc<Shared>>>,
    /// Sessions waiting on tool results.
    parked: Mutex<Vec<Session>>,
}

/// The loopback MCP server, started with the first request.
fn bridge() -> Result<&'static Bridge, AiError> {
    static BRIDGE: OnceLock<Result<Bridge, String>> = OnceLock::new();
    BRIDGE.get_or_init(Bridge::start).as_ref().map_err(|e| AiError::ClaudeCode(e.clone()))
}

impl Bridge {
    fn start() -> Result<Self, String> {
        let server = tiny_http::Server::http("127.0.0.1:0").map_err(|e| e.to_string())?;
        let port = server.server_addr().to_ip().map(|a| a.port()).ok_or("no loopback port")?;
        std::thread::Builder::new()
            .name("formalnotes-claude-code".into())
            .spawn(move || {
                for request in server.incoming_requests() {
                    // A tool call waits for its result, so every request gets a thread.
                    let _ = std::thread::Builder::new().name("formalnotes-mcp".into()).spawn(move || serve(request));
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self { port, sessions: Mutex::default(), parked: Mutex::default() })
    }

    /// The parked session `request` carries the tool results for, with those results delivered.
    fn resume(&self, request: &Request) -> Option<Session> {
        let mut parked = lock(&self.parked);
        parked.retain(|s| s.parked.elapsed() < PARKED);
        let last = request.messages.last().filter(|m| m.role == Role::User)?;
        let results: Vec<(&String, &String, bool)> = last
            .content
            .iter()
            .filter_map(|b| match b {
                Block::ToolResult { id, content, is_error } => Some((id, content, *is_error)),
                _ => None,
            })
            .collect();
        let (first, ..) = results.first()?;
        let at = parked.iter().position(|s| s.shared.lock().open.iter().any(|c| c.id == **first))?;
        let session = parked.swap_remove(at);
        {
            let mut calls = session.shared.lock();
            for (id, content, is_error) in results {
                calls.results.insert(id.clone(), (content.clone(), is_error));
            }
        }
        session.shared.changed.notify_all();
        Some(session)
    }
}

struct Session {
    id: String,
    child: Child,
    /// Claude Code's output, one JSON line at a time.
    events: flume::Receiver<Value>,
    stderr: Arc<Mutex<String>>,
    shared: Arc<Shared>,
    parked: Instant,
}

impl Session {
    fn start(bridge: &'static Bridge, request: &Request, model: &str) -> Result<Self, AiError> {
        let program = find_claude().ok_or(AiError::NoClaudeCode)?;
        let id = session_id()?;
        let tools = request
            .tools
            .iter()
            .map(|t| json!({ "name": t.name, "description": t.description, "inputSchema": t.schema }))
            .collect();
        let shared = Arc::new(Shared { tools, calls: Mutex::default(), changed: Condvar::new() });
        let servers = json!({
            "mcpServers": { SERVER: { "type": "http", "url": format!("http://127.0.0.1:{}/mcp/{id}", bridge.port) } }
        });
        let system = if request.system.trim().is_empty() { "You are a helpful assistant." } else { &request.system };
        let mut command = Command::new(program);
        command
            .args(["-p", "--input-format", "stream-json", "--output-format", "stream-json"])
            .args(["--include-partial-messages", "--verbose", "--no-session-persistence"])
            // No built-in tools, no settings, hooks or MCP servers of the user's: the model sees
            // the request and nothing of this computer.
            .args(["--tools", "", "--setting-sources", "", "--strict-mcp-config", "--permission-prompts", "none"])
            .args(["--mcp-config", &servers.to_string()])
            .args(["--allowedTools", &format!("mcp__{SERVER}__*")])
            .args(["--system-prompt", system])
            .current_dir(workdir())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if !model.trim().is_empty() {
            command.args(["--model", model.trim()]);
        }
        #[cfg(windows)]
        {
            // CREATE_NO_WINDOW: a console program started from a windowed app opens a console.
            std::os::windows::process::CommandExt::creation_flags(&mut command, 0x0800_0000);
        }
        lock(&bridge.sessions).insert(id.clone(), shared.clone());
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(e) => {
                lock(&bridge.sessions).remove(&id);
                return Err(AiError::ClaudeCode(e.to_string()));
            }
        };
        let (tx, events) = flume::unbounded();
        if let Some(out) = child.stdout.take() {
            let _ = std::thread::Builder::new().name("formalnotes-claude-out".into()).spawn(move || {
                for line in BufReader::new(out).lines().map_while(Result::ok) {
                    if let Ok(event) = serde_json::from_str::<Value>(&line)
                        && tx.send(event).is_err()
                    {
                        break;
                    }
                }
            });
        }
        let stderr = Arc::new(Mutex::new(String::new()));
        if let Some(err) = child.stderr.take() {
            let kept = stderr.clone();
            let _ = std::thread::Builder::new().name("formalnotes-claude-err".into()).spawn(move || {
                for line in BufReader::new(err).lines().map_while(Result::ok) {
                    *lock(&kept) = line;
                }
            });
        }
        let stdin = child.stdin.take();
        // nd-allow(clock-in-ui): a request thread's own deadline, not view code
        let session = Self { id, child, events, stderr, shared, parked: Instant::now() };
        let prompt = json!({
            "type": "user",
            "message": { "role": "user", "content": [{ "type": "text", "text": prompt(&request.messages) }] },
        });
        // Dropping the pipe ends the input: this conversation has one prompt.
        let sent = stdin.and_then(|mut stdin| writeln!(stdin, "{prompt}").ok());
        if sent.is_none() {
            return Err(session.explain(AiError::ClaudeCode("it did not start".into())));
        }
        Ok(session)
    }

    /// A failure in Claude Code's words when it left any on its way out.
    fn explain(&self, error: AiError) -> AiError {
        let said = lock(&self.stderr).trim().to_owned();
        match error {
            AiError::ClaudeCode(why) if !said.is_empty() => AiError::ClaudeCode(format!("{why}: {said}")),
            other => other,
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        {
            let mut calls = self.shared.lock();
            calls.closed = true;
        }
        self.shared.changed.notify_all();
        if let Ok(bridge) = bridge() {
            lock(&bridge.sessions).remove(&self.id);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn session_id() -> Result<String, AiError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| AiError::ClaudeCode(e.to_string()))?;
    Ok(hex::encode(bytes))
}

/// An empty folder to run in, so Claude Code finds no project there.
fn workdir() -> PathBuf {
    let dir = std::env::temp_dir().join("formalnotes-claude-code");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// The `claude` command: `FORMALNOTES_CLAUDE`, the PATH, then where its installers put it. An
/// app started from the desktop does not have the shell's PATH.
fn find_claude() -> Option<PathBuf> {
    if let Some(set) = std::env::var_os("FORMALNOTES_CLAUDE").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(set));
    }
    let names: &[&str] = if cfg!(windows) { &["claude.exe", "claude.cmd"] } else { &["claude"] };
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from);
    let path = std::env::var_os("PATH").unwrap_or_default();
    let installed = [".local/bin", ".claude/local", ".bun/bin", ".npm-global/bin"];
    std::env::split_paths(&path)
        .chain(home.iter().flat_map(|home| installed.iter().map(move |dir| home.join(dir))))
        .chain(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from))
        .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
        .find(|candidate| candidate.is_file())
}

/// The conversation as the one prompt a new process gets: the question, with the turns before
/// it written out when there are any.
fn prompt(messages: &[Message]) -> String {
    let Some((last, earlier)) = messages.split_last() else { return String::new() };
    if earlier.is_empty() {
        return last.text();
    }
    let mut out = String::from("The conversation so far:\n");
    for message in earlier {
        out.push_str(&written(message));
    }
    let asks = last.text();
    if asks.trim().is_empty() {
        out.push_str(&written(last));
        out.push_str("\nGo on from the tool results above.");
    } else {
        out.push_str("\nNow answer this:\n");
        out.push_str(&asks);
    }
    out
}

fn written(message: &Message) -> String {
    let who = match message.role {
        Role::User => "User",
        Role::Assistant => "Assistant",
    };
    let mut out = String::new();
    for block in &message.content {
        match block {
            Block::Text(text) if !text.trim().is_empty() => out.push_str(&format!("\n{who}: {}\n", text.trim())),
            Block::ToolUse { name, input, .. } => out.push_str(&format!("\nAssistant called {name} with {input}\n")),
            Block::ToolResult { content, .. } => out.push_str(&format!("\nThe tool returned:\n{content}\n")),
            Block::Text(_) | Block::Opaque(_) => {}
        }
    }
    out
}

/// Reads Claude Code's output up to the end of one model turn. The turn arrives as the
/// Anthropic stream events it was made of.
fn read_turn(
    events: &flume::Receiver<Value>,
    on_text: &mut dyn FnMut(&str),
    cancel: &Cancel,
) -> Result<Reply, AiError> {
    let mut parser = anthropic::Parser::default();
    loop {
        if cancelled(cancel) {
            return Err(AiError::Cancelled);
        }
        let line = match events.recv_timeout(POLL) {
            Ok(line) => line,
            Err(flume::RecvTimeoutError::Timeout) => continue,
            Err(flume::RecvTimeoutError::Disconnected) => return Err(AiError::ClaudeCode("it stopped".into())),
        };
        match line.get("type").and_then(Value::as_str) {
            Some("stream_event") => {
                let event = line.get("event").cloned().unwrap_or(Value::Null);
                let kind = event.get("type").and_then(Value::as_str).unwrap_or_default().to_owned();
                if kind == "message_start" {
                    parser = anthropic::Parser::default();
                }
                parser.event(&sse::Event { name: kind.clone(), data: event.to_string() }, on_text)?;
                if kind == "message_stop" {
                    return Box::new(parser).finish();
                }
            }
            Some("result") => {
                let said = line.get("result").and_then(Value::as_str).unwrap_or("it gave no answer").to_owned();
                return Err(AiError::ClaudeCode(said));
            }
            _ => {}
        }
    }
}

/// The reply as the app's own tools named them, without the blocks only Claude Code reads.
fn clean(mut reply: Reply) -> Reply {
    let prefix = format!("mcp__{SERVER}__");
    reply.message.content.retain(|b| !matches!(b, Block::Opaque(_)));
    for block in &mut reply.message.content {
        if let Block::ToolUse { name, .. } = block
            && let Some(own) = name.strip_prefix(&prefix)
        {
            *name = own.to_owned();
        }
    }
    reply
}

/// Answers one MCP request from a Claude Code process.
fn serve(mut request: tiny_http::Request) {
    let session = request.url().strip_prefix("/mcp/").map(str::to_owned);
    let shared = session.and_then(|id| bridge().ok().and_then(|b| lock(&b.sessions).get(&id).cloned()));
    let mut body = String::new();
    let read = request.as_reader().take(MESSAGE_LIMIT).read_to_string(&mut body).is_ok();
    let message =
        serde_json::from_str::<Value>(&body).ok().filter(|_| read && *request.method() == tiny_http::Method::Post);
    let (Some(shared), Some(message)) = (shared, message) else {
        let _ = request.respond(tiny_http::Response::empty(404));
        return;
    };
    let _ = match answer(&shared, &message) {
        Some(reply) => {
            let json = tiny_http::Header::from_bytes("content-type", "application/json");
            let response = tiny_http::Response::from_string(reply.to_string());
            request.respond(match json {
                Ok(header) => response.with_header(header),
                Err(()) => response,
            })
        }
        None => request.respond(tiny_http::Response::empty(202)),
    };
}

/// The JSON-RPC reply to `message`; none for a notification.
fn answer(shared: &Shared, message: &Value) -> Option<Value> {
    let id = message.get("id").filter(|id| !id.is_null())?.clone();
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let result = match message.get("method").and_then(Value::as_str).unwrap_or_default() {
        "initialize" => json!({
            "protocolVersion": params.get("protocolVersion").cloned().unwrap_or_else(|| json!("2025-06-18")),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "formalnotes", "version": env!("CARGO_PKG_VERSION") },
        }),
        "ping" => json!({}),
        "tools/list" => json!({ "tools": shared.tools }),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or_default();
            let call = params.pointer("/_meta/claudecode~1toolUseId").and_then(Value::as_str);
            let (text, is_error) =
                shared.result(call, name).unwrap_or_else(|| ("The conversation was closed.".to_owned(), true));
            json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
        }
        _ => {
            return Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": "Method not found" },
            }));
        }
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use super::*;

    fn shared() -> Arc<Shared> {
        let tools = vec![json!({ "name": "search_notes", "description": "d", "inputSchema": { "type": "object" } })];
        Arc::new(Shared { tools, calls: Mutex::default(), changed: Condvar::new() })
    }

    fn turn(lines: &[Value]) -> (Result<Reply, AiError>, String) {
        let (tx, rx) = flume::unbounded();
        for line in lines {
            tx.send(line.clone()).unwrap();
        }
        drop(tx);
        let mut text = String::new();
        let reply = read_turn(&rx, &mut |t| text.push_str(t), &Arc::new(AtomicBool::new(false)));
        (reply, text)
    }

    fn stream(event: Value) -> Value {
        json!({ "type": "stream_event", "event": event })
    }

    #[test]
    fn a_turn_ends_at_its_message_stop_with_the_tools_under_their_own_names() {
        let (reply, text) = turn(&[
            json!({ "type": "system", "subtype": "init" }),
            stream(json!({ "type": "message_start" })),
            stream(
                json!({ "type": "content_block_start", "index": 0, "content_block": { "type": "thinking", "thinking": "" } }),
            ),
            stream(json!({ "type": "content_block_stop", "index": 0 })),
            stream(
                json!({ "type": "content_block_start", "index": 1, "content_block": { "type": "text", "text": "" } }),
            ),
            stream(
                json!({ "type": "content_block_delta", "index": 1, "delta": { "type": "text_delta", "text": "Looking." } }),
            ),
            stream(json!({ "type": "content_block_stop", "index": 1 })),
            stream(json!({
                "type": "content_block_start",
                "index": 2,
                "content_block": { "type": "tool_use", "id": "toolu_1", "name": "mcp__notes__search_notes", "input": {} },
            })),
            stream(json!({
                "type": "content_block_delta",
                "index": 2,
                "delta": { "type": "input_json_delta", "partial_json": "{\"query\":\"lemons\"}" },
            })),
            stream(json!({ "type": "content_block_stop", "index": 2 })),
            stream(json!({ "type": "message_delta", "delta": { "stop_reason": "tool_use" } })),
            stream(json!({ "type": "message_stop" })),
            json!({ "type": "result", "is_error": false, "result": "never read" }),
        ]);
        let reply = clean(reply.unwrap());
        assert_eq!(text, "Looking.");
        assert_eq!(reply.stop, Stop::ToolUse);
        assert_eq!(
            reply.message.content,
            vec![
                Block::Text("Looking.".into()),
                Block::ToolUse {
                    id: "toolu_1".into(),
                    name: "search_notes".into(),
                    input: json!({ "query": "lemons" })
                },
            ]
        );
    }

    #[test]
    fn a_run_that_ends_without_a_turn_says_why() {
        let (reply, _) = turn(&[json!({ "type": "result", "is_error": true, "result": "Not logged in" })]);
        assert_eq!(reply, Err(AiError::ClaudeCode("Not logged in".into())));
        let (reply, _) = turn(&[]);
        assert_eq!(reply, Err(AiError::ClaudeCode("it stopped".into())));
    }

    #[test]
    fn the_mcp_server_lists_the_tools_and_holds_a_call_until_its_result_comes() {
        let shared = shared();
        let listed = answer(&shared, &json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" })).unwrap();
        assert_eq!(listed["result"]["tools"][0]["name"], "search_notes");
        assert_eq!(answer(&shared, &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })), None);
        let unknown = answer(&shared, &json!({ "jsonrpc": "2.0", "id": 2, "method": "resources/list" })).unwrap();
        assert_eq!(unknown["error"]["code"], -32601);

        shared.lock().open = vec![Call { id: "toolu_1".into(), name: "search_notes".into() }];
        let waiting = shared.clone();
        let call = std::thread::spawn(move || {
            let call = json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": { "name": "search_notes", "arguments": {}, "_meta": { "claudecode/toolUseId": "toolu_1" } },
            });
            answer(&waiting, &call).unwrap()
        });
        shared.lock().results.insert("toolu_1".into(), ("[]".into(), false));
        shared.changed.notify_all();
        let done = call.join().unwrap();
        assert_eq!(done["result"]["content"][0]["text"], "[]");
        assert_eq!(done["result"]["isError"], false);

        // Without the id, the call takes the first open one of its name.
        shared.lock().open = vec![Call { id: "toolu_2".into(), name: "search_notes".into() }];
        shared.lock().results.insert("toolu_2".into(), ("no".into(), true));
        let call = json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "search_notes" } });
        assert_eq!(answer(&shared, &call).unwrap()["result"]["isError"], true);

        shared.lock().closed = true;
        let call = json!({ "jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": { "name": "search_notes" } });
        assert_eq!(answer(&shared, &call).unwrap()["result"]["isError"], true);
    }

    #[test]
    fn a_new_process_gets_the_earlier_turns_written_out() {
        assert_eq!(prompt(&[Message::user("What is on my list?")]), "What is on my list?");
        let messages = [
            Message::user("What is on my list?"),
            Message { role: Role::Assistant, content: vec![Block::Text("Oat milk.".into())] },
            Message::user("And the recipe?"),
        ];
        assert_eq!(
            prompt(&messages),
            "The conversation so far:\n\nUser: What is on my list?\n\nAssistant: Oat milk.\n\nNow answer this:\nAnd the recipe?"
        );
    }
}
