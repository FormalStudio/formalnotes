//! Sharing: this computer answers Ask for the user's other devices, with its own Claude Code.
//!
//! A small HTTP server that speaks Anthropic Messages (`POST /v1/messages`, streamed), so a
//! device points its Anthropic settings at this computer and uses the access code as its key.
//! The device keeps running the tools on its own library; this computer only lends the model.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Value, json};

use super::anthropic;
use super::chat::{AiError, Cancel, Reply, Stop, Transport};

/// The keychain account the access code is stored under.
pub const CODE_ACCOUNT: &str = "sharing-access-code";
/// The largest request read, in bytes.
const BODY_LIMIT: u64 = 16 << 20;

/// The server while it runs; dropping it stops it.
pub struct Server {
    server: Arc<tiny_http::Server>,
    pub port: u16,
}

impl Server {
    /// Listens on every interface at `port` and answers requests that carry `code`.
    pub fn start(port: u16, code: String, transport: Arc<dyn Transport>) -> Result<Self, String> {
        let server = tiny_http::Server::http(SocketAddr::from(([0, 0, 0, 0], port))).map_err(|e| e.to_string())?;
        let port = server.server_addr().to_ip().map_or(port, |a| a.port());
        let server = Arc::new(server);
        let listening = server.clone();
        let code: Arc<str> = code.into();
        std::thread::Builder::new()
            .name("formalnotes-sharing".into())
            .spawn(move || {
                for request in listening.incoming_requests() {
                    let (code, transport) = (code.clone(), transport.clone());
                    // An answer streams for as long as the model takes, so every request gets
                    // a thread.
                    let _ = std::thread::Builder::new()
                        .name("formalnotes-shared-ask".into())
                        .spawn(move || serve(request, &code, transport.as_ref()));
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self { server, port })
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.server.unblock();
    }
}

/// A new access code: 20 letters and digits, without the ones that read alike.
pub fn new_code() -> io::Result<String> {
    const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    let mut bytes = [0u8; 20];
    getrandom::fill(&mut bytes).map_err(|e| io::Error::other(e.to_string()))?;
    Ok(bytes.iter().map(|b| char::from(ALPHABET[usize::from(*b) % ALPHABET.len()])).collect())
}

/// This computer's address on the local network, for the line that tells a device where to
/// point. Nothing is sent: connecting a UDP socket only picks the route.
pub fn local_address() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    Some(socket.local_addr().ok()?.ip().to_string())
}

fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0, |diff, (x, y)| diff | (x ^ y)) == 0
}

fn header<'a>(request: &'a tiny_http::Request, name: &'static str) -> Option<&'a str> {
    request.headers().iter().find(|h| h.field.equiv(name)).map(|h| h.value.as_str())
}

fn refuse(request: tiny_http::Request, status: u16, kind: &str, message: &str) {
    let body = json!({ "type": "error", "error": { "type": kind, "message": message } });
    let response = tiny_http::Response::from_string(body.to_string()).with_status_code(status);
    let _ = match tiny_http::Header::from_bytes("content-type", "application/json") {
        Ok(json) => request.respond(response.with_header(json)),
        Err(()) => request.respond(response),
    };
}

fn serve(mut request: tiny_http::Request, code: &str, transport: &dyn Transport) {
    let path = request.url().split('?').next().unwrap_or_default().to_owned();
    if *request.method() == tiny_http::Method::Get && path == "/" {
        let _ = request.respond(tiny_http::Response::from_string("Formal Notes"));
        return;
    }
    if *request.method() != tiny_http::Method::Post || path != "/v1/messages" {
        return refuse(request, 404, "not_found_error", "Formal Notes answers POST /v1/messages.");
    }
    let given = header(&request, "x-api-key")
        .or_else(|| header(&request, "authorization").map(|v| v.strip_prefix("Bearer ").unwrap_or(v)))
        .unwrap_or_default()
        .trim()
        .to_owned();
    if !same(&given, code) {
        return refuse(request, 401, "authentication_error", "The access code is not the one this computer shows.");
    }
    let mut body = String::new();
    if request.as_reader().take(BODY_LIMIT).read_to_string(&mut body).is_err() {
        return refuse(request, 400, "invalid_request_error", "The request could not be read.");
    }
    let asked = serde_json::from_str::<Value>(&body).map_err(|e| e.to_string()).and_then(|v| anthropic::request(&v));
    let asked = match asked {
        Ok(asked) => asked,
        Err(why) => return refuse(request, 400, "invalid_request_error", &why),
    };

    // The reply is written straight to the connection, a chunk per event: tiny_http's own
    // responses buffer, which would hold the answer back until it was whole.
    let mut out = Stream::new(request.into_writer());
    let cancel: Cancel = Arc::new(AtomicBool::new(false));
    if out.start().is_err() {
        return;
    }
    let reply = transport.send(
        &asked,
        &mut |text| {
            if out.text(text).is_err() {
                // The device hung up; stop the model too.
                cancel.store(true, Ordering::Relaxed);
            }
        },
        &cancel,
    );
    let _ = match reply {
        Ok(reply) => out.finish(&reply),
        Err(error) => out.error(&error),
    };
}

