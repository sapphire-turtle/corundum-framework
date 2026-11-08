//! The `fs.*` and `journal.*` methods for a server's JSON-RPC router.

use std::sync::Arc;

use corundum_framework_fs_api::{
    ApplyParams, Check, DEFAULT_HISTORY_LIMIT, DryRun, FS_APPLY, FS_LIST, FS_READ, FS_STAT, FsOp,
    History, HistoryParams, JOURNAL_HISTORY, JOURNAL_REDO, JOURNAL_STATUS, JOURNAL_UNDO,
    JOURNAL_UNDO_BATCH, MAX_APPLY_OPS, MAX_HISTORY_LIMIT, OriginParams, StatusParams,
    UndoBatchParams,
};
use corundum_framework_journal::{
    Actor, BatchId, Journal, JournalConfig, JournalStore, MemoryStore, Origin, Planned, Registry,
};
use grain_id::GrainId;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::ops::{MkdirOp, MkdirPlan, MoveOp, MovePlan, TrashOp, TrashPlan};
use crate::wire::{self, internal};
use crate::{FsCtx, FsError, Workspace, WorkspaceApp, WsPath, list, read, register_builtin, stat};

/// A workspace, its app and their journal, answering the `fs.*` and `journal.*` methods.
///
/// Clones share all three.
pub struct FsService<A: WorkspaceApp> {
    ws: Arc<Workspace>,
    app: Arc<A>,
    journal: Journal<FsCtx<A>>,
}

impl<A: WorkspaceApp> Clone for FsService<A> {
    fn clone(&self) -> Self {
        FsService {
            ws: Arc::clone(&self.ws),
            app: Arc::clone(&self.app),
            journal: self.journal.clone(),
        }
    }
}

type Op<P, R> = fn(&Workspace, &dyn WorkspaceApp, &P) -> Result<R, FsError>;

impl<A: WorkspaceApp> FsService<A> {
    /// The service over `ws`, told about it by `app`, with a journal of the built-in kinds
    /// kept in memory ([`MemoryStore::default`], [`JournalConfig::default`]).
    ///
    /// # Panics
    ///
    /// Panics as [`FsService::with_journal`] does.
    pub fn new(ws: Workspace, app: A) -> FsService<A> {
        FsService::with_journal(
            ws,
            app,
            Registry::new(),
            MemoryStore::default(),
            JournalConfig::default(),
        )
    }

    /// The service over `ws`, told about it by `app`, with a journal of the app's `kinds`
    /// and the built-in ones (`fs.*`), over `store`.
    ///
    /// # Panics
    ///
    /// Panics if one of `kinds` is named as a built-in kind is, as any kind registered twice
    /// does; if the app's [`WorkspaceApp::own_folder`] is not a single dot-name (see
    /// [`FsCtx::new`]); or if the journal's thread cannot be started.
    pub fn with_journal(
        ws: Workspace,
        app: A,
        mut kinds: Registry<FsCtx<A>>,
        store: impl JournalStore + 'static,
        config: JournalConfig,
    ) -> FsService<A> {
        let (ws, app) = (Arc::new(ws), Arc::new(app));
        register_builtin(&mut kinds);
        let ctx = FsCtx::new(Arc::clone(&ws), Arc::clone(&app));
        FsService {
            ws,
            app,
            journal: Journal::new(kinds, ctx, store, config),
        }
    }

    /// The journal, for an app's own batches. Its calls may block: call them off the async
    /// threads.
    pub fn journal(&self) -> &Journal<FsCtx<A>> {
        &self.journal
    }

    /// Answers one request: `params` parsed for `method`, the work done off the async
    /// threads, the result as JSON.
    ///
    /// `actor` is who asks: the changes (`fs.apply`, the undo and redo methods) are made
    /// for it, and the reads ignore it.
    ///
    /// A method that is neither one of the `fs.*` nor one of the `journal.*` answers
    /// [`FsError::MethodNotFound`], so a server can try its own methods next.
    pub async fn dispatch(
        &self,
        method: &str,
        params: Value,
        actor: &Actor,
    ) -> Result<Value, FsError> {
        match method {
            FS_LIST => self.run(params, list).await,
            FS_STAT => self.run(params, stat).await,
            FS_READ => self.run(params, read).await,
            FS_APPLY => self.apply(parse(params)?, actor.clone()).await,
            JOURNAL_UNDO | JOURNAL_REDO => {
                let p: OriginParams = parse(params)?;
                let (origin, actor) = (Origin::new(p.origin), actor.clone());
                let redo = method == JOURNAL_REDO;
                self.compensate(move |j| {
                    if redo {
                        j.redo(&origin, &actor)
                    } else {
                        j.undo(&origin, &actor)
                    }
                })
                .await
            }
            JOURNAL_UNDO_BATCH => {
                let p: UndoBatchParams = parse(params)?;
                let target = batch_id("batch_id", &p.batch_id)?;
                let (origin, actor) = (Origin::new(p.origin), actor.clone());
                self.compensate(move |j| j.undo_batch(target, &origin, &actor))
                    .await
            }
            JOURNAL_HISTORY => self.history(parse(params)?).await,
            JOURNAL_STATUS => {
                let p: StatusParams = parse(params)?;
                let id = batch_id("batch_id", &p.batch_id)?;
                self.blocking(move |j| wire::records(&j.status(id)?)).await
            }
            other => Err(FsError::MethodNotFound(other.to_owned())),
        }
    }

