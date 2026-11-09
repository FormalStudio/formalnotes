//! The card grid. `tile_grid` places tiles with `.offset`, which AppKit draws but does not
//! hit-test (every tile answers clicks at the grid's origin), so the cards wrap here in one flex
//! box of real frames, sized by nd's `TileMetrics` for the design's column count. All cards
//! share that one parent, so a card that reflows into another row glides there instead of
//! remounting.

use nd::components::TileMetrics;
use nd::prelude::*;

use super::metrics::*;
use super::motion;
use super::root::FormalNotes;

impl FormalNotes {
    /// `cards` as (key, face), as many to a row as fit. Each card's frame is keyed by its key:
    /// the frame a presented note grows from. A card shows at once (the design keeps pages
    /// instant), glides and resizes on the layout spring when the grid reflows or a card comes or
    /// goes before it, and fades and shrinks where it stood when it leaves.
    pub(super) fn grid(&self, cards: Vec<(SharedString, AnyElement)>, gap: f32, cx: &mut Context<Self>) -> Div {
        let this = cx.weak_entity();
        let container = div().w_full().on_resize(move |e, _, cx| {
            let _ = this.update(cx, |this, cx| {
                // The window's first layout reports no width at all: cards placed for it would
                // glide in from a column of nothing once the real width arrives.
                if e.width >= 1. && this.grid_width != Some(e.width) {
                    this.grid_width = Some(e.width);
                    cx.notify();
                }
            });
        });
        let Some(width) = self.grid_width else { return container };
        let columns = card_columns(width, gap);
        let m = TileMetrics::new(width, columns, 1., CARD_ASPECT, gap);
        let cards = cards.into_iter().map(|(key, face)| {
            div()
                .id(key)
                .flex_none()
                .w(px(m.tile.0))
                .h(px(m.tile.1))
                .animate_layout(motion::layout())
                .exit_to(motion::card_exit)
                .child(face)
        });
        container.child(h_flex().flex_wrap().items_start().gap(px(gap)).children(cards))
    }
}
