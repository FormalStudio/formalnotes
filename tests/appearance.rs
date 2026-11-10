//! Tier 2: the appearance audit. Every window in light, dark and matugen themes on each backend
//! kind; fails on a colour that is not a theme token and on text under WCAG contrast. Runs in
//! `nd check` and `nd test`. It audits the demo library, never the user's notes.

use formalnotes::sync::Status;
use formalnotes::sync::remote::Usage;
use nd::core::WindowId;
use nd::prelude::App;

/// The Account tab, signed out or signed in with `account`.
fn settings(cx: &mut App, account: Option<(Usage, Status)>) -> WindowId {
    match account {
        Some((usage, status)) => preview(cx, Some(usage.email.clone()), Some(usage), status),
        None => preview(cx, None, None, Status::SignedOut),
    }
    match formalnotes::settings::open_tab(cx, formalnotes::settings::ACCOUNT_TAB) {
        Some(window) => window.id(),
        None => panic!("a settings window was already open"),
    }
}

/// The sync state every window shows: it is app-wide, so each window sets its own.
fn preview(cx: &mut App, email: Option<String>, usage: Option<Usage>, status: Status) {
    formalnotes::sync::model(cx).update(cx, |m, _| m.preview(email, usage, status));
}
mod support;

fn main() {
    nd::audit::Audit::new("formalnotes", formalnotes::app)
        .window("main", |cx| {
            preview(cx, None, None, Status::SignedOut);
            let window = formalnotes::open_demo(cx);
            // No host pumps the disk worker here: wait for the library so the cards are audited.
            let store = window.root().read(cx).store().clone();
            store.update(cx, |store, cx| store.settle(cx));
            window.id()
        })
        .window("ask", |cx| {
            // A scripted provider answers, so the pane is audited with a question, an answer
            // with a citation and its source row.
            let script = support::Script::new([
                support::call("c1", "search_notes", serde_json::json!({ "query": "beta" })),
                support::answer("The beta ships on the **14th** [[Projects/Meeting notes.md]]."),
            ]);
            cx.set_global(formalnotes::ai::Connector(std::sync::Arc::new(support::Fake(script))));
            let window = formalnotes::open_demo(cx);
            let store = window.root().read(cx).store().clone();
            store.update(cx, |store, cx| store.settle(cx));
            let root = window.root();
            root.update(cx, |root, cx| root.set_ask_open(true, cx));
            let ask = root.read(cx).ask().clone();
            ask.update(cx, |ask, cx| {
                ask.ask("When does the beta ship?", cx);
                ask.settle(cx);
            });
            window.id()
        })
        .window("settings", |cx| match formalnotes::settings::open_window(cx) {
            Some(window) => window.id(),
            None => panic!("a settings window was already open"),
        })
        .window("settings-account", |cx| settings(cx, None))
        .window("settings-account-signed-in", |cx| {
            let usage = Usage { email: "ada@example.com".into(), objects: 128, bytes: 2_400_000, avatar: None };
            let synced = Status::Synced { at: jiff::Timestamp::now(), problems: Vec::new() };
            settings(cx, Some((usage, synced)))
        })
        .window("main-sync-offline", |cx| {
            preview(cx, Some("ada@example.com".into()), None, Status::Offline);
            let window = formalnotes::open_demo(cx);
            let store = window.root().read(cx).store().clone();
            store.update(cx, |store, cx| store.settle(cx));
            window.id()
        })
        .run()
}
