//! The key schedule and the sealed box from `docs/protocol.md` ("Keys", "Objects"). Every value
//! here is reproduced byte for byte from `docs/sync-vectors.json` by the tests below.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use hkdf::Hkdf;
use sha2::{Digest, Sha256};

pub const ITERATIONS: u32 = 600_000;
const VERSION: u8 = 0x01;
const NONCE: usize = 12;
const TAG: usize = 16;
const PAD_TO: usize = 1024;
pub const VAULT_KEY_AAD: &[u8] = b"vault-key";

pub type Key = [u8; 32];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CryptoError {
    /// Not a sealed box, the wrong key, or a tampered blob.
    Open,
    Random,
}

impl std::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CryptoError::Open => f.write_str("the data could not be decrypted"),
            CryptoError::Random => f.write_str("the system gave no random bytes"),
        }
    }
}

impl std::error::Error for CryptoError {}

/// What a password yields: the account password the server sees and the key that wraps the
/// vault key. The master key itself is not kept.
#[derive(Clone)]
pub struct Keys {
    pub auth: Key,
    pub wrap: Key,
}

impl Keys {
    pub fn derive(email: &str, password: &str) -> Self {
        Self::from_master(&master_key(email, password, ITERATIONS))
    }

    pub fn from_master(master: &Key) -> Self {
        Self { auth: expand(master, b"formalnotes auth v1"), wrap: expand(master, b"formalnotes wrap v1") }
    }

    /// `base64url(authKey)` without padding: the `password` of every account route.
    pub fn auth_password(&self) -> String {
        URL_SAFE_NO_PAD.encode(self.auth)
    }
}

pub fn salt(email: &str) -> String {
    format!("formalnotes:v1:{}", email.trim().to_lowercase())
}

pub fn master_key(email: &str, password: &str, iterations: u32) -> Key {
    let mut out = [0; 32];
    pbkdf2::pbkdf2_hmac::<Sha256>(password.as_bytes(), salt(email).as_bytes(), iterations, &mut out);
    out
}

fn expand(master: &Key, info: &[u8]) -> Key {
    let mut out = [0; 32];
    // 32 bytes is far under HKDF-SHA256's 8160-byte limit, so this cannot fail.
    let _ = Hkdf::<Sha256>::new(None, master).expand(info, &mut out);
    out
}

pub fn random<const N: usize>() -> Result<[u8; N], CryptoError> {
    let mut out = [0; N];
    getrandom::fill(&mut out).map_err(|_| CryptoError::Random)?;
    Ok(out)
}

