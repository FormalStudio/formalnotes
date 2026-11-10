//! The card grid: nd's `tile_grid` at the design's column count for the content region's width.
//! Each tile is keyed by its card's key.

use nd::components::{tile, tile_grid};
use nd::prelude::*;

use super::metrics::*;
use super::motion;
use super::root::FormalNotes;
use crate::nav::ENTRY_DRAG;

impl FormalNotes {
    /// `cards` as (key, face) in the grid `id`, one per page so a page shows its cards at once.
    /// Cards glide on the layout spring when the grid reflows or a card comes or goes before
    /// them, and a card that leaves fades and shrinks where it stood. `draggable` cards drag
    /// as entries into folders, keeping the page's sort order.
    pub(super) fn grid(
        &self,
        id: impl Into<SharedString>,
        cards: Vec<(SharedString, AnyElement)>,
        gap: f32,
        draggable: bool,
    ) -> AnyElement {
        let columns = self.region.map_or(4, |(width, _)| card_columns(width - 2. * PAGE_PAD_X, gap));
        let grid = tile_grid(id);
        let grid = if draggable { grid.draggable_tiles(ENTRY_DRAG).fixed_order() } else { grid };
        grid.max_columns(columns)
            .min_tile_width(1.)
            .aspect(CARD_ASPECT)
            .gap(gap)
            .reflow(motion::layout())
            .tiles(cards.into_iter().map(|(key, face)| tile(key, face)))
            .into_any_element()
    }
}
