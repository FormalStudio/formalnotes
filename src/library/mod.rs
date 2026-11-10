//! The notes library: a folder of Markdown files, its model, the disk worker and the store
//! entity the views read.

pub mod debounce;
pub mod disk;
pub mod model;
pub mod properties;
mod store;
pub mod view;
mod worker;

pub use model::{EntryId, Folder, FolderId, Library, Note, NoteId, Sort, TrashItem, TrashKind};
pub use properties::{PropType, Raw, Value};
pub use store::Store;
pub use view::{Column, FolderView, SortBy};
