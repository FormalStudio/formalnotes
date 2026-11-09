//! The notes library: a folder of Markdown files, its model, the disk worker and the store
//! entity the views read.

pub mod debounce;
pub mod disk;
pub mod model;
mod store;
mod worker;

pub use model::{EntryId, Folder, FolderId, Library, Note, NoteId, Sort, TrashItem, TrashKind};
pub use store::Store;
