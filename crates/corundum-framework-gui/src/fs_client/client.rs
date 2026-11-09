//! The typed calls.

use std::sync::{Arc, OnceLock};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use corundum_framework_fs_api::{
    API_VERSION, ApplyParams, BatchRecords, Entry, FS_APPLY, FS_LIST, FS_READ, FS_STAT, FS_VERSION,
    FsOp, History, HistoryParams, JOURNAL_HISTORY, JOURNAL_REDO, JOURNAL_STATUS, JOURNAL_UNDO,
    JOURNAL_UNDO_BATCH, ListParams, Listing, OriginParams, ReadParams, ReadResult, StatParams,
    StatusParams, UndoBatchParams, VersionInfo, codes,
};
use corundum_framework_platform::pending::Pending;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::{FsError, FsTransport};

/// The `fs.*` and `journal.*` calls, typed, through a transport.
///
/// One origin: the batches it applies are undone and redone by its own [`undo`](Self::undo)
/// and [`redo`](Self::redo). Clones share the origin, the transport and the version found.
#[derive(Clone)]
pub struct FsClient {
    transport: Arc<dyn FsTransport>,
    origin: Arc<str>,
    /// The server's version, once `check_version` found it differs from this client's.
    mismatch: Arc<OnceLock<u32>>,
}

impl FsClient {
    /// A client over `transport`, with an origin of its own.
    pub fn new(transport: impl FsTransport) -> FsClient {
        FsClient {
            transport: Arc::new(transport),
            origin: format!("gui:{}", grain_id::GrainId::random()).into(),
            mismatch: Arc::new(OnceLock::new()),
        }
    }

    /// The origin of this client's batches: `gui:` and a random id.
    pub fn origin(&self) -> &str {
        &self.origin
    }

    /// Asks the server's API version (`fs.version`). A server without the method, or with
    /// another version, answers [`FsError::ApiMismatch`], and so does every call after. The
    /// mismatch is latched when this answer is taken in (polled or waited for), not when the
    /// server answers: calls made before that still go out.
    pub fn check_version(&self) -> Pending<Result<(), FsError>> {
        let mismatch = Arc::clone(&self.mismatch);
        self.call::<VersionInfo>(FS_VERSION, json!({}))
            .map(move |answer| {
                let server = match answer {
                    Ok(info) => info.api,
                    Err(FsError::Rpc { code, .. }) if code == codes::METHOD_NOT_FOUND => 0,
                    Err(e) => return Err(e),
                };
                if server == API_VERSION {
                    return Ok(());
                }
                let _ = mismatch.set(server);
                Err(FsError::ApiMismatch {
                    server,
                    client: API_VERSION,
                })
            })
    }

    /// A page of the folder `path`'s entries, after `cursor` (the previous page's
    /// `next_cursor`; `None` for the first).
    pub fn list(&self, path: &str, cursor: Option<String>) -> Pending<Result<Listing, FsError>> {
        self.call(
            FS_LIST,
            ListParams {
                path: path.to_owned(),
                cursor,
                limit: None,
            },
        )
    }

    /// The entry at `path`.
    pub fn stat(&self, path: &str) -> Pending<Result<Entry, FsError>> {
        self.call(
            FS_STAT,
            StatParams {
                path: path.to_owned(),
            },
        )
    }

    /// At most `len` bytes of the file `path` from `offset`.
    ///
    /// The bytes are decoded from base64 where the [`Pending`] is polled or waited for: poll a
    /// large read off the UI thread, or read through an [`FsReader`](super::FsReader) on a
    /// worker.
    pub fn read(&self, path: &str, offset: u64, len: u32) -> Pending<Result<Vec<u8>, FsError>> {
        self.read_with_total(path, offset, len)
            .map(|answer| answer.map(|(bytes, _)| bytes))
    }

    /// As [`read`](Self::read), with the file's whole length; decoded, as there, where the
    /// [`Pending`] is polled or waited for.
    pub fn read_with_total(
        &self,
        path: &str,
        offset: u64,
        len: u32,
    ) -> Pending<Result<(Vec<u8>, u64), FsError>> {
        let params = ReadParams {
            path: path.to_owned(),
            offset,
            len,
        };
        self.call::<ReadResult>(FS_READ, params).map(|answer| {
            let read = answer?;
            let bytes = STANDARD
                .decode(read.data)
                .map_err(|e| FsError::Decode(e.to_string()))?;
            Ok((bytes, read.total))
        })
    }

    /// Applies `ops` as one batch of this client's origin.
    pub fn apply(
        &self,
        ops: Vec<FsOp>,
        label: Option<String>,
    ) -> Pending<Result<BatchRecords, FsError>> {
        self.call(
            FS_APPLY,
            ApplyParams {
                ops,
                origin: self.origin.to_string(),
                label,
                dry_run: false,
            },
        )
    }

    /// Undoes this client's latest batch.
    pub fn undo(&self) -> Pending<Result<BatchRecords, FsError>> {
        self.call(JOURNAL_UNDO, self.origin_params())
    }

    /// Redoes this client's latest undone batch.
    pub fn redo(&self) -> Pending<Result<BatchRecords, FsError>> {
        self.call(JOURNAL_REDO, self.origin_params())
    }

    /// Undoes the batch `batch_id`, of any origin, the undo being of this client's.
    pub fn undo_batch(&self, batch_id: &str) -> Pending<Result<BatchRecords, FsError>> {
        self.call(
            JOURNAL_UNDO_BATCH,
            UndoBatchParams {
                batch_id: batch_id.to_owned(),
                origin: self.origin.to_string(),
            },
        )
    }

    /// A page of the workspace's batches, newest first, after `cursor` (the previous page's
    /// `next_cursor`; `None` for the first).
    pub fn history(&self, cursor: Option<String>) -> Pending<Result<History, FsError>> {
        self.call(
            JOURNAL_HISTORY,
            HistoryParams {
                cursor,
                limit: None,
            },
        )
    }

    /// The batch `batch_id` with its operations.
    pub fn status(&self, batch_id: &str) -> Pending<Result<BatchRecords, FsError>> {
        self.call(
            JOURNAL_STATUS,
            StatusParams {
                batch_id: batch_id.to_owned(),
            },
        )
    }

    fn origin_params(&self) -> OriginParams {
        OriginParams {
            origin: self.origin.to_string(),
        }
    }

    /// Calls `method` with `params`, decoding its result as `R`; at once
    /// [`FsError::ApiMismatch`] after a mismatch was found.
    fn call<R: DeserializeOwned + Send + 'static>(
        &self,
        method: &str,
        params: impl Serialize,
    ) -> Pending<Result<R, FsError>> {
        if let Some(&server) = self.mismatch.get() {
            return Pending::ready(Err(FsError::ApiMismatch {
                server,
                client: API_VERSION,
            }));
        }
        let params = serde_json::to_value(params).expect("the fs.* params serialize");
        self.transport.call(method, params).map(|answer| {
            answer.and_then(|value: Value| {
                serde_json::from_value(value).map_err(|e| FsError::Decode(e.to_string()))
            })
        })
    }
}
