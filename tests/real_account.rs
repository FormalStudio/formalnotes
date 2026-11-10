#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Tier 3 against the real server: create an account in the Settings window, watch the
//! library sync, and capture the Account pane and the sidebar's status line. The account is
//! deleted at the end. `nd test --tier real -- --ignored real_account`; screenshots go to
//! `FORMALNOTES_SHOTS` (default: the temp folder).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use std::collections::HashMap;
use std::sync::Mutex;

use formalnotes::sync::account::{self, Keystore};
use formalnotes::sync::picture;
use formalnotes::sync::remote::{Http, Remote, base_url};
use nd_test::{LaunchOptions, launch_app};

/// The test's own device. Reading the app's keychain entry from this binary would stop on a
/// macOS prompt, so it signs in beside the app instead.
#[derive(Default)]
struct Memory(Mutex<HashMap<String, String>>);

impl Keystore for Memory {
    fn get(&self, name: &str) -> std::io::Result<Option<String>> {
        Ok(self.0.lock().unwrap().get(name).cloned())
    }
    fn set(&self, name: &str, value: &str) -> std::io::Result<()> {
        self.0.lock().unwrap().insert(name.to_owned(), value.to_owned());
        Ok(())
    }
    fn delete(&self, name: &str) -> std::io::Result<()> {
        self.0.lock().unwrap().remove(name);
        Ok(())
    }
}

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
    // The toolbar's account button opens Settings on the Account pane.
    app.get_by_test_id("account").click().unwrap();
    until("the account form", || app.get_by_test_id("account-email").count().unwrap_or(0) > 0);
    app.screenshot(&out.join("formalnotes-account-signed-out.png"), None).unwrap();

    app.get_by_test_id("account-email").fill(&email).unwrap();
    app.get_by_test_id("account-password").fill(password).unwrap();
    app.get_by_test_id("account-create").click().unwrap();
    until("the signed-in pane", || app.get_by_test_id("account-sign-out").count().unwrap_or(0) > 0);

    let remote = Http::new(base_url());
    let device = Memory::default();
    let session = account::sign_in(&remote, &device, &email, password).unwrap();
    until("the library on the server", || remote.usage(&session.token).is_ok_and(|u| u.objects == 3));
    app.screenshot(&out.join("formalnotes-account-signed-in.png"), None).unwrap();

    // A picture another device put shows after the next sync, in the pane and in the toolbar.
    let photo = image::RgbImage::from_fn(600, 400, |x, y| image::Rgb([(x / 3) as u8, (y / 2) as u8, 180]));
    let mut png = Vec::new();
    photo.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    let jpeg = picture::square_jpeg(&png).unwrap();
    let version = remote.put_avatar(&session.token, &picture::seal(&session.vault, &jpeg).unwrap()).unwrap();
    assert_eq!(remote.usage(&session.token).unwrap().avatar, Some(version));
    app.get_by_test_id("account-sync-now").click().unwrap();
    until("the remove button", || app.get_by_test_id("account-picture-remove").count().unwrap_or(0) > 0);
    std::thread::sleep(Duration::from_secs(1));
    app.screenshot(&out.join("formalnotes-account-picture.png"), None).unwrap();
    app.screenshot(&out.join("formalnotes-toolbar-picture.png"), Some(1)).unwrap();
    app.get_by_test_id("account-picture-remove").click().unwrap();
    until("the picture gone from the server", || remote.usage(&session.token).is_ok_and(|u| u.avatar.is_none()));

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

    // Sign out in the app so its session leaves the keychain; the status line opens the pane.
    app.get_by_test_id("sync-status-line").click().unwrap();
    until("the signed-in pane", || app.get_by_test_id("account-sign-out").count().unwrap_or(0) > 0);
    app.screenshot(&out.join("formalnotes-account-offline.png"), None).unwrap();
    app.get_by_test_id("account-sign-out").scroll_into_view().unwrap();
    app.get_by_test_id("account-sign-out").click().unwrap();
    until("the account form", || app.get_by_test_id("account-email").count().unwrap_or(0) > 0);
    drop(app);

    account::delete_account(&remote, &device, &session, password).unwrap();
    assert!(remote.usage(&session.token).is_err(), "the account is gone");
}
