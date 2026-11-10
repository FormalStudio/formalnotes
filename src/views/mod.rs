//! The window and everything drawn in it.

mod cards;
pub mod editor;
mod fields;
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
pub mod strip;
pub mod table;

pub use notices::Undo;
pub use root::FormalNotes;
