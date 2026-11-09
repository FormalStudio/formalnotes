//! The note presented over the page: a sheet of paper grows out of the note's card to fill the
//! content region (or rises in when the card is not on screen), then the path and the editor
//! fade in on it. Opening another note while one is open crossfades the text.

use nd::prelude::*;

use super::metrics::*;
use super::motion;
use super::root::FormalNotes;
use crate::actions::GoBack;
use crate::library::FolderId;
use crate::nav::{Location, folder_key};

impl FormalNotes {
    pub(super) fn render_note(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        // The paper's size is given in points so it grows with the travel; before the region
        // has been measured it just fills it.
        let paper = div()
            .id("note-paper")
            .absolute()
            .top_0()
            .left_0()
            .bg(theme.surface.raised)
            .border_1()
            .border_color(theme.border.subtle)
            .rounded(px(PAGE_RADIUS))
            .map(|d| match self.region {
                Some((w, h)) => d.w(px(w)).h(px(h)),
                None => d.size_full(),
            })
            .hero_with(&self.presented, self.hero.clone());
        let switching = self.switching.is_some();
        let content = v_flex()
            .id("note-content")
            .size_full()
            .opacity(if switching { 0. } else { 1. })
            .animate(motion::switch())
            .on_animation_end(cx.listener(|this, _: &AnimationEnded, window, cx| this.finish_switch(window, cx)))
            .child(self.note_header(cx))
            .child(div().flex_1().min_h_0().child(self.editor.clone()));
        let page = div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .bg(theme.surface.raised)
            .reveal_with(&self.presented)
            .child(content);
        div().id("note").size_full().relative().child(paper).child(page).into_any_element()
    }

    /// Back, then the note's path: Home, its folders, its title.
    fn note_header(&self, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme().clone();
        let folder = self.shown.as_ref().map(|n| n.folder()).unwrap_or_default();
        let title = self.shown.as_ref().and_then(|id| self.store.read(cx).note(id)).map(|n| n.title.clone());
        let mut trail = vec![FolderId::root()];
        trail.extend(folder.ancestry());
        let mut crumbs = h_flex().gap(px(6.)).items_center().min_w_0();
        for f in trail {
            let name = if f.is_root() { "Home".to_owned() } else { f.name().to_owned() };
            crumbs = crumbs
                .child(
                    div()
                        .id(format!("note-crumb:{}", folder_key(&f)))
                        .on_tap(cx.listener(move |this, _: &TapEvent, _, cx| this.go(Location::Folder(f.clone()), cx)))
                        .child(text(name).text_color(theme.text.secondary)),
                )
                .child(icon(Icon::ChevronRight, 12.).text_color(theme.text.tertiary));
        }
        crumbs = crumbs.child(text(title.unwrap_or_default()).text_color(theme.text.primary).truncate());
        h_flex()
            .flex_none()
            .h(px(HEADER_HEIGHT))
            .px(px(PAGE_PAD_X - 6.))
            .gap(theme.spacing.sm)
            .items_center()
            .child(icon_button("note-back", Icon::ChevronLeft).tooltip("Back").dispatch(GoBack))
            .child(crumbs)
    }
}
