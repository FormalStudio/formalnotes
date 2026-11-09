//! Tier 2: the appearance audit. Every window in light, dark and matugen themes on each backend
//! kind; fails on a colour that is not a theme token and on text under WCAG contrast. Runs in
//! `nd check` and `nd test`. It audits the demo library, never the user's notes.

fn main() {
    nd::audit::Audit::new("formalnotes", formalnotes::app)
        .window("main", |cx| {
            let window = formalnotes::open_demo(cx);
            // No host pumps the disk worker here: wait for the library so the cards are audited.
            let store = window.root().read(cx).store().clone();
            store.update(cx, |store, cx| store.settle(cx));
            window.id()
        })
        .run()
}
