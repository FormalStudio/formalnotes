#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Tier 3 against the real server: create an account in the Settings window, watch the
//! library sync, and capture the Account pane and the sidebar's status line. The account is
//! deleted at the end. `nd test --tier real -- --ignored real_account`; screenshots go to
//! `FORMALNOTES_SHOTS` (default: the temp folder).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use formalnotes::sync::account::{self, Keychain};
use formalnotes::sync::remote::{Http, Remote, base_url};
use nd_test::{LaunchOptions, launch_app};

fn shots() -> PathBuf {
    std::env::var_os("FORMALNOTES_SHOTS").map_or_else(std::env::temp_dir, PathBuf::from)
}

fn until(what: &str, mut ok: impl FnMut() -> bool) {
    let start = Instant::now();
    while !ok() {
        assert!(start.elapsed() < Duration::from_secs(60), "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(250));
    }
}

#[test]
#[ignore = "creates an account on the real server"]
fn real_account_signs_up_syncs_and_shows_its_status() {
    let lib = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(lib.path().join("Journal")).unwrap();
    std::fs::write(lib.path().join("Groceries.md"), "# Groceries\n\nOat milk, lemons.\n").unwrap();
    std::fs::write(lib.path().join("Journal/Slow start.md"), "# Slow start\n\nCoffee on the balcony.\n").unwrap();
    std::fs::write(
        config.path().join("config.toml"),
        format!("library_root = {:?}\n", lib.path().display().to_string()),
    )
    .unwrap();

    let email = format!("screens-{}@example.com", &formalnotes::sync::crypto::new_id().unwrap()[..8]);
    let password = "correct horse battery staple";
    let app = launch_app(
        LaunchOptions::new(env!("CARGO_BIN_EXE_formalnotes"))
            .env("ND_APP_CONFIG_DIR", config.path().display().to_string()),
    )
    .expect("launch");
    let out = shots();

    until("the main window", || app.get_by_test_id("sidebar").count().unwrap_or(0) > 0);
    let settings = || {
        let windows = app.windows().unwrap();
        windows.as_array().unwrap().iter().find(|w| w["title"] == "Settings").and_then(|w| w["id"].as_u64())
    };
    until("the settings window", || {
        if settings().is_none() {
            // To the main window by id: a freshly launched app may not be key yet.
            let sent = app.call("keys", serde_json::json!({ "keys": "cmd-,", "window": 1 }));
            eprintln!("keys: {sent:?}");
            std::thread::sleep(Duration::from_millis(500));
        }
        settings().is_some()
    });
    eprintln!("TREE {}", app.tree(settings()).unwrap().snapshot());
    app.get_by_text("Account").first().click().unwrap();
    until("the account form", || app.get_by_test_id("account-email").count().unwrap_or(0) > 0);
    app.screenshot(&out.join("formalnotes-account-signed-out.png"), None).unwrap();

    app.get_by_test_id("account-email").fill(&email).unwrap();
    app.get_by_test_id("account-password").fill(password).unwrap();
    app.get_by_test_id("account-create").click().unwrap();
    until("the signed-in pane", || app.get_by_test_id("account-sign-out").count().unwrap_or(0) > 0);

    let remote = Http::new(base_url());
    let session = account::load(&Keychain).unwrap().expect("the session is in the keychain");
    until("the library on the server", || remote.usage(&session.token).is_ok_and(|u| u.objects == 3));
    app.screenshot(&out.join("formalnotes-account-signed-in.png"), None).unwrap();

    drop(app);

    // Signed in, with the server out of reach: the sidebar says so.
    let app = launch_app(
        LaunchOptions::new(env!("CARGO_BIN_EXE_formalnotes"))
            .env("ND_APP_CONFIG_DIR", config.path().display().to_string())
            .env("FORMALNOTES_SYNC_URL", "http://127.0.0.1:9"),
    )
    .expect("relaunch");
    until("the offline line", || app.get_by_text("Offline").count().unwrap_or(0) > 0);
    app.screenshot(&out.join("formalnotes-sidebar-offline.png"), None).unwrap();
    drop(app);

    account::delete_account(&remote, &Keychain, &session, password).unwrap();
    assert!(remote.usage(&session.token).is_err(), "the account is gone");
}
