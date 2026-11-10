//! What a device remembers between syncs, per account and library: the cursor, and per object
//! its id, path, the hash and `seq` it last synced. Kept in the app's data directory, never in
//! the library.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::crypto::sha256_hex;
use super::object::Kind;
use super::remote::Change;
use crate::library::disk::write_atomic;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tracked {
    pub kind: Kind,
    /// Where the object is in this library now; empty when it has none and is to be deleted.
    pub path: String,
    /// Where it was when it last synced.
    pub synced_path: String,
    /// SHA-256 of the text it last synced; empty when nothing has synced yet.
    pub hash: String,
    pub seq: u64,
    /// A push whose answer has not come back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sent: Option<Sent>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sent {
    pub path: String,
    pub hash: String,
    pub deleted: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub cursor: u64,
    pub objects: BTreeMap<String, Tracked>,
    /// Pulled changes held back while the note is open with unsaved typing.
    #[serde(default)]
    pub deferred: BTreeMap<String, Change>,
}

impl State {
    pub fn load(path: &Path) -> io::Result<Self> {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e),
        }
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        write_atomic(path, &serde_json::to_vec(self).map_err(io::Error::other)?)
    }

    /// The object tracked at `path` now, if any.
    pub fn at(&self, path: &str) -> Option<&str> {
        self.objects.iter().find(|(_, t)| t.path == path).map(|(id, _)| id.as_str())
    }
}

/// The state file for `email`'s sync of the library at `root`, under `dir`.
pub fn file(dir: &Path, email: &str, root: &Path) -> PathBuf {
    let key = sha256_hex(format!("{}\n{}", email.trim().to_lowercase(), root.display()).as_bytes());
    dir.join(format!("{}.json", &key[..32]))
}
