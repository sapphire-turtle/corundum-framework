//! The `fs.*` methods for a server's JSON-RPC router.

use std::sync::Arc;

use corundum_framework_fs_api::{FS_LIST, FS_READ, FS_STAT};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::{FsError, Workspace, WorkspaceApp, list, read, stat};

/// A workspace and its app, answering the `fs.*` methods.
///
/// Clones share both.
pub struct FsService<A: WorkspaceApp> {
    ws: Arc<Workspace>,
    app: Arc<A>,
}

impl<A: WorkspaceApp> Clone for FsService<A> {
    fn clone(&self) -> Self {
        FsService {
            ws: Arc::clone(&self.ws),
            app: Arc::clone(&self.app),
        }
    }
}

type Op<P, R> = fn(&Workspace, &dyn WorkspaceApp, &P) -> Result<R, FsError>;

impl<A: WorkspaceApp> FsService<A> {
    /// The service over `ws`, told about it by `app`.
    pub fn new(ws: Workspace, app: A) -> FsService<A> {
        FsService {
            ws: Arc::new(ws),
            app: Arc::new(app),
        }
    }

    /// Answers one request: `params` parsed for `method`, the work done off the async
    /// threads, the result as JSON.
    ///
    /// A method that is not one of the `fs.*` answers [`FsError::MethodNotFound`], so a
    /// server can try its own methods next.
    pub async fn dispatch(&self, method: &str, params: Value) -> Result<Value, FsError> {
        match method {
            FS_LIST => self.run(params, list).await,
            FS_STAT => self.run(params, stat).await,
            FS_READ => self.run(params, read).await,
            other => Err(FsError::MethodNotFound(other.to_owned())),
        }
    }

    async fn run<P, R>(&self, params: Value, op: Op<P, R>) -> Result<Value, FsError>
    where
        P: DeserializeOwned + Send + 'static,
        R: Serialize + Send + 'static,
    {
        let params: P =
            serde_json::from_value(params).map_err(|e| FsError::InvalidParams(e.to_string()))?;
        let (ws, app) = (Arc::clone(&self.ws), Arc::clone(&self.app));
        let result = tokio::task::spawn_blocking(move || op(&ws, &*app, &params))
            .await
            .map_err(|e| FsError::Io {
                path: String::new(),
                source: std::io::Error::other(e),
            })??;
        serde_json::to_value(result).map_err(|e| FsError::Io {
            path: String::new(),
            source: std::io::Error::other(e),
        })
    }
}
