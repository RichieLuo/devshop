//! Local history and atomic document saves. UI code never supplies filenames for history blobs.
mod files;
mod history;

pub use files::{FileStamp, read_document, save_document};
pub use history::{ContentKind, HistoryEntry, HistoryStore, default_data_directory};
