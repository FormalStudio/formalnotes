//! The faces of the cards on Home, folder and trash pages: a note as a miniature page in a
//! frame, a folder as stacked papers behind a front panel. Look only; the page wires taps,
//! drags and menus.

use jiff::Timestamp;
use nd::prelude::*;

use super::format;
use super::metrics::*;
use super::motion;
use crate::library::model::{PreviewLine, preview};

/// The hover group every card face belongs to.
pub const GROUP: &str = "card";

/// What a note card shows.
pub struct NoteFace<'a> {
    pub title: &'a str,
    pub text: &'a str,
    /// The line under the card: a relative date, or where a trashed note came from.
    pub caption: String,
    pub hovered: bool,
    /// Shorter by the folder tabs' rise, so its top lines up with the folder bodies beside it.
    pub beside_folders: bool,
}

pub fn note_card(theme: &Theme, face: NoteFace<'_>) -> Div {
    let lines = preview(face.text, PREVIEW_LINES);
    let empty = lines.is_empty();
    let page = v_flex()
        .size_full()
        .rounded(px(PAGE_RADIUS))
        .bg(theme.surface.raised)
        .border_1()
        .border_color(theme.border.subtle)
        .overflow_hidden()
        .p(px(CARD_PADDING))
        .gap(px(2.))
        .child(
            div().flex_none().mb(px(6.)).child(
                text(face.title.to_owned())
                    .text_size(theme.type_scale.body)
                    .line_height(px(TITLE_PREVIEW_LINE))
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme.text.primary)
                    .line_clamp(2)
                    .text_ellipsis(),
            ),
        )
        .children(lines.into_iter().map(|line| preview_line(theme, line)))
        .when(empty, |page| {
            page.child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(text("Blank note").text_size(theme.type_scale.caption).text_color(theme.text.secondary)),
            )
        });
    let lift = if face.hovered { -motion::LIFT } else { 0. };
    v_flex()
        .size_full()
        .when(face.beside_folders, |c| c.child(div().flex_none().h(relative(FOLDER_TAB_RISE))))
        .child(
            div()
                .flex_1()
                .min_h_0()
                .p(px(CARD_FRAME))
                .rounded(px(CARD_RADIUS))
                .bg(theme.border.subtle)
                .group_hover(GROUP, |s| s.bg(theme.border.strong))
                .shadow(crate::theme::card_shadow(theme, face.hovered))
                .offset(px(0.), px(lift))
                .animate(motion::hover(face.hovered))
                .child(page),
        )
        .child(caption(theme, face.caption))
}

fn caption(theme: &Theme, label: String) -> Div {
    h_flex()
        .flex_none()
        .h(px(CARD_FOOTER))
        .px(px(6.))
        .items_end()
        .child(text(label).text_size(theme.type_scale.caption).text_color(theme.text.secondary).truncate())
}

/// One line of a miniature page, cut with an ellipsis. Its box sets the line pitch, which AppKit
/// labels do not take from `line_height`.
fn preview_line(theme: &Theme, line: PreviewLine) -> AnyElement {
    let pitch = match line {
        PreviewLine::Heading(_) => PREVIEW_LINE + 2.,
        _ => PREVIEW_LINE,
    };
    div().flex_none().h(px(pitch)).child(preview_text(theme, line)).into_any_element()
}

fn preview_text(theme: &Theme, line: PreviewLine) -> AnyElement {
    let small = |s: String| text(s).text_size(px(PREVIEW_TEXT)).line_height(px(PREVIEW_LINE)).truncate();
    match line {
        PreviewLine::Heading(s) => text(s)
            .text_size(px(PREVIEW_HEADING))
            .line_height(px(PREVIEW_LINE + 2.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme.text.primary)
            .truncate()
            .into_any_element(),
        PreviewLine::Bullet(s) => small(format!("•  {s}")).text_color(theme.text.primary).into_any_element(),
        PreviewLine::Task { done, text: s } => {
            let mark = if done { "☑" } else { "☐" };
            let color = if done { theme.text.secondary } else { theme.text.primary };
            small(format!("{mark}  {s}")).text_color(color).into_any_element()
        }
        PreviewLine::Quote(s) => h_flex()
            .gap(px(4.))
            .child(div().w(px(1.5)).h(px(PREVIEW_LINE)).bg(theme.text.tertiary))
            .child(small(s).text_color(theme.text.secondary))
            .into_any_element(),
        PreviewLine::Code(s) => small(s).text_color(theme.text.secondary).into_any_element(),
        PreviewLine::Text(s) => small(s).text_color(theme.text.primary).into_any_element(),
    }
}

/// What a folder card shows.
pub struct FolderFace<'a> {
    pub name: &'a str,
    pub items: usize,
    /// The first note's text, drawn on the front paper.
    pub first_page: Option<&'a str>,
    pub hovered: bool,
    /// A dragged entry is over it: it opens and rings in the accent.
    pub targeted: bool,
}

