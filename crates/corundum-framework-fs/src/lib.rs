//! An app server's workspace folder over the `fs.*` and `journal.*` methods of
//! `corundum-framework-fs-api`.
//!
//! Paths are checked ([`WsPath`]), and symlinks are never followed. Every dot-name is
//! hidden, and so are the files the app keeps for itself. Folders are listed a page at a
//! time, and files are read in ranges. `fs.apply` runs the built-in [`ops`] as one batch of
//! a [`journal`], which the `journal.*` methods undo and redo (see [`FsService`]).
//!
//! See `docs/superpowers/specs/2026-11-06-workspace-fs-journal-design.md`.
//!
//! # Known limitations
//!
//! On Linux, hiding is judged on the name as the request spells it, and on the name
//! `canonicalize` returns, which on Linux is the spelling asked for. So on a mount that
//! ignores case (ext4 or f2fs with casefold, vfat, exFAT, CIFS/SMB), a name spelled
//! differently from a hidden entry can still reach it, FAT 8.3 short names such as
//! `CORUND~1` for `.corundum` among them. Clients should send only names they listed. On
//! macOS and Windows, hiding is judged on the disk's spelling.
//!
//! For the same reason, on Linux the paths [`WorkspaceApp::route`] and
//! [`WorkspaceApp::follow`] see stay as the request spells them; on macOS and Windows they
//! are the disk's spelling of the entries that exist.

#![warn(missing_docs)]

mod access;
mod app;
mod ctx;
mod error;
mod expect;
mod list;
pub mod ops;
mod path;
mod read;
mod rename;
mod service;
mod wire;
mod workspace;

pub use access::Access;
pub use app::{Route, WorkspaceApp};
pub use corundum_framework_fs_api as api;
pub use corundum_framework_journal as journal;
pub use ctx::FsCtx;
pub use error::FsError;
pub use list::list;
pub use ops::{DenyOp, DenyPlan, register_builtin};
pub use path::WsPath;
pub use read::{read, stat};
pub use service::FsService;
pub use workspace::{Resolved, Workspace};
