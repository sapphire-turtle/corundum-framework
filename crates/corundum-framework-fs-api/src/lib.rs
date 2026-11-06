//! The `fs.*` methods an app's server serves over its workspace folder: their names, their
//! parameters, their results and their error codes.
//!
//! Plain serde data, kept apart from the engine (`corundum-framework-fs`) so that a client
//! links nothing heavier, and so that the API moves only with [`API_VERSION`]. The transport
//! is the app's: asteria serves these methods over HTTP, sapphire over its local socket.
//!
//! Paths are workspace-relative, with `/` between segments; the root is `""`.
//!
//! See `docs/superpowers/specs/2026-11-06-workspace-fs-journal-design.md`.

#![warn(missing_docs)]

use serde::{Deserialize, Serialize};

/// The version of this API.
///
/// Bumped by hand on any breaking change to a method, a parameter or a result here. The
/// crate is versioned with the rest of corundum, so its version cannot stand for the API's.
pub const API_VERSION: u32 = 1;

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

/// The JSON-RPC error codes of these methods.
///
/// corundum's own are in `-32199..=-32100`, apart from the apps' (asteria's are
/// `-32001..=-32009`).
pub mod codes {
    /// The path is malformed, leaves the workspace, passes a symlink, or names a hidden entry.
    pub const INVALID_PATH: i32 = -32101;
    /// The path does not exist.
    pub const NOT_FOUND: i32 = -32102;
    /// The request asks for more than a limit allows.
    pub const TOO_LARGE: i32 = -32103;
    /// JSON-RPC's: no such method.
    pub const METHOD_NOT_FOUND: i32 = -32601;
    /// JSON-RPC's: the parameters do not parse.
    pub const INVALID_PARAMS: i32 = -32602;
    /// JSON-RPC's: the server failed (an I/O error, say).
    pub const INTERNAL_ERROR: i32 = -32603;
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use serde::de::DeserializeOwned;

    fn round_trip<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(v: &T) {
        let json = serde_json::to_string(v).unwrap();
        assert_eq!(&serde_json::from_str::<T>(&json).unwrap(), v, "{json}");
    }

    fn entry(managed: Option<Managed>) -> Entry {
        Entry {
            name: "p.jpg".into(),
            kind: EntryKind::File,
            size: 42,
            modified: Some(1_700_000_000_000),
            managed,
        }
    }

    #[test]
    fn every_type_round_trips() {
        round_trip(&entry(None));
        round_trip(&entry(Some(Managed {
            kind: "photo".into(),
            item_id: "abc".into(),
        })));
        round_trip(&Listing {
            entries: vec![entry(None)],
            next_cursor: Some("p.jpg".into()),
        });
        round_trip(&ListParams {
            path: "a".into(),
            cursor: Some("b".into()),
            limit: Some(10),
        });
        round_trip(&StatParams { path: "a".into() });
        round_trip(&ReadParams {
            path: "a".into(),
            offset: 3,
            len: 5,
        });
        round_trip(&ReadResult {
            data: "aGVsbG8=".into(),
            total: 5,
        });
    }

    #[test]
    fn entry_kinds_are_lowercase() {
        assert_eq!(serde_json::to_string(&EntryKind::File).unwrap(), "\"file\"");
        assert_eq!(serde_json::to_string(&EntryKind::Dir).unwrap(), "\"dir\"");
        assert_eq!(
            serde_json::to_string(&EntryKind::Symlink).unwrap(),
            "\"symlink\""
        );
    }

    #[test]
    fn missing_optional_params_default() {
        let p: ListParams = serde_json::from_str(r#"{"path":"a"}"#).unwrap();
        assert_eq!(
            p,
            ListParams {
                path: "a".into(),
                cursor: None,
                limit: None
            }
        );
        let json = serde_json::to_value(entry(None)).unwrap();
        assert!(json.get("managed").is_none(), "{json}");
    }

    #[test]
    fn method_names_are_namespaced() {
        for name in [FS_LIST, FS_STAT, FS_READ] {
            assert!(name.starts_with("fs."), "{name}");
        }
    }

    #[test]
    fn codes_stay_in_their_range() {
        for code in [codes::INVALID_PATH, codes::NOT_FOUND, codes::TOO_LARGE] {
            assert!((-32199..=-32100).contains(&code), "{code}");
        }
    }
}
