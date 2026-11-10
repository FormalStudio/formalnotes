//! The window and everything drawn in it.

pub mod account;
mod ai;
pub mod api_key;
pub mod ask;
mod cards;
pub mod editor;
mod fields;
pub(crate) mod format;
mod grid;
pub(crate) mod metrics;
mod motion;
mod note;
mod notices;
mod page;
pub mod palette;
mod root;
pub mod sharing;
mod sidebar;
pub mod strip;
pub mod sync_status;
pub mod table;

pub use notices::Undo;
pub use root::FormalNotes;
