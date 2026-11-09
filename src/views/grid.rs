//! The card grid: nd's `tile_grid` at the design's column count for the content region's width.
//! Each tile is keyed by its card's key, the frame a presented note grows from.

use nd::components::{tile, tile_grid};
use nd::prelude::*;

use super::metrics::*;
use super::motion;
use super::root::FormalNotes;

impl FormalNotes {
    /// `cards` as (key, face) in the grid `id`, one per page so a page shows its cards at once.
    /// Cards glide on the layout spring when the grid reflows or a card comes or goes before
    /// them, and a card that leaves fades and shrinks where it stood.
    pub(super) fn grid(
        &self,
        id: impl Into<SharedString>,
        cards: Vec<(SharedString, AnyElement)>,
        gap: f32,
    ) -> AnyElement {
        let columns = self.region.map_or(4, |(width, _)| card_columns(width - 2. * PAGE_PAD_X, gap));
        tile_grid(id)
            .max_columns(columns)
            .min_tile_width(1.)
            .aspect(CARD_ASPECT)
            .gap(gap)
            .reflow(motion::layout())
            .tiles(cards.into_iter().map(|(key, face)| tile(key, face)))
            .into_any_element()
    }
}
