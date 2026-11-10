//! The server's API ("API" in `docs/protocol.md`): a trait, the HTTP client, and an in-memory
//! server that keeps the same rules for the tests.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const DEFAULT_URL: &str = "https://notes.formalsnake.dev";
pub const PULL_LIMIT: u32 = 500;
pub const PUSH_LIMIT: usize = 100;
pub const BLOB_LIMIT: usize = 1024 * 1024;
const LIVE_OBJECT_LIMIT: u64 = 20_000;
const BYTE_LIMIT: u64 = 200_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteError {
    /// No answer: no network, the server is down, or the request timed out.
    Offline(String),
    /// `401`: the session is gone; sign in again.
    Unauthorized,
    Api {
        status: u16,
        code: String,
        message: String,
    },
}

impl RemoteError {
    pub fn code(&self) -> Option<&str> {
        match self {
            RemoteError::Api { code, .. } => Some(code),
            _ => None,
        }
    }
}

impl std::fmt::Display for RemoteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RemoteError::Offline(why) => write!(f, "the server could not be reached ({why})"),
            RemoteError::Unauthorized => f.write_str("the session has ended"),
            RemoteError::Api { message, code, .. } if message.is_empty() => f.write_str(code),
            RemoteError::Api { message, .. } => f.write_str(message),
        }
    }
}

impl std::error::Error for RemoteError {}

pub type Result<T> = std::result::Result<T, RemoteError>;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Usage {
    pub email: String,
    pub objects: u64,
    pub bytes: u64,
}