/// The resting pose of each paper, front first: its left edge and top, as shares of the card,
/// and how far it rises on hover. The design tilts them; nd has no rotation, so they fan out by
/// offset alone.
const PAPERS: [(f32, f32, f32); 3] = [(0.10, 0.17, 10.), (0.15, 0.135, 7.), (0.05, 0.115, 4.)];

pub fn folder_card(theme: &Theme, face: FolderFace<'_>) -> Div {
    let papers = face.items.clamp(1, 3);
    let open = face.hovered || face.targeted;
    let paper = |i: usize| {
        let (left, top, rise) = PAPERS[i];
        let y = if open { -rise } else { 0. };
        let content = match (i, face.first_page) {
            (0, Some(page)) => v_flex()
                .gap(px(1.))
                .children(preview(page, 6).into_iter().map(|line| preview_line(theme, line)))
                .into_any_element(),
            _ => v_flex()
                .gap(px(5.))
                .pt(px(4.))
                .children(
                    [0.9, 0.75, 0.85, 0.6].map(|w| div().h(px(3.)).w(relative(w)).rounded_full().bg(theme.separator)),
                )
                .into_any_element(),
        };
        div()
            .absolute()
            .left(relative(left))
            .top(relative(top))
            .w(relative(0.8))
            .h(relative(0.64))
            .rounded(px(PAPER_RADIUS))
            .bg(theme.surface.raised)
            .border_1()
            .border_color(theme.border.subtle)
            .overflow_hidden()
            .p(px(12.))
            .child(content)
            .offset(px(0.), px(y))
            .animate(motion::paper(i))
    };
    let front_y = if open { 2. } else { 0. };
    div()
        .size_full()
        .relative()
        // The back plate and its tab, edge to edge: the plate's top-left corner is square
        // under the tab.
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .w(relative(FOLDER_TAB_WIDTH))
                .h(relative(FOLDER_TAB_RISE))
                .rounded_t(px(PAPER_RADIUS))
                .bg(theme.surface.raised_hover),
        )
        .child(
            div()
                .absolute()
                .top(relative(FOLDER_TAB_RISE))
                .left_0()
                .right_0()
                .bottom_0()
                .rounded(px(FOLDER_RADIUS))
                .rounded_tl(px(0.))
                .bg(theme.surface.raised_hover),
        )
        .children((0..papers).rev().map(paper))
        // The front panel, over the papers' lower half.
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .top(relative(FOLDER_FRONT_TOP))
                .rounded(px(FOLDER_RADIUS))
                .bg(theme.surface.raised)
                .border_1()
                .border_color(theme.border.subtle)
                .opacity(0.94)
                .offset(px(0.), px(front_y))
                .animate(motion::folder_open()),
        )
        .child(
            v_flex()
                .absolute()
                .left(px(16.))
                .right(px(14.))
                .bottom(px(14.))
                .child(
                    text(face.name.to_owned())
                        .text_size(theme.type_scale.headline)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text.primary)
                        .truncate(),
                )
                .child(
                    text(format::items(face.items))
                        .text_size(theme.type_scale.caption)
                        .text_color(theme.text.secondary),
                ),
        )
        .child(
            div()
                .absolute()
                .top(px(-DROP_RING_OUTSET))
                .left(px(-DROP_RING_OUTSET))
                .right(px(-DROP_RING_OUTSET))
                .bottom(px(-DROP_RING_OUTSET))
                .rounded(px(FOLDER_RADIUS + DROP_RING_OUTSET))
                .border_2()
                .border_color(theme.accent)
                .opacity(if face.targeted { 1. } else { 0. })
                .animate(motion::hover(face.targeted)),
        )
}

/// A trashed note or folder: its card, captioned with where it came from.
pub fn trash_caption(origin: &str, trashed: Timestamp, now: Timestamp) -> String {
    let from = origin.rsplit_once('/').map_or("Home", |(folder, _)| folder.rsplit('/').next().unwrap_or(folder));
    format!("From {from} · {}", format::relative(trashed, now))
}
