//! The window and everything drawn in it.

pub mod account;
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
mod sidebar;
pub mod strip;
pub mod sync_status;
pub mod table;

pub use notices::Undo;
pub use root::FormalNotes;
