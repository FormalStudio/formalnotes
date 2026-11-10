#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Tier 3 with the real Claude Code: the app shares it, and a device's client asks the demo
//! library through it. `nd test --tier real -- --run-ignored only real_sharing`; the
//! screenshot goes to `FORMALNOTES_SHOTS` (default: the temp folder).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use formalnotes::ai::agent::{self, Progress};
use formalnotes::ai::chat::{Cancel, Endpoint, Http, Provider};
use formalnotes::ai::tools::LibraryTools;
use formalnotes::library::disk;
use nd_test::{LaunchOptions, launch_app};

const PORT: u16 = 47611;
/// The app's secrets for this run, apart from the installed app's.
const KEYCHAIN: &str = "dev.formalsnake.formalnotes.real-sharing";

fn until(what: &str, mut ok: impl FnMut() -> bool) {
    let start = Instant::now();
    while !ok() {
        assert!(start.elapsed() < Duration::from_secs(60), "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(250));
    }
}

#[test]
#[ignore = "runs the claude command on this computer's account"]
fn real_sharing_answers_a_device_with_this_computers_claude_code() {
    let config = tempfile::tempdir().unwrap();
    std::fs::write(config.path().join("config.toml"), format!("[sharing]\nenabled = true\nport = {PORT}\n")).unwrap();
    let app = launch_app(
        LaunchOptions::new(env!("CARGO_BIN_EXE_formalnotes"))
            .env("FORMALNOTES_DEMO", "1")
            .env("FORMALNOTES_KEYCHAIN", KEYCHAIN)
            .env("ND_APP_CONFIG_DIR", config.path().display().to_string()),
    )
    .expect("launch");
    until("the main window", || app.get_by_test_id("sidebar").count().unwrap_or(0) > 0);
    // A question without a key fails with a button that opens Settings on the Assistant tab,
    // where the sharing address and code are too.
    app.get_by_test_id("ask").first().click().unwrap();
    let field = app.get_by_test_id("ask-field");
    field.fill("Anything?").unwrap();
    field.press("enter").unwrap();
    until("the failed answer", || app.get_by_text("Open Settings").count().unwrap_or(0) > 0);
    app.get_by_text("Open Settings").first().click().unwrap();
    until("the access code", || app.get_by_test_id("sharing-code").count().unwrap_or(0) > 0);
    let code = app.get_by_test_id("sharing-code").text().unwrap();
    assert_eq!(code.len(), 20, "{code}");
    assert!(app.get_by_test_id("sharing-address").text().unwrap().ends_with(&format!(":{PORT}")));
    let out = std::env::var_os("FORMALNOTES_SHOTS").map_or_else(std::env::temp_dir, PathBuf::from);
    app.screenshot(&out.join("formalnotes-settings-sharing.png"), None).unwrap();

    // The device: its own copy of the library and its own tools, this computer's model.
    let root = std::env::temp_dir().join(format!("formalnotes-demo-{}", app.pid()));
    let library = Arc::new(disk::scan(&disk::Root::new(root), &mut disk::ScanCache::default()).unwrap());
    let device = |key: &str| Http {
        endpoint: Endpoint {
            provider: Provider::Anthropic,
            model: String::new(),
            base_url: format!("http://127.0.0.1:{PORT}"),
            key: key.to_owned(),
        },
    };
    let ask = |device: &Http| {
        let request =
            formalnotes::ai::ask::request(&[], "How long does the focaccia bake, and at what temperature?", "Monday");
        let mut called = Vec::new();
        let added =
            agent::run(device, request, &mut LibraryTools::new(library.clone()), &Cancel::default(), &mut |progress| {
                if let Progress::Tool { name, .. } = progress {
                    called.push(name);
                }
            });
        (added.map(|added| added.last().unwrap().text()), called)
    };

    let (refused, _) = ask(&device("not-the-code"));
    assert_eq!(refused, Err(formalnotes::ai::chat::AiError::Unauthorized));
    let (answer, called) = ask(&device(&code));
    drop(app);
    let _ = std::process::Command::new("security")
        .args(["delete-generic-password", "-s", KEYCHAIN, "-a", "sharing-access-code"])
        .output();
    let answer = answer.unwrap();
    eprintln!("tools: {called:?}\nanswer: {answer}");
    assert!(!called.is_empty(), "it used the device's tools");
    assert!(answer.contains("230") && answer.contains("22"), "{answer}");
}
