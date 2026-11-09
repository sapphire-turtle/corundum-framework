//! The one method a client calls through, and what can go wrong.

use corundum_framework_fs_api::codes;
use corundum_framework_platform::pending::Pending;
use serde_json::Value;

use crate::tr;

/// Why a call failed.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsError {
    /// The server answered with a JSON-RPC error: one of `codes`, or the app's own.
    Rpc {
        /// The error's code.
        code: i32,
        /// The server's message.
        message: String,
    },
    /// The call did not reach the server, or its answer did not come back.
    Transport(String),
    /// The answer came, but not in the shape of the method's result.
    Decode(String),
    /// The server speaks another version of the API (`0` when it has no `fs.version`).
    ApiMismatch {
        /// The server's version.
        server: u32,
        /// This client's, `API_VERSION`.
        client: u32,
    },
}

impl FsError {
    /// The error's JSON-RPC code: a server's own, `API_MISMATCH`, or `INTERNAL_ERROR` for
    /// a failure on this side.
    pub fn code(&self) -> i32 {
        match self {
            FsError::Rpc { code, .. } => *code,
            FsError::ApiMismatch { .. } => codes::API_MISMATCH,
            FsError::Transport(_) | FsError::Decode(_) => codes::INTERNAL_ERROR,
        }
    }

    /// The error for a person, in the context's language: the server's message, this
    /// side's text, or that the versions differ (`corundum-fs-api-mismatch`).
    pub fn text(&self, ctx: &egui::Context) -> String {
        match self {
            FsError::ApiMismatch { server, client } => tr!(
                ctx,
                "corundum-fs-api-mismatch",
                server = *server,
                client = *client
            ),
            other => other.to_string(),
        }
    }
}

/// The server's message, this side's text, or that the versions differ, in English (see
/// [`FsError::text`] for the person's language).
impl std::fmt::Display for FsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FsError::Rpc { message, .. } => f.write_str(message),
            FsError::Transport(text) | FsError::Decode(text) => f.write_str(text),
            FsError::ApiMismatch { server, client } => write!(
                f,
                "This app's file API (version {client}) does not match the server's (version {server})"
            ),
        }
    }
}

impl std::error::Error for FsError {}

/// A call whose sender went away without answering.
impl Default for FsError {
    fn default() -> Self {
        FsError::Transport("the request was lost".into())
    }
}

/// How a client's calls reach the server: one JSON-RPC method at a time.
pub trait FsTransport: Send + Sync + 'static {
    /// Calls `method` with `params`, answering its result, or the server's error as
    /// [`FsError::Rpc`]. Never blocks: the answer comes in the [`Pending`], whose channel
    /// wakes the app's window.
    fn call(&self, method: &str, params: Value) -> Pending<Result<Value, FsError>>;
}
