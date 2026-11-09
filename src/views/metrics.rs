//! The layout's geometry in points, from the design: one place for the numbers the views share.

/// Default window size and the smallest it shrinks to.
pub const WINDOW: (f32, f32) = (1280., 800.);
pub const WINDOW_MIN: f32 = 480.;

pub const SIDEBAR_WIDTH: f32 = 280.;
pub const SIDEBAR_MIN: f32 = 240.;
pub const SIDEBAR_MAX: f32 = 800.;

/// Page padding around the card grid.
pub const PAGE_PAD_X: f32 = 24.;
pub const PAGE_PAD_TOP: f32 = 14.;
pub const HEADER_HEIGHT: f32 = 48.;

/// Cards: at least this wide, this tall for their width, this far apart (more when a level
/// holds folders, whose tabs need the room).
pub const CARD_MIN_WIDTH: f32 = 240.;
pub const CARD_ASPECT: f32 = 264. / 236.;
pub const CARD_GAP: f32 = 16.;
pub const CARD_GAP_FOLDERS: f32 = 24.;
pub const CARD_MAX_COLUMNS: usize = 8;
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

/// Editor: the title, its line, and the least margin beside the text column.
pub const TITLE_SIZE: f32 = 34.;
pub const TITLE_LINE: f32 = 55.;
pub const EDITOR_MARGIN: f32 = 66.;
pub const EDITOR_TOP: f32 = 24.;

/// Sidebar search pill.
pub const PILL_HEIGHT: f32 = 30.;
pub const PILL_RADIUS: f32 = 9.;
