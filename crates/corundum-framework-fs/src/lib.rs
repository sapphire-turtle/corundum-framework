//! An app server's workspace folder over the `fs.*` and `journal.*` methods of
//! `corundum-framework-fs-api`.
//!
//! Paths are checked ([`WsPath`]), and symlinks are never followed. Every dot-name is
//! hidden, and so are the files the app keeps for itself. Folders are listed a page at a
//! time, and files are read in ranges. `fs.apply` runs the built-in [`ops`] as one batch of
//! a [`journal`], which the `journal.*` methods undo and redo (see [`FsService`]).
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
