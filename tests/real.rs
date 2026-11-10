#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Tier 3: the app as its own process on the demo library, through real clicks. Each run leaves
//! captures of the table and of a note's properties, light and dark, in `target/nd/shots/`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use nd_test::{AppHandle, LaunchOptions, expect, launch_app};

/// The table's row height on AppKit, in points, and the blank row that keeps the first note
/// clear of the column header.
const ROW: f32 = 32.;
const HEADER: f32 = 28.;

fn shots() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/nd/shots");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A real click in the middle of `test_id`, for the cards that take taps.
fn tap(app: &AppHandle, test_id: &str) {
    let node = app.get_by_test_id(test_id).node().unwrap();
    let b = node.bounds.unwrap_or_else(|| panic!("`{test_id}` has no bounds"));
    app.mouse().click(b.origin.x + b.size.width / 2., b.origin.y + b.size.height / 2.).unwrap();
}

/// A real click on the title of the table's row `index`. AppKit reports every cell at the table's
/// own origin, so the point is worked out from the table.
fn tap_row(app: &AppHandle, index: usize) {
    let b = app.get_by_test_id("table").node().unwrap().bounds.unwrap();
    app.mouse().click(b.origin.x + 100., b.origin.y + HEADER + ROW * index as f32 + ROW / 2.).unwrap();
}

fn table_then_note(theme: &str) {
    let config = tempfile::tempdir().unwrap();
    std::fs::write(config.path().join("config.toml"), format!("theme = \"{theme}\"\n")).unwrap();
    let app = launch_app(
        LaunchOptions::new(env!("CARGO_BIN_EXE_formalnotes"))
            .env("FORMALNOTES_DEMO", "1")
            .env("ND_APP_CONFIG_DIR", config.path().to_string_lossy()),
    )
    .expect("launch");

    expect(&app.get_by_test_id("face:folder:Reading")).to_be_visible().unwrap();
    // A click on a window that is not yet key only activates it, so the card may need a second.
    for _ in 0..3 {
        tap(&app, "face:folder:Reading");
        std::thread::sleep(Duration::from_millis(800));
        if app.get_by_test_id("table").count().unwrap() > 0 {
            break;
        }
    }
    expect(&app.get_by_test_id("table")).to_be_visible().unwrap();
    std::thread::sleep(Duration::from_millis(1500));
    app.screenshot(&shots().join(format!("table-{theme}.png")), None).unwrap();
    std::fs::write(shots().join(format!("table-{theme}.txt")), app.snapshot().unwrap()).unwrap();

    tap_row(&app, 0);
    expect(&app.get_by_test_id("note-back")).to_be_visible().unwrap();
    expect(&app.get_by_test_id("property-name:Status")).to_be_visible().unwrap();
    std::thread::sleep(Duration::from_millis(1000));
    app.screenshot(&shots().join(format!("note-{theme}.png")), None).unwrap();
}

#[test]
fn real_a_table_row_opens_its_note_with_properties_light() {
    table_then_note("light");
}

#[test]
fn real_a_table_row_opens_its_note_with_properties_dark() {
    table_then_note("dark");
}
