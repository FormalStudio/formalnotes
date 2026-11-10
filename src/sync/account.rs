//! The account: sign-up, sign-in, sign-out, a new password and deletion, as `docs/protocol.md`
//! orders the calls. The session token and the vault key live in a [`Keystore`] (the OS
//! keychain in the app); the password is used for the keys and dropped.

use std::io;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use super::crypto::{self, Key, Keys, VAULT_KEY_AAD};
use super::remote::{Remote, RemoteError};

/// The keychain entry that holds the signed-in session.
const ENTRY: &str = "sync-session";
pub const MIN_PASSWORD: usize = 8;

/// Secrets by name. The app's is the OS keychain; tests keep them in memory.
pub trait Keystore: Send + Sync {
    fn get(&self, name: &str) -> io::Result<Option<String>>;
    fn set(&self, name: &str, value: &str) -> io::Result<()>;
    fn delete(&self, name: &str) -> io::Result<()>;
}

/// The keychain through `crate::secrets`.
pub struct Keychain;

impl Keystore for Keychain {
    fn get(&self, name: &str) -> io::Result<Option<String>> {
        crate::secrets::get(name)
    }

    fn set(&self, name: &str, value: &str) -> io::Result<()> {
        crate::secrets::set(name, value)
    }

    fn delete(&self, name: &str) -> io::Result<()> {
        crate::secrets::delete(name)
    }
}

#[derive(Clone)]
pub struct Session {
    pub email: String,
    pub token: String,
    pub vault: Key,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session").field("email", &self.email).finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub enum AccountError {
    /// Sign-in with an email and password that do not match an account.
    WrongCredentials,
    /// The password given to change it or to delete the account.
    WrongPassword,
    Exists,
    ShortPassword,
    Remote(RemoteError),
    Keystore(io::Error),
    Crypto(crypto::CryptoError),
}

impl std::fmt::Display for AccountError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AccountError::WrongCredentials => f.write_str("The email or password is not right."),
            AccountError::WrongPassword => f.write_str("That password is not right."),
            AccountError::Exists => f.write_str("There is already an account with this email. Sign in instead."),
            AccountError::ShortPassword => write!(f, "Use a password of at least {MIN_PASSWORD} characters."),
            AccountError::Remote(RemoteError::Offline(_)) => f.write_str("The sync server could not be reached."),
            AccountError::Remote(RemoteError::Unauthorized) => f.write_str("The session has ended. Sign in again."),
            AccountError::Remote(e) => write!(f, "The server said: {e}"),
            AccountError::Keystore(e) => write!(f, "The keychain could not be used: {e}"),
            AccountError::Crypto(e) => write!(f, "The vault key could not be opened: {e}"),
        }
    }
}

impl std::error::Error for AccountError {}

impl From<RemoteError> for AccountError {
    fn from(e: RemoteError) -> Self {
        match e.code() {
            Some("wrong_password") => AccountError::WrongPassword,
            Some("invalid_email_or_password") => AccountError::WrongCredentials,
            Some(code) if code.starts_with("user_already_exists") => AccountError::Exists,
            _ => AccountError::Remote(e),
        }
    }
}

impl From<crypto::CryptoError> for AccountError {
    fn from(e: crypto::CryptoError) -> Self {
        AccountError::Crypto(e)
    }
}

pub type Result<T> = std::result::Result<T, AccountError>;

pub fn normalize(email: &str) -> String {
    email.trim().to_lowercase()
}

/// The session the keychain holds, if any.
pub fn load(keys: &dyn Keystore) -> Result<Option<Session>> {
    let Some(stored) = keys.get(ENTRY).map_err(AccountError::Keystore)? else { return Ok(None) };
    let value: serde_json::Value = serde_json::from_str(&stored).unwrap_or_default();
    let vault = value["vaultKey"].as_str().and_then(|v| STANDARD.decode(v).ok()).and_then(|v| Key::try_from(v).ok());
    match (value["email"].as_str(), value["token"].as_str(), vault) {
        (Some(email), Some(token), Some(vault)) => {
            Ok(Some(Session { email: email.to_owned(), token: token.to_owned(), vault }))
        }
        _ => Ok(None),
    }
}