    async fn run<P, R>(&self, params: Value, op: Op<P, R>) -> Result<Value, FsError>
    where
        P: DeserializeOwned + Send + 'static,
        R: Serialize + Send + 'static,
    {
        let params: P = parse(params)?;
        let (ws, app) = (Arc::clone(&self.ws), Arc::clone(&self.app));
        let result = tokio::task::spawn_blocking(move || op(&ws, &*app, &params))
            .await
            .map_err(internal)??;
        serde_json::to_value(result).map_err(internal)
    }

    /// Runs `f` on the journal on a blocking thread, and gives its result as JSON.
    async fn blocking<R, F>(&self, f: F) -> Result<Value, FsError>
    where
        R: Serialize + Send + 'static,
        F: FnOnce(&Journal<FsCtx<A>>) -> Result<R, FsError> + Send + 'static,
    {
        let journal = self.journal.clone();
        let result = tokio::task::spawn_blocking(move || f(&journal))
            .await
            .map_err(internal)??;
        serde_json::to_value(result).map_err(internal)
    }

    /// Starts a compensation with `start`, waits for it to end, and gives its records.
    async fn compensate<F>(&self, start: F) -> Result<Value, FsError>
    where
        F: FnOnce(&Journal<FsCtx<A>>) -> Result<BatchId, corundum_framework_journal::JournalError>
            + Send
            + 'static,
    {
        self.blocking(move |j| {
            let id = start(j)?;
            wire::records(&j.wait(id)?)
        })
        .await
    }

    async fn apply(&self, p: ApplyParams, actor: Actor) -> Result<Value, FsError> {
        if p.ops.is_empty() {
            return Err(FsError::InvalidParams(
                "ops is empty: an empty batch would take an undo".into(),
            ));
        }
        if p.ops.len() > MAX_APPLY_OPS {
            return Err(FsError::TooLarge(format!(
                "{} operations, at most {MAX_APPLY_OPS}",
                p.ops.len()
            )));
        }
        let ops = p
            .ops
            .into_iter()
            .map(planned)
            .collect::<Result<Vec<_>, _>>()?;
        if p.dry_run {
            return self
                .blocking(move |j| {
                    let checks = j
                        .dry_run(&ops)?
                        .into_iter()
                        .zip(ops)
                        .map(|(result, op)| {
                            Ok(Check {
                                kind: op.kind,
                                plan: op.plan,
                                skip: result.err().as_ref().map(wire::skip).transpose()?,
                            })
                        })
                        .collect::<Result<_, FsError>>()?;
                    Ok(DryRun { checks })
                })
                .await;
        }
        let origin = Origin::new(p.origin);
        self.blocking(move |j| {
            let id = j.submit(ops, &origin, &actor, p.label)?;
            wire::records(&j.wait(id)?)
        })
        .await
    }

    async fn history(&self, p: HistoryParams) -> Result<Value, FsError> {
        let limit = p
            .limit
            .unwrap_or(DEFAULT_HISTORY_LIMIT)
            .clamp(1, MAX_HISTORY_LIMIT) as usize;
        let cursor = p
            .cursor
            .as_deref()
            .map(|c| batch_id("cursor", c))
            .transpose()?;
        self.blocking(move |j| {
            let page = j.history(cursor, limit)?;
            let next_cursor = (page.len() == limit)
                .then(|| page.last().map(|b| b.begin.batch_id.to_string()))
                .flatten();
            let batches = page.iter().map(wire::summary).collect::<Result<_, _>>()?;
            Ok(History {
                batches,
                next_cursor,
            })
        })
        .await
    }
}

fn parse<P: DeserializeOwned>(params: Value) -> Result<P, FsError> {
    serde_json::from_value(params).map_err(|e| FsError::InvalidParams(e.to_string()))
}

/// The batch id `s`, given as the parameter `name`.
fn batch_id(name: &str, s: &str) -> Result<BatchId, FsError> {
    s.parse::<GrainId>()
        .map(BatchId)
        .map_err(|e| FsError::InvalidParams(format!("{name} {s:?}: {e}")))
}

/// The built-in operation for `op`, its paths checked.
fn planned(op: FsOp) -> Result<Planned, FsError> {
    Ok(match op {
        FsOp::Move { from, to, expect } => Planned::new::<MoveOp>(&MovePlan {
            from: WsPath::parse(&from)?,
            to: WsPath::parse(&to)?,
            expect,
        }),
        FsOp::Mkdir { path } => Planned::new::<MkdirOp>(&MkdirPlan::Make {
            path: WsPath::parse(&path)?,
        }),
        FsOp::Trash { path, expect } => Planned::new::<TrashOp>(&TrashPlan::Trash {
            path: WsPath::parse(&path)?,
            expect,
        }),
    })
}
