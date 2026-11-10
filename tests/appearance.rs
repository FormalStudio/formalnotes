//! Tier 2: the appearance audit. Every window in light, dark and matugen themes on each backend
//! kind; fails on a colour that is not a theme token and on text under WCAG contrast. Runs in
//! `nd check` and `nd test`. It audits the demo library, never the user's notes.

mod support;

fn main() {
    nd::audit::Audit::new("formalnotes", formalnotes::app)
        .window("main", |cx| {
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
        .run()
}
