//! The window and everything drawn in it.

mod ai;
pub mod api_key;
pub mod ask;
mod cards;
pub mod editor;
mod format;
mod grid;
pub(crate) mod metrics;
mod motion;
mod note;
mod notices;
mod page;
pub mod palette;
mod root;
mod sidebar;

pub use notices::Undo;
pub use root::FormalNotes;
