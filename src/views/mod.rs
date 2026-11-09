//! The window and everything drawn in it.

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
