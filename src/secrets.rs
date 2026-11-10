//! Secrets in the OS keychain (Keychain, the Secret Service, Credential Manager) through nd's
//! credentials, under one service name. Blocking: call from a background thread.

use std::io;

const SERVICE: &str = "dev.formalsnake.formalnotes";

/// `FORMALNOTES_KEYCHAIN` names another service. A test binary that touched the installed
/// app's entries would stop on the OS asking the user for permission.
fn service() -> String {
    std::env::var("FORMALNOTES_KEYCHAIN").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| SERVICE.to_owned())
}

fn wait<T>(pending: nd::system::Pending<T>) -> io::Result<T> {
    pollster::block_on(pending).map_err(|e| io::Error::other(e.to_string()))
}

pub fn get(name: &str) -> io::Result<Option<String>> {
    wait(nd::system::system().credentials().get(service(), name))
}

pub fn set(name: &str, secret: &str) -> io::Result<()> {
    wait(nd::system::system().credentials().set(service(), name, secret))
}

pub fn delete(name: &str) -> io::Result<()> {
    wait(nd::system::system().credentials().delete(service(), name))
}