/// A reply going out as the server-sent events of an Anthropic stream.
pub struct Stream<W: Write> {
    out: W,
    text_open: bool,
    index: u64,
}

impl<W: Write> Stream<W> {
    pub fn new(out: W) -> Self {
        Self { out, text_open: false, index: 0 }
    }

    fn event(&mut self, name: &str, data: &Value) -> io::Result<()> {
        let event = format!("event: {name}\ndata: {data}\n\n");
        write!(self.out, "{:x}\r\n{event}\r\n", event.len())?;
        self.out.flush()
    }

    /// The last chunk, which ends the response and leaves the connection usable.
    fn end(&mut self) -> io::Result<()> {
        self.out.write_all(b"0\r\n\r\n")?;
        self.out.flush()
    }

    pub fn start(&mut self) -> io::Result<()> {
        self.out.write_all(
            b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\n\
              transfer-encoding: chunked\r\n\r\n",
        )?;
        let message = json!({
            "id": "msg_formalnotes",
            "type": "message",
            "role": "assistant",
            "model": "claude-code",
            "content": [],
            "stop_reason": null,
            "usage": { "input_tokens": 0, "output_tokens": 0 },
        });
        self.event("message_start", &json!({ "type": "message_start", "message": message }))
    }

    pub fn text(&mut self, text: &str) -> io::Result<()> {
        if !self.text_open {
            self.text_open = true;
            let block = json!({ "type": "text", "text": "" });
            self.event(
                "content_block_start",
                &json!({ "type": "content_block_start", "index": self.index, "content_block": block }),
            )?;
        }
        let delta = json!({ "type": "text_delta", "text": text });
        self.event(
            "content_block_delta",
            &json!({ "type": "content_block_delta", "index": self.index, "delta": delta }),
        )
    }

    fn stop_block(&mut self) -> io::Result<()> {
        self.event("content_block_stop", &json!({ "type": "content_block_stop", "index": self.index }))?;
        self.index += 1;
        Ok(())
    }

    /// Ends the stream with the reply's tool calls and its stop reason. The text went out as it
    /// came.
    pub fn finish(&mut self, reply: &Reply) -> io::Result<()> {
        if self.text_open {
            self.text_open = false;
            self.stop_block()?;
        }
        for (id, name, input) in reply.message.tool_uses() {
            let block = json!({ "type": "tool_use", "id": id, "name": name, "input": {} });
            self.event(
                "content_block_start",
                &json!({ "type": "content_block_start", "index": self.index, "content_block": block }),
            )?;
            let delta = json!({ "type": "input_json_delta", "partial_json": input.to_string() });
            self.event(
                "content_block_delta",
                &json!({ "type": "content_block_delta", "index": self.index, "delta": delta }),
            )?;
            self.stop_block()?;
        }
        let reason = match &reply.stop {
            Stop::EndTurn => "end_turn",
            Stop::ToolUse => "tool_use",
            Stop::MaxTokens => "max_tokens",
            Stop::Refusal => "refusal",
            Stop::Other(other) => other,
        };
        let delta = json!({ "stop_reason": reason, "stop_sequence": null });
        self.event(
            "message_delta",
            &json!({ "type": "message_delta", "delta": delta, "usage": { "output_tokens": 0 } }),
        )?;
        self.event("message_stop", &json!({ "type": "message_stop" }))?;
        self.end()
    }

