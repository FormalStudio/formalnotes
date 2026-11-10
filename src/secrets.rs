//! Secrets in the OS keychain (Keychain, the Secret Service, Credential Manager) through nd's
//! credentials, under one service name. Blocking: call from a background thread.

use std::io;

const SERVICE: &str = "dev.formalsnake.formalnotes";

fn wait<T>(pending: nd::system::Pending<T>) -> io::Result<T> {
    pollster::block_on(pending).map_err(|e| io::Error::other(e.to_string()))
}

pub fn get(name: &str) -> io::Result<Option<String>> {
    wait(nd::system::system().credentials().get(SERVICE, name))
}

pub fn set(name: &str, secret: &str) -> io::Result<()> {
    wait(nd::system::system().credentials().set(SERVICE, name, secret))
}

pub fn delete(name: &str) -> io::Result<()> {
    wait(nd::system::system().credentials().delete(SERVICE, name))
}
