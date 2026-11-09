//! The one method a client calls through, and what can go wrong.

use corundum_framework_fs_api::codes;
use corundum_framework_platform::pending::Pending;
use serde_json::Value;

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
}

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