    /// Ends the stream with why there is no answer, in the words the app would show here.
    pub fn error(&mut self, error: &AiError) -> io::Result<()> {
        let kind = match error {
            AiError::RateLimited => "rate_limit_error",
            AiError::Unauthorized => "authentication_error",
            _ => "api_error",
        };
        self.event("error", &json!({ "type": "error", "error": { "type": kind, "message": error.line() } }))?;
        self.end()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::super::agent::{self, Tools};
    use super::super::chat::{Block, Choice, Endpoint, Http, Message, Provider, Request, Role, ToolSpec};
    use super::*;

    /// Asks for one tool call, then answers from its result.
    struct Scripted;

    impl Transport for Scripted {
        fn send(&self, request: &Request, on_text: &mut dyn FnMut(&str), _: &Cancel) -> Result<Reply, AiError> {
            assert_eq!(request.system, "Answer from the notes.");
            assert_eq!(request.tools[0].name, "search_notes");
            let last = request.messages.last().unwrap();
            if last.text() == "fail" {
                return Err(AiError::NoClaudeCode);
            }
            if let Some(Block::ToolResult { content, .. }) = last.content.first() {
                on_text("It says ");
                on_text(content);
                let content = vec![Block::Text(format!("It says {content}"))];
                return Ok(Reply { message: Message { role: Role::Assistant, content }, stop: Stop::EndTurn });
            }
            on_text("Looking.");
            let call =
                Block::ToolUse { id: "toolu_1".into(), name: "search_notes".into(), input: json!({ "query": "x" }) };
            let content = vec![Block::Text("Looking.".into()), call];
            Ok(Reply { message: Message { role: Role::Assistant, content }, stop: Stop::ToolUse })
        }
    }

    struct Notes(Vec<Value>);

    impl Tools for Notes {
        fn specs(&self) -> Vec<ToolSpec> {
            vec![ToolSpec { name: "search_notes".into(), description: "d".into(), schema: json!({ "type": "object" }) }]
        }

        fn run(&mut self, _: &str, input: &Value) -> Result<String, String> {
            self.0.push(input.clone());
            Ok("oat milk".into())
        }
    }

    /// The app's own client, pointed at the server as a device's would be.
    fn device(server: &Server, code: &str) -> Http {
        let base_url = format!("http://127.0.0.1:{}", server.port);
        Http { endpoint: Endpoint { provider: Provider::Anthropic, model: "any".into(), base_url, key: code.into() } }
    }

    fn request(question: &str) -> Request {
        Request {
            system: "Answer from the notes.".into(),
            messages: vec![Message::user(question)],
            tools: Vec::new(),
            choice: Choice::Auto,
            max_tokens: 100,
        }
    }

    #[test]
    fn a_device_runs_the_tool_loop_through_this_computer() {
        let server = Server::start(0, "the-code".into(), Arc::new(Scripted)).unwrap();
        let mut tools = Notes(Vec::new());
        let mut text = String::new();
        let added = agent::run(
            &device(&server, "the-code"),
            request("What does it say?"),
            &mut tools,
            &Cancel::default(),
            &mut |progress| {
                if let agent::Progress::Text(t) = progress {
                    text.push_str(&t);
                }
            },
        )
        .unwrap();
        assert_eq!(tools.0, vec![json!({ "query": "x" })]);
        assert_eq!(text, "Looking.It says oat milk");
        assert_eq!(added.last().unwrap().text(), "It says oat milk");
    }

    #[test]
    fn a_wrong_code_is_turned_away_and_a_failure_arrives_in_words() {
        let server = Server::start(0, "the-code".into(), Arc::new(Scripted)).unwrap();
        let mut asked = request("What does it say?");
        asked.tools = Notes(Vec::new()).specs();
        let reply = device(&server, "not-the-code").send(&asked, &mut |_| {}, &Cancel::default());
        assert_eq!(reply, Err(AiError::Unauthorized));

        asked.messages = vec![Message::user("fail")];
        let reply = device(&server, "the-code").send(&asked, &mut |_| {}, &Cancel::default());
        assert!(matches!(reply, Err(AiError::Server { status: 500, message }) if message.contains("not installed")));

        assert!(!same("abc", "abd") && !same("abc", "abcd") && same("abc", "abc"));
        assert_eq!(new_code().unwrap().len(), 20);
    }
}