fn store(keys: &dyn Keystore, session: &Session) -> Result<()> {
    let value = serde_json::json!({
        "email": session.email,
        "token": session.token,
        "vaultKey": STANDARD.encode(session.vault),
    });
    keys.set(ENTRY, &value.to_string()).map_err(AccountError::Keystore)
}

/// Removes the session from the keychain. Local notes stay.
pub fn forget(keys: &dyn Keystore) -> Result<()> {
    keys.delete(ENTRY).map_err(AccountError::Keystore)
}

/// Makes a vault key, wraps it and puts it, or opens the one another device put first.
fn new_vault(remote: &dyn Remote, token: &str, keys: &Keys) -> Result<Key> {
    let vault: Key = crypto::random()?;
    let wrapped = crypto::to_base64(&crypto::seal(&keys.wrap, &vault, VAULT_KEY_AAD)?);
    match remote.put_keys(token, &wrapped) {
        Ok(()) => Ok(vault),
        Err(e) if e.code() == Some("keys_exist") => match remote.keys(token)? {
            Some(wrapped) => open_vault(&wrapped, keys),
            None => Err(AccountError::Remote(e)),
        },
        Err(e) => Err(e.into()),
    }
}

fn open_vault(wrapped: &str, keys: &Keys) -> Result<Key> {
    let vault = crypto::open(&keys.wrap, &crypto::from_base64(wrapped)?, VAULT_KEY_AAD)?;
    Key::try_from(vault).map_err(|_| AccountError::Crypto(crypto::CryptoError::Open))
}

pub fn sign_up(remote: &dyn Remote, store_in: &dyn Keystore, email: &str, password: &str) -> Result<Session> {
    if password.chars().count() < MIN_PASSWORD {
        return Err(AccountError::ShortPassword);
    }
    let email = normalize(email);
    let keys = Keys::derive(&email, password);
    let token = remote.sign_up(&email, &keys.auth_password())?;
    let vault = new_vault(remote, &token, &keys)?;
    let session = Session { email, token, vault };
    store(store_in, &session)?;
    Ok(session)
}

pub fn sign_in(remote: &dyn Remote, store_in: &dyn Keystore, email: &str, password: &str) -> Result<Session> {
    let email = normalize(email);
    let keys = Keys::derive(&email, password);
    let token = match remote.sign_in(&email, &keys.auth_password()) {
        Err(RemoteError::Unauthorized) => return Err(AccountError::WrongCredentials),
        other => other?,
    };
    let vault = match remote.keys(&token)? {
        Some(wrapped) => open_vault(&wrapped, &keys)?,
        None => new_vault(remote, &token, &keys)?,
    };
    let session = Session { email, token, vault };
    store(store_in, &session)?;
    Ok(session)
}

/// Ends the session on the server when it can, and here in any case.
pub fn sign_out(remote: &dyn Remote, store_in: &dyn Keystore, session: &Session) -> Result<()> {
    let _ = remote.sign_out(&session.token);
    forget(store_in)
}

/// A new password: the vault key is wrapped again under the new password's key. Other
/// devices are signed out by the server; this one stays signed in.
pub fn change_password(remote: &dyn Remote, session: &Session, current: &str, new: &str) -> Result<()> {
    if new.chars().count() < MIN_PASSWORD {
        return Err(AccountError::ShortPassword);
    }
    let old = Keys::derive(&session.email, current);
    let next = Keys::derive(&session.email, new);
    let wrapped = crypto::to_base64(&crypto::seal(&next.wrap, &session.vault, VAULT_KEY_AAD)?);
    remote.change_password(&session.token, &old.auth_password(), &next.auth_password(), &wrapped)?;
    Ok(())
}

/// Deletes the account and everything the server holds for it. Local notes stay.
pub fn delete_account(remote: &dyn Remote, store_in: &dyn Keystore, session: &Session, password: &str) -> Result<()> {
    let keys = Keys::derive(&session.email, password);
    remote.delete_account(&session.token, &keys.auth_password())?;
    forget(store_in)
}