/// A pulled object in its current state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change {
    pub id: String,
    pub seq: u64,
    pub deleted: bool,
    pub blob: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Page {
    pub changes: Vec<Change>,
    pub cursor: u64,
    pub more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Push {
    pub id: String,
    pub base: u64,
    pub deleted: bool,
    pub blob: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum Outcome {
    Ok { id: String, seq: u64 },
    Conflict { id: String, seq: u64, deleted: bool, blob: Option<String> },
}

/// The API. Every account route after sign-in takes the session token.
pub trait Remote: Send + Sync {
    /// Creates the account and returns its session token.
    fn sign_up(&self, email: &str, password: &str) -> Result<String>;
    fn sign_in(&self, email: &str, password: &str) -> Result<String>;
    fn sign_out(&self, token: &str) -> Result<()>;
    fn usage(&self, token: &str) -> Result<Usage>;
    /// The wrapped vault key, or `None` for an account that has none yet.
    fn keys(&self, token: &str) -> Result<Option<String>>;
    fn put_keys(&self, token: &str, wrapped: &str) -> Result<()>;
    fn change_password(&self, token: &str, current: &str, new: &str, wrapped: &str) -> Result<()>;
    fn delete_account(&self, token: &str, password: &str) -> Result<()>;
    fn pull(&self, token: &str, since: u64, limit: u32) -> Result<Page>;
    fn push(&self, token: &str, changes: &[Push]) -> Result<Vec<Outcome>>;
}

/// The server at `FORMALNOTES_SYNC_URL`, else [`DEFAULT_URL`].
pub fn base_url() -> String {
    std::env::var("FORMALNOTES_SYNC_URL")
        .ok()
        .filter(|u| !u.trim().is_empty())
        .map_or_else(|| DEFAULT_URL.to_owned(), |u| u.trim().trim_end_matches('/').to_owned())
}

pub struct Http {
    base: String,
    agent: ureq::Agent,
}

impl Http {
    pub fn new(base: String) -> Self {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_global(Some(Duration::from_secs(120)))
            .build();
        Self { base, agent: config.into() }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    fn call(
        &self,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> Result<(u16, Option<String>, Value)> {
        let url = self.url(path);
        let auth = token.map(|t| format!("Bearer {t}"));
        let response = match (method, body) {
            ("GET", _) => {
                let mut r = self.agent.get(&url);
                if let Some(a) = &auth {
                    r = r.header("Authorization", a);
                }
                r.call()
            }
            (method, body) => {
                let mut r = match method {
                    "PUT" => self.agent.put(&url),
                    "DELETE" => self.agent.delete(&url).force_send_body(),
                    _ => self.agent.post(&url),
                };
                if let Some(a) = &auth {
                    r = r.header("Authorization", a);
                }
                r.header("Content-Type", "application/json").send(body.unwrap_or_else(|| json!({})).to_string())
            }
        };
        let mut response = response.map_err(|e| RemoteError::Offline(e.to_string()))?;
        let status = response.status().as_u16();
        let token = response.headers().get("set-auth-token").and_then(|v| v.to_str().ok()).map(str::to_owned);
        let text = response
            .body_mut()
            .with_config()
            .limit(1 << 30)
            .read_to_string()
            .map_err(|e| RemoteError::Offline(e.to_string()))?;
        let value: Value =
            if text.trim().is_empty() { Value::Null } else { serde_json::from_str(&text).unwrap_or(Value::Null) };
        match status {
            200..=299 => Ok((status, token, value)),
            401 => Err(RemoteError::Unauthorized),
            // Errors are `{ error, message }`; Better Auth's own routes answer `{ code, message }`.
            _ => Err(RemoteError::Api {
                status,
                code: value["error"].as_str().or(value["code"].as_str()).unwrap_or("http_error").to_owned(),
                message: value["message"].as_str().unwrap_or_default().to_owned(),
            }),
        }
    }

    fn session(&self, path: &str, body: Value) -> Result<String> {
        let (_, header, value) = self.call("POST", path, None, Some(body))?;
        header
            .or_else(|| value["token"].as_str().map(str::to_owned))
            .ok_or_else(|| RemoteError::Offline("the server sent no session".into()))
    }
}

fn decode<T: for<'de> Deserialize<'de>>(value: Value) -> Result<T> {
    serde_json::from_value(value).map_err(|e| RemoteError::Offline(format!("unexpected answer: {e}")))
}

impl Remote for Http {
    fn sign_up(&self, email: &str, password: &str) -> Result<String> {
        let name = email.split('@').next().unwrap_or(email);
        self.session("/api/auth/sign-up/email", json!({ "name": name, "email": email, "password": password }))
    }

    fn sign_in(&self, email: &str, password: &str) -> Result<String> {
        self.session("/api/auth/sign-in/email", json!({ "email": email, "password": password }))
    }

    fn sign_out(&self, token: &str) -> Result<()> {
        self.call("POST", "/api/auth/sign-out", Some(token), None).map(drop)
    }

    fn usage(&self, token: &str) -> Result<Usage> {
        decode(self.call("GET", "/v1/account", Some(token), None)?.2)
    }

    fn keys(&self, token: &str) -> Result<Option<String>> {
        match self.call("GET", "/v1/keys", Some(token), None) {
            Ok((_, _, v)) => Ok(v["wrappedVaultKey"].as_str().map(str::to_owned)),
            Err(e) if e.code() == Some("no_keys") => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn put_keys(&self, token: &str, wrapped: &str) -> Result<()> {
        self.call("PUT", "/v1/keys", Some(token), Some(json!({ "wrappedVaultKey": wrapped }))).map(drop)
    }

    fn change_password(&self, token: &str, current: &str, new: &str, wrapped: &str) -> Result<()> {
        let body = json!({ "currentPassword": current, "newPassword": new, "wrappedVaultKey": wrapped });
        self.call("POST", "/v1/account/password", Some(token), Some(body)).map(drop)
    }

    fn delete_account(&self, token: &str, password: &str) -> Result<()> {
        self.call("DELETE", "/v1/account", Some(token), Some(json!({ "password": password }))).map(drop)
    }

    fn pull(&self, token: &str, since: u64, limit: u32) -> Result<Page> {
        decode(self.call("GET", &format!("/v1/sync?since={since}&limit={limit}"), Some(token), None)?.2)
    }

    fn push(&self, token: &str, changes: &[Push]) -> Result<Vec<Outcome>> {
        let (_, _, v) = self.call("POST", "/v1/sync", Some(token), Some(json!({ "changes": changes })))?;
        decode(v["results"].clone())
    }
}

/// Whether the server answers `GET /health`.
pub fn healthy(base: &str) -> bool {
    Http::new(base.to_owned()).call("GET", "/health", None, None).is_ok_and(|(_, _, v)| v["ok"] == json!(true))
}

/// The server's rules in memory: accounts, sessions, one counter per account, `base` checks,
/// conflicts, pages and limits. Cloning shares the server, so two devices can talk to one.
#[derive(Clone, Default)]
pub struct Fake(Arc<Mutex<FakeState>>);

#[derive(Default)]
struct FakeState {
    users: HashMap<String, FakeUser>,
    sessions: HashMap<String, String>,
    next_session: u64,
    offline: bool,
    /// Pushes to apply and then answer as if the connection dropped.
    lose_answers: u32,
}

#[derive(Default)]
struct FakeUser {
    password: String,
    keys: Option<String>,
    counter: u64,
    objects: BTreeMap<String, Change>,
}

fn api<T>(status: u16, code: &str) -> Result<T> {
    Err(RemoteError::Api { status, code: code.to_owned(), message: String::new() })
}

/// The account password is `base64url(authKey)`: exactly 43 characters.
fn check_password(password: &str) -> Result<()> {
    match password.chars().count() {
        n if n < 43 => api(400, "password_too_short"),
        n if n > 43 => api(400, "password_too_long"),
        _ => Ok(()),
    }
}

fn is_id(id: &str) -> bool {
    let parts: Vec<&str> = id.split('-').collect();
    let lens = [8, 4, 4, 4, 12];
    parts.len() == 5
        && parts
            .iter()
            .zip(lens)
            .all(|(p, n)| p.len() == n && p.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
        && parts[2].starts_with('4')
        && matches!(parts[3].as_bytes().first(), Some(b'8' | b'9' | b'a' | b'b'))
}

fn blob_len(blob: &Option<String>) -> u64 {
    blob.as_deref().and_then(|b| super::crypto::from_base64(b).ok()).map_or(0, |b| b.len() as u64)
}

impl Fake {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_offline(&self, offline: bool) {
        self.with(|s| s.offline = offline);
    }

    /// The next `n` pushes are applied, but their answers never arrive.
    pub fn lose_answers(&self, n: u32) {
        self.with(|s| s.lose_answers = n);
    }

    /// Drops every object of the account but keeps its counter: the server lost its data.
    pub fn forget_objects(&self, email: &str) {
        self.with(|s| {
            if let Some(u) = s.users.get_mut(email) {
                u.objects.clear();
            }
        });
    }

    pub fn user_exists(&self, email: &str) -> bool {
        self.with(|s| s.users.contains_key(email))
    }

    pub fn session_count(&self) -> usize {
        self.with(|s| s.sessions.len())
    }

    /// Every live object as `(id, blob)`, for checking that the server holds only ciphertext.
    pub fn live_blobs(&self, email: &str) -> Vec<(String, String)> {
        self.with(|s| {
            s.users.get(email).map_or_else(Vec::new, |u| {
                u.objects.values().filter_map(|c| Some((c.id.clone(), c.blob.clone()?))).collect()
            })
        })
    }

    fn with<T>(&self, f: impl FnOnce(&mut FakeState) -> T) -> T {
        match self.0.lock() {
            Ok(mut s) => f(&mut s),
            Err(poisoned) => f(&mut poisoned.into_inner()),
        }
    }

    fn user<T>(&self, token: &str, f: impl FnOnce(&mut FakeUser) -> Result<T>) -> Result<T> {
        self.with(|s| {
            if s.offline {
                return Err(RemoteError::Offline("fake server offline".into()));
            }
            let email = s.sessions.get(token).cloned().ok_or(RemoteError::Unauthorized)?;
            let user = s.users.get_mut(&email).ok_or(RemoteError::Unauthorized)?;
            f(user)
        })
    }

    fn open_session(s: &mut FakeState, email: &str) -> String {
        s.next_session += 1;
        let token = format!("session-{}", s.next_session);
        s.sessions.insert(token.clone(), email.to_owned());
        token
    }
}

impl Remote for Fake {
    fn sign_up(&self, email: &str, password: &str) -> Result<String> {
        self.with(|s| {
            if s.offline {
                return Err(RemoteError::Offline("fake server offline".into()));
            }
            check_password(password)?;
            let email = email.trim().to_lowercase();
            if s.users.contains_key(&email) {
                return api(422, "user_already_exists_use_another_email");
            }
            s.users.insert(email.clone(), FakeUser { password: password.to_owned(), ..FakeUser::default() });
            Ok(Self::open_session(s, &email))
        })
    }

    fn sign_in(&self, email: &str, password: &str) -> Result<String> {
        self.with(|s| {
            if s.offline {
                return Err(RemoteError::Offline("fake server offline".into()));
            }
            let email = email.trim().to_lowercase();
            match s.users.get(&email) {
                Some(u) if u.password == password => Ok(Self::open_session(s, &email)),
                _ => Err(RemoteError::Unauthorized),
            }
        })
    }

    fn sign_out(&self, token: &str) -> Result<()> {
        self.with(|s| s.sessions.remove(token));
        Ok(())
    }

    fn usage(&self, token: &str) -> Result<Usage> {
        let email = self.with(|s| s.sessions.get(token).cloned()).unwrap_or_default();
        self.user(token, |u| {
            let live = u.objects.values().filter(|c| !c.deleted);
            let (objects, bytes) = live.fold((0, 0), |(n, b), c| (n + 1, b + blob_len(&c.blob)));
            Ok(Usage { email, objects, bytes })
        })
    }

    fn keys(&self, token: &str) -> Result<Option<String>> {
        self.user(token, |u| Ok(u.keys.clone()))
    }

    fn put_keys(&self, token: &str, wrapped: &str) -> Result<()> {
        self.user(token, |u| {
            if u.keys.is_some() {
                return api(409, "keys_exist");
            }
            u.keys = Some(wrapped.to_owned());
            Ok(())
        })
    }

    fn change_password(&self, token: &str, current: &str, new: &str, wrapped: &str) -> Result<()> {
        if current.chars().count() != 43 || new.chars().count() != 43 {
            return api(400, "bad_request");
        }
        let email = self.user(token, |u| {
            if u.password != current {
                return api(403, "wrong_password");
            }
            u.password = new.to_owned();
            u.keys = Some(wrapped.to_owned());
            Ok(())
        });
        email.map(|()| self.with(|s| s.sessions.retain(|t, _| t == token)))
    }

    fn delete_account(&self, token: &str, password: &str) -> Result<()> {
        self.user(token, |u| if u.password == password { Ok(()) } else { api(403, "wrong_password") })?;
        self.with(|s| {
            if let Some(email) = s.sessions.get(token).cloned() {
                s.users.remove(&email);
                s.sessions.retain(|_, e| *e != email);
            }
        });
        Ok(())
    }

    fn pull(&self, token: &str, since: u64, limit: u32) -> Result<Page> {
        let limit = limit.clamp(1, PULL_LIMIT) as usize;
        self.user(token, |u| {
            let mut changes: Vec<Change> = u.objects.values().filter(|c| c.seq > since).cloned().collect();
            changes.sort_by_key(|c| c.seq);
            let more = changes.len() > limit;
            changes.truncate(limit);
            let cursor = changes.last().map_or(since, |c| c.seq);
            Ok(Page { changes, cursor, more })
        })
    }

    fn push(&self, token: &str, changes: &[Push]) -> Result<Vec<Outcome>> {
        if changes.len() > PUSH_LIMIT || changes.iter().any(|c| !is_id(&c.id) || (!c.deleted && c.blob.is_none())) {
            return api(400, "bad_request");
        }
        for change in changes.iter().filter(|c| !c.deleted) {
            match change.blob.as_deref().map(super::crypto::from_base64) {
                Some(Ok(b)) if b.first() == Some(&1) => {
                    if b.len() > BLOB_LIMIT {
                        return api(413, "too_large");
                    }
                }
                _ => return api(400, "bad_request"),
            }
        }
        let results = self.user(token, |u| {
            // Applied to a copy: a quota failure keeps nothing, conflicts are answered one by one.
            let mut objects = u.objects.clone();
            let mut counter = u.counter;
            let mut out = Vec::new();
            for change in changes {
                let current = objects.get(&change.id);
                let taken = match current {
                    Some(c) => c.seq == change.base,
                    None => change.base == 0,
                };
                if !taken {
                    let c =
                        current.cloned().unwrap_or(Change { id: change.id.clone(), seq: 0, deleted: true, blob: None });
                    out.push(Outcome::Conflict { id: c.id, seq: c.seq, deleted: c.deleted, blob: c.blob });
                    continue;
                }
                counter += 1;
                let blob = if change.deleted { None } else { change.blob.clone() };
                objects.insert(
                    change.id.clone(),
                    Change { id: change.id.clone(), seq: counter, deleted: change.deleted, blob },
                );
                out.push(Outcome::Ok { id: change.id.clone(), seq: counter });
                let live = objects.values().filter(|c| !c.deleted);
                let (n, bytes) = live.fold((0u64, 0u64), |(n, b), c| (n + 1, b + blob_len(&c.blob)));
                if (!change.deleted && n > LIVE_OBJECT_LIMIT) || bytes > BYTE_LIMIT {
                    return api(403, "quota");
                }
            }
            u.objects = objects;
            u.counter = counter;
            Ok(out)
        })?;
        let lost = self.with(|s| {
            let lost = s.lose_answers > 0;
            s.lose_answers = s.lose_answers.saturating_sub(1);
            lost
        });
        if lost {
            return Err(RemoteError::Offline("the answer was lost".into()));
        }
        Ok(results)
    }
}
