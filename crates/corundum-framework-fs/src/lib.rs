//! An app server's workspace folder over the `fs.*` methods of `corundum-framework-fs-api`.
//!
//! Paths are checked ([`WsPath`]), and symlinks are never followed. Every dot-name is
//! hidden, and so are the files the app keeps for itself. Folders are listed a page at a
//! time, and files are read in ranges.
//!
//! See `docs/superpowers/specs/2026-11-06-workspace-fs-journal-design.md`.

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
mod workspace;

pub use access::Access;
pub use app::WorkspaceApp;
pub use corundum_framework_fs_api as api;
pub use ctx::FsCtx;
pub use error::FsError;
pub use list::list;
pub use ops::register_builtin;
pub use path::WsPath;
pub use read::{read, stat};
pub use service::FsService;
pub use workspace::{Resolved, Workspace};
