//! The read methods: [`FS_LIST`], [`FS_STAT`] and [`FS_READ`].

use serde::{Deserialize, Serialize};

/// List a folder, a page at a time: [`ListParams`] → [`Listing`].
pub const FS_LIST: &str = "fs.list";
/// One entry: [`StatParams`] → [`Entry`].
pub const FS_STAT: &str = "fs.stat";
/// A range of a file's bytes: [`ReadParams`] → [`ReadResult`].
pub const FS_READ: &str = "fs.read";

/// The most bytes one [`FS_READ`] returns (1 MiB).
pub const MAX_READ_LEN: u32 = 1_048_576;
/// The entries of a page when [`ListParams::limit`] is not given.
pub const DEFAULT_LIST_LIMIT: u32 = 200;
/// The most entries of a page.
pub const MAX_LIST_LIMIT: u32 = 1000;

/// What an entry is. Symlinks are listed but never followed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    /// A file (or anything that is neither a folder nor a symlink).
    File,
    /// A folder.
    Dir,
    /// A symbolic link.
    Symlink,
}

/// What the app says of an entry it manages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Managed {
    /// The app's kind of item: `"photo"`, `"track"`.
    pub kind: String,
    /// The app's id of the item.
    pub item_id: String,
}

/// One entry of a folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// Its name in its folder; `""` for the root.
    pub name: String,
    /// What it is.
    pub kind: EntryKind,
    /// A file's length in bytes; 0 for a folder or a symlink.
    pub size: u64,
    /// When it last changed, in milliseconds since the Unix epoch, if the system says.
    pub modified: Option<i64>,
    /// Set when the app manages it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub managed: Option<Managed>,
}

/// Parameters of [`FS_LIST`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListParams {
    /// The folder.
    pub path: String,
    /// The previous page's [`Listing::next_cursor`]; `None` for the first page.
    #[serde(default)]
    pub cursor: Option<String>,
    /// Entries per page: [`DEFAULT_LIST_LIMIT`] when `None`, at most [`MAX_LIST_LIMIT`].
    #[serde(default)]
    pub limit: Option<u32>,
}

/// Result of [`FS_LIST`]: entries in name order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Listing {
    /// This page's entries.
    pub entries: Vec<Entry>,
    /// The cursor of the next page; `None` when this one is the last.
    pub next_cursor: Option<String>,
}

/// Parameters of [`FS_STAT`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatParams {
    /// The entry.
    pub path: String,
}

/// Parameters of [`FS_READ`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadParams {
    /// The file.
    pub path: String,
    /// The first byte.
    pub offset: u64,
    /// How many bytes, at most [`MAX_READ_LEN`].
    pub len: u32,
}

/// Result of [`FS_READ`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadResult {
    /// The bytes, in standard base64 with padding; empty past the end of the file.
    pub data: String,
    /// The file's whole length.
    pub total: u64,
}
