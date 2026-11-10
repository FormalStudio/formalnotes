//! The window and everything drawn in it.

pub mod account;
mod cards;
pub mod editor;
pub(crate) mod format;
mod grid;
pub(crate) mod metrics;
mod motion;
mod note;
mod notices;
mod page;
pub mod palette;
mod root;
mod sidebar;
pub mod sync_status;

pub use notices::Undo;
pub use root::FormalNotes;
