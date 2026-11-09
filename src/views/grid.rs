//! The card grid. `tile_grid` places tiles with `.offset`, which AppKit draws but does not
//! hit-test (every tile answers clicks at the grid's origin), so the cards are laid out here in
//! rows of real frames, sized by nd's `TileMetrics` for the measured width.

use nd::components::TileMetrics;
use nd::prelude::*;

use super::metrics::*;
use super::root::FormalNotes;

impl FormalNotes {
    /// `cards` as (key, face) in rows of as many columns as fit. Each card's frame is keyed by
    /// its key: the frame a presented note grows from.
    pub(super) fn grid(&self, cards: Vec<(SharedString, AnyElement)>, gap: f32, cx: &mut Context<Self>) -> Div {
        let this = cx.weak_entity();
        let container = div().w_full().on_resize(move |e, _, cx| {
            let _ = this.update(cx, |this, cx| {
                if this.grid_width != Some(e.width) {
                    this.grid_width = Some(e.width);
                    cx.notify();
                }
            });
        });
        let Some(width) = self.grid_width else { return container };
        let m = TileMetrics::new(width, CARD_MAX_COLUMNS, CARD_MIN_WIDTH, CARD_ASPECT, gap);
        let mut rows = Vec::new();
        let mut cards = cards.into_iter().peekable();
        while cards.peek().is_some() {
            let row = h_flex().gap(px(gap)).children(
                cards
                    .by_ref()
                    .take(m.columns)
                    .map(|(key, face)| div().id(key).flex_none().w(px(m.tile.0)).h(px(m.tile.1)).child(face)),
            );
            rows.push(row);
        }
        container.child(v_flex().gap(px(gap)).children(rows))
    }
}
