//! Sharing: the server that lends this computer's Claude Code to the
//! user's other devices, started and stopped as the setting changes, and the address and
//! access code a device needs.

use std::sync::Arc;

use nd::prelude::*;

use crate::ai::chat::Provider;
use crate::ai::claude_code::ClaudeCode;
use crate::ai::host::{self, CODE_ACCOUNT, Server};
use crate::settings::NotesSettings;

/// The port and model the server runs with.
type Wanted = (u16, String);

#[derive(Clone, Debug, PartialEq, Default)]
enum Status {
    #[default]
    Off,
    Starting,
    On,
    Failed(String),
}

#[derive(Default)]
struct Running {
    wanted: Option<Wanted>,
    server: Option<Server>,
    status: Status,
    code: Option<String>,
}

fn state(cx: &mut App) -> &mut Running {
    if !cx.has_global::<Running>() {
        cx.set_global(Running::default());
    }
    cx.global_mut()
}

fn wanted(cx: &App) -> Option<Wanted> {
    let settings = cx.settings::<NotesSettings>();
    let model = match settings.assistant.provider {
        Provider::ClaudeCode => settings.assistant.model(),
        Provider::Anthropic | Provider::OpenAi => String::new(),
    };
    settings.sharing.enabled.then(|| (settings.sharing.port, model))
}

/// Starts, restarts or stops the server so it matches the settings. Cheap when nothing changed.
pub fn follow(cx: &mut App) {
    let want = wanted(cx);
    let running = state(cx);
    if running.wanted == want {
        return;
    }
    running.wanted = want.clone();
    running.server = None;
    let Some(want) = want else {
        running.status = Status::Off;
        return;
    };
    running.status = Status::Starting;
    let fresh = running.code.is_none();
    nd::system::spawn(
        async move {
            if !fresh {
                return Ok(None);
            }
            // The keychain read can wait on the OS; the code is made the first time.
            match crate::secrets::get(CODE_ACCOUNT)? {
                Some(code) if !code.trim().is_empty() => Ok(Some(code)),
                _ => {
                    let code = host::new_code()?;
                    crate::secrets::set(CODE_ACCOUNT, &code)?;
                    Ok(Some(code))
                }
            }
        },
        move |code: std::io::Result<Option<String>>, cx| {
            let running = state(cx);
            if running.wanted.as_ref() != Some(&want) {
                return;
            }
            match code {
                Ok(Some(code)) => running.code = Some(code),
                Ok(None) => {}
                Err(e) => running.status = Status::Failed(format!("Could not read the keychain: {e}")),
            }
            if let Some(code) = running.code.clone().filter(|_| running.status == Status::Starting) {
                let (port, model) = want;
                match Server::start(port, code, Arc::new(ClaudeCode { model })) {
                    Ok(server) => {
                        running.server = Some(server);
                        running.status = Status::On;
                    }
                    Err(e) => running.status = Status::Failed(format!("Could not listen on port {port}: {e}")),
                }
            }
            cx.refresh_windows();
        },
    );
}

/// Replaces the access code; devices that had the old one have to be given the new one.
fn new_code(cx: &mut App) {
    nd::system::spawn(
        async move {
            let code = host::new_code()?;
            crate::secrets::set(CODE_ACCOUNT, &code)?;
            Ok(code)
        },
        |code: std::io::Result<String>, cx| {
            let running = state(cx);
            match code {
                Ok(code) => {
                    running.code = Some(code);
                    running.wanted = None;
                }
                Err(e) => running.status = Status::Failed(format!("Could not save the code: {e}")),
            }
            follow(cx);
            cx.refresh_windows();
        },
    );
    let _ = cx;
}

fn copy(text: String) {
    nd::system::spawn(async move { nd::system::system().clipboard().write_text(text).await }, |_, _| {});
}

/// The section of the Assistant pane that says where a device connects; none while sharing is
/// off. Brings the server in line with the settings first.
pub fn section(cx: &mut App) -> Option<FormSection> {
    follow(cx);
    let running = state(cx);
    let (port, _) = running.wanted.clone()?;
    let (status, code) = (running.status.clone(), running.code.clone());
    let theme = cx.theme();
    let address = format!("http://{}:{port}", host::local_address().unwrap_or_else(|| "this-computer".to_owned()));
    let footer = match &status {
        Status::Failed(why) => why.clone(),
        _ => "On your phone, choose Your Computer under Ask in Settings and enter this address and code. The \
                  phone has to reach this computer: the same network, or a VPN between them."
            .to_owned(),
    };
    let mut section = form_section().title("Sharing with your devices").footer(footer).child(
        form_row("Address")
            .id("sharing-address-row")
            .child(label(address).id("sharing-address").selectable().secondary()),
    );
    if let Some(code) = code {
        let copied = code.clone();
        section = section.child(
            form_row("Access code").id("sharing-code-row").child(
                h_flex()
                    .gap(theme.spacing.sm)
                    .child(label(code).id("sharing-code").selectable().secondary())
                    .child(button("sharing-copy", "Copy").on_click(move |_: &ClickEvent, _, _| copy(copied.clone())))
                    .child(button("sharing-new-code", "New Code").on_click(|_: &ClickEvent, _, cx| new_code(cx))),
            ),
        );
    }
    Some(section)
}
