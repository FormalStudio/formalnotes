//! What an object's blob holds once opened: one note, folder or `.folder.json`.

use serde::{Deserialize, Serialize};

use super::crypto::{self, CryptoError, Key};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Note,
    Folder,
    /// A `.folder.json`.
    File,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Payload {
    pub v: u32,
    pub kind: Kind,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    pub created: i64,
    pub modified: i64,
}

impl Payload {
    pub fn to_json(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// A padded plaintext: the padding is JSON whitespace, which the parser skips.
    pub fn parse(padded: &[u8]) -> Option<Self> {
        let payload: Self = serde_json::from_slice(padded).ok()?;
        (payload.v == 1).then_some(payload)
    }

    pub fn seal(&self, vault: &Key, id: &str) -> Result<String, CryptoError> {
        Ok(crypto::to_base64(&crypto::seal(vault, &crypto::pad(self.to_json()), id.as_bytes())?))
    }

    pub fn open(blob: &str, vault: &Key, id: &str) -> Result<Self, CryptoError> {
        let padded = crypto::open(vault, &crypto::from_base64(blob)?, id.as_bytes())?;
        Self::parse(&padded).ok_or(CryptoError::Open)
    }

    /// The hash a device keeps for what it synced: the text, or nothing for a folder.
    pub fn hash(&self) -> String {
        content_hash(self.text.as_deref())
    }
}

pub fn content_hash(text: Option<&str>) -> String {
    crypto::sha256_hex(text.unwrap_or_default().as_bytes())
}
