//! What can go wrong with a request.

use corundum_framework_fs_api::codes;

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
        }
    }
}