/// `0x01 || nonce || ciphertext || tag` under a fresh random nonce.
pub fn seal(key: &Key, plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>, CryptoError> {
    Ok(seal_with_nonce(key, &random()?, plaintext, aad))
}

pub fn seal_with_nonce(key: &Key, nonce: &[u8; NONCE], plaintext: &[u8], aad: &[u8]) -> Vec<u8> {
    let cipher = Aes256Gcm::new(key.into());
    // AES-GCM only fails on inputs past 64 GiB.
    let body = cipher.encrypt(Nonce::from_slice(nonce), Payload { msg: plaintext, aad }).unwrap_or_default();
    let mut out = Vec::with_capacity(1 + NONCE + body.len());
    out.push(VERSION);
    out.extend_from_slice(nonce);
    out.extend_from_slice(&body);
    out
}

pub fn open(key: &Key, sealed: &[u8], aad: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if sealed.len() < 1 + NONCE + TAG || sealed[0] != VERSION {
        return Err(CryptoError::Open);
    }
    let (nonce, body) = sealed[1..].split_at(NONCE);
    Aes256Gcm::new(key.into())
        .decrypt(Nonce::from_slice(nonce), Payload { msg: body, aad })
        .map_err(|_| CryptoError::Open)
}

/// Spaces up to the next multiple of 1024 bytes, so a blob's size says little about its note.
pub fn pad(mut plaintext: Vec<u8>) -> Vec<u8> {
    let len = plaintext.len().max(1).div_ceil(PAD_TO) * PAD_TO;
    plaintext.resize(len, b' ');
    plaintext
}

pub fn to_base64(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

pub fn from_base64(text: &str) -> Result<Vec<u8>, CryptoError> {
    STANDARD.decode(text).map_err(|_| CryptoError::Open)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// A random UUID v4, lowercase with hyphens.
pub fn new_id() -> Result<String, CryptoError> {
    let mut b: [u8; 16] = random()?;
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h = hex::encode(b);
    Ok(format!("{}-{}-{}-{}-{}", &h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..]))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use serde_json::Value;

    use super::*;
    use crate::sync::object::Payload as ObjectPayload;

    fn vectors() -> Value {
        serde_json::from_str(include_str!("../../docs/sync-vectors.json")).unwrap()
    }

    fn bytes(v: &Value) -> Vec<u8> {
        hex::decode(v.as_str().unwrap()).unwrap()
    }

    fn key(v: &Value) -> Key {
        bytes(v).try_into().unwrap()
    }

    #[test]
    fn the_key_schedule_matches_the_vectors() {
        let v = &vectors()["kdf"];
        let email = v["email"].as_str().unwrap();
        assert_eq!(salt(email), v["salt"].as_str().unwrap());
        assert_eq!(v["iterations"].as_u64().unwrap(), u64::from(ITERATIONS));
        let master = master_key(email, v["password"].as_str().unwrap(), ITERATIONS);
        assert_eq!(master.to_vec(), bytes(&v["masterKey"]));
        let keys = Keys::from_master(&master);
        assert_eq!(keys.auth.to_vec(), bytes(&v["authKey"]));
        assert_eq!(keys.wrap.to_vec(), bytes(&v["wrapKey"]));
        assert_eq!(keys.auth_password(), v["authPassword"].as_str().unwrap());
    }

    #[test]
    fn the_wrapped_vault_key_matches_the_vectors() {
        let v = &vectors()["vaultKey"];
        let wrap = key(&vectors()["kdf"]["wrapKey"]);
        let nonce: [u8; 12] = bytes(&v["nonce"]).try_into().unwrap();
        let vault = bytes(&v["key"]);
        let sealed = seal_with_nonce(&wrap, &nonce, &vault, v["aad"].as_str().unwrap().as_bytes());
        assert_eq!(to_base64(&sealed), v["wrappedVaultKey"].as_str().unwrap());
        assert_eq!(open(&wrap, &sealed, VAULT_KEY_AAD).unwrap(), vault);
    }

    #[test]
    fn the_object_blob_matches_the_vectors() {
        let v = &vectors()["object"];
        let plaintext = v["plaintext"].as_str().unwrap();
        let payload: ObjectPayload = serde_json::from_str(plaintext).unwrap();
        assert_eq!(payload.to_json(), plaintext.as_bytes(), "the payload serialises to the same bytes");

        let padded = pad(plaintext.as_bytes().to_vec());
        assert_eq!(padded.len() as u64, v["paddedLength"].as_u64().unwrap());
        let id = v["id"].as_str().unwrap();
        let nonce: [u8; 12] = bytes(&v["nonce"]).try_into().unwrap();
        let sealed = seal_with_nonce(&key(&v["key"]), &nonce, &padded, id.as_bytes());
        assert_eq!(to_base64(&sealed), v["blob"].as_str().unwrap());

        let opened = open(&key(&v["key"]), &from_base64(v["blob"].as_str().unwrap()).unwrap(), id.as_bytes()).unwrap();
        assert_eq!(ObjectPayload::parse(&opened).unwrap(), payload);
    }

    #[test]
    fn open_refuses_other_versions_and_other_aad() {
        let key = [7; 32];
        let mut sealed = seal(&key, b"hello", b"a").unwrap();
        assert!(open(&key, &sealed, b"b").is_err());
        sealed[0] = 2;
        assert!(open(&key, &sealed, b"a").is_err());
    }

    #[test]
    fn padding_rounds_up_to_whole_kilobytes() {
        assert_eq!(pad(vec![b'x'; 1]).len(), 1024);
        assert_eq!(pad(vec![b'x'; 1024]).len(), 1024);
        assert_eq!(pad(vec![b'x'; 1025]).len(), 2048);
    }

    #[test]
    fn ids_are_uuid_v4() {
        let id = new_id().unwrap();
        assert_eq!(id.len(), 36);
        assert_eq!(&id[14..15], "4");
        assert!(matches!(&id[19..20], "8" | "9" | "a" | "b"));
        assert_eq!(id, id.to_lowercase());
    }
}
