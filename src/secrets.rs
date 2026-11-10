//! Secrets in the OS keychain (Keychain, the Secret Service, Credential Manager), never in the
//! settings file. Each call waits for the OS store, so call them off the main thread.

const SERVICE: &str = "dev.formalsnake.formalnotes";

/// The secret stored under `account`, or `None` when there is none.
pub fn get(account: &str) -> Result<Option<String>, String> {
    pollster::block_on(nd::system::system().credentials().get(SERVICE, account)).map_err(|e| e.to_string())
}

pub fn set(account: &str, secret: &str) -> Result<(), String> {
    pollster::block_on(nd::system::system().credentials().set(SERVICE, account, secret)).map_err(|e| e.to_string())
}

/// Deleting a secret that is not there succeeds.
pub fn delete(account: &str) -> Result<(), String> {
    pollster::block_on(nd::system::system().credentials().delete(SERVICE, account)).map_err(|e| e.to_string())
}
