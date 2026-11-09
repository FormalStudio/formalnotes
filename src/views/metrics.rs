//! The layout's geometry in points, from the design: one place for the numbers the views share.

/// Default window size and the smallest it shrinks to.
pub const WINDOW: (f32, f32) = (1280., 800.);
pub const WINDOW_MIN: f32 = 480.;

pub const SIDEBAR_WIDTH: f32 = 280.;
pub const SIDEBAR_MIN: f32 = 240.;
pub const SIDEBAR_MAX: f32 = 800.;
/// The page keeps this much room beside the sidebar; the window stops narrowing there.
pub const CONTENT_MIN: f32 = 420.;

/// Page padding around the card grid.
pub const PAGE_PAD_X: f32 = 24.;
pub const PAGE_PAD_TOP: f32 = 14.;
pub const HEADER_HEIGHT: f32 = 48.;

/// Cards: this tall for their width, this far apart (more when a level holds folders, whose
/// tabs need the room).
pub const CARD_ASPECT: f32 = 264. / 236.;
pub const CARD_GAP: f32 = 16.;
pub const CARD_GAP_FOLDERS: f32 = 24.;
pub const CARD_FRAME: f32 = 4.;
pub const CARD_RADIUS: f32 = 20.;
pub const PAGE_RADIUS: f32 = 16.;
pub const CARD_PADDING: f32 = 16.;
/// The date line under a card.
pub const CARD_FOOTER: f32 = 22.;
/// Line height of a note card's title, which shows at most two lines.
pub const TITLE_PREVIEW_LINE: f32 = 17.;
/// Lines of a note a card's miniature shows.
pub const PREVIEW_LINES: usize = 14;
/// Miniature text: the editor's body scaled down to a readable hint of the page.
pub const PREVIEW_TEXT: f32 = 7.;
pub const PREVIEW_LINE: f32 = 10.;
pub const PREVIEW_HEADING: f32 = 8.;

/// Folder cards: the plate's radius, the tab's share of the width and its rise.
pub const FOLDER_RADIUS: f32 = 18.;
pub const FOLDER_TAB_WIDTH: f32 = 0.4;
pub const FOLDER_TAB_RISE: f32 = 0.06;
pub const PAPER_RADIUS: f32 = 10.;
/// Where the frosted front starts, as a share of the card's height.
pub const FOLDER_FRONT_TOP: f32 = 0.5;

/// Editor: the title, its line, and the least margin beside the text column; the room above
/// the title and between it and the body.
pub const TITLE_SIZE: f32 = 34.;
pub const TITLE_LINE: f32 = 55.;
pub const EDITOR_MARGIN: f32 = 66.;
pub const EDITOR_TOP: f32 = 48.;
pub const TITLE_GAP: f32 = 22.;

/// How many cards a row of `avail` points holds with `gap` between them: four or more once
/// four 200-point cards fit (each then at least 240), otherwise one to three of at least 160.
pub fn card_columns(avail: f32, gap: f32) -> usize {
    let fit = |w: f32| ((avail + gap) / (w + gap)).floor().max(0.) as usize;
    if fit(200.) >= 4 { fit(240.).max(4) } else { fit(160.).clamp(1, 3) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_follow_the_design_rule() {
        // Home at 1280 x 800 with a 280 sidebar: 952 points, folders on the level.
        assert_eq!(card_columns(952., CARD_GAP_FOLDERS), 4);
        assert_eq!(card_columns(952., CARD_GAP), 4);
        // Wide: four or more, each at least 240.
        assert_eq!(card_columns(1800., CARD_GAP), 7);
        // Narrow: one to three, each at least 160.
        assert_eq!(card_columns(532., CARD_GAP_FOLDERS), 3);
        assert_eq!(card_columns(400., CARD_GAP), 2);
        assert_eq!(card_columns(100., CARD_GAP), 1);
    }
}
