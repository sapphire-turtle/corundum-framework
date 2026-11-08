//! What can go wrong with a request.

use corundum_framework_fs_api::codes;
use corundum_framework_journal::JournalError;

/// An error answering one of the `fs.*` methods. [`FsError::code`] is its JSON-RPC code.
#[derive(Debug, thiserror::Error)]
pub enum FsError {
    /// The path is malformed, leaves the workspace, passes a symlink, or names a hidden entry.
    #[error("invalid path: {0}")]
    InvalidPath(String),
    /// The path does not exist.
    #[error("not found: {0}")]
    NotFound(String),
    /// The request asks for more than a limit allows.
    #[error("too large: {0}")]
    TooLarge(String),
    /// The parameters do not parse.
    #[error("invalid parameters: {0}")]
    InvalidParams(String),
    /// Not one of the `fs.*` methods: the server's own methods may answer it.
    #[error("no method {0:?}")]
    MethodNotFound(String),
    /// The file system failed.
    #[error("{path}: {source}")]
    Io {
        /// The workspace-relative path.
        path: String,
        /// What the OS said.
        source: std::io::Error,
    },
    /// The journal refused or failed the call: nothing to undo, an expired batch, a busy
    /// worker and so on.
    #[error(transparent)]
    Journal(#[from] JournalError),
}

impl FsError {
    /// The JSON-RPC error code to answer with (see [`codes`]).
    pub fn code(&self) -> i32 {
        match self {
            FsError::InvalidPath(_) => codes::INVALID_PATH,
            FsError::NotFound(_) => codes::NOT_FOUND,
            FsError::TooLarge(_) => codes::TOO_LARGE,
            FsError::InvalidParams(_) => codes::INVALID_PARAMS,
            FsError::MethodNotFound(_) => codes::METHOD_NOT_FOUND,
            FsError::Io { .. } => codes::INTERNAL_ERROR,
            FsError::Journal(e) => match e {
                JournalError::NothingToUndo => codes::NOTHING_TO_UNDO,
                JournalError::NothingToRedo => codes::NOTHING_TO_REDO,
                JournalError::AlreadyUndone(_) => codes::ALREADY_UNDONE,
                JournalError::Expired(_) => codes::EXPIRED,
                JournalError::Forbidden(_) => codes::FORBIDDEN,
                JournalError::Busy => codes::BUSY,
                JournalError::NotFound(_) => codes::NOT_FOUND,
                // UnknownKind, InvalidPlan, Stopped, Store, and any later variant.
                _ => codes::INTERNAL_ERROR,
            },
        }
    }
}
