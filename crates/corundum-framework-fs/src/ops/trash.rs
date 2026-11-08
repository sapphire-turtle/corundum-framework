//! `fs.trash`, and its undo.

use corundum_framework_journal::{Applied, OpKind, Operation, Skip};
use grain_id::GrainId;
use serde::{Deserialize, Serialize};

use crate::api::{Expect, FsOp};
use crate::expect;
use crate::ops::FsDone;
use crate::{Access, FsCtx, Workspace, WorkspaceApp, WsPath};

/// Moves an entry into the trash, `<own folder>/trash/<trash id>/<path>`; its undo moves it
/// back, never replacing anything.
///
/// The own folder is [`WorkspaceApp::own_folder`]. Nothing empties the trash yet.
#[derive(Debug, Clone, Copy, Default)]
pub struct TrashOp;

/// The plan of an `fs.trash`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "do", rename_all = "snake_case")]
pub enum TrashPlan {
    /// Move the entry at `path` into the trash, under a fresh trash id. Skipped with
    /// [`Skip::InvalidPath`] for the root or a hidden path.
    Trash {
        /// The entry.
        path: WsPath,
        /// How the entry must be now; `None` checks nothing.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expect: Option<Expect>,
    },
    /// Move the entry trashed from `path` under `trash_id` back to `path`; skipped with
    /// [`Skip::Exists`] if anything is there, and [`Skip::NotFound`] if `path`'s folder or
    /// the trashed entry is gone.
    Restore {
        /// Where the entry was, and goes back to.
        path: WsPath,
        /// The trash id it was trashed under.
        trash_id: String,
    },
}

/// What an `fs.trash` did, with the entry's `size` and `modified` from before the move.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "do", rename_all = "snake_case")]
pub enum TrashDone {
    /// The entry was moved into the trash.
    Trashed {
        /// Where it was.
        path: WsPath,
        /// The trash id it is under.
        trash_id: String,
        /// Its `size`, as `fs.stat` gives it.
        size: u64,
        /// Its `modified`, as `fs.stat` gives it.
        modified: Option<i64>,
    },
    /// The entry was moved back out of the trash.
    Restored {
        /// Where it is now.
        path: WsPath,
        /// The trash id it was under.
        trash_id: String,
        /// Its `size`, as `fs.stat` gives it.
        size: u64,
        /// Its `modified`, as `fs.stat` gives it.
        modified: Option<i64>,
    },
}

impl OpKind for TrashOp {
    const KIND: &'static str = "fs.trash";
    type Plan = TrashPlan;
    type Done = TrashDone;
}

impl<A: WorkspaceApp> Operation<FsCtx<A>> for TrashOp {
    fn check(&self, ctx: &FsCtx<A>, plan: &TrashPlan) -> Result<(), Skip> {
        let (ws, app) = (ctx.workspace(), ctx.app());
        match plan {
            TrashPlan::Trash { path, expect } => {
                visible(ws, app, path)?;
                let resolved = ws.entry(app, path, Access::Visible)?;
                if let Some(expect) = expect {
                    expect::check(&resolved, expect)?;
                }
                Ok(())
            }
            TrashPlan::Restore { path, trash_id } => {
                visible(ws, app, path)?;
                let from = trashed_at(ctx, trash_id, path)?;
                visible_folder(ws, app, path)?;
                ws.move_entry(app, &from, path, Access::App, None, true)
                    .map(|_| ())
            }
        }
    }

    fn apply(&self, ctx: &FsCtx<A>, plan: &TrashPlan) -> Result<Applied<TrashDone>, Skip> {
        Ok(match plan {
            TrashPlan::Trash { path, expect } => trash(ctx, path, expect.as_ref())?,
            // Only an undo's: no follow-ups.
            TrashPlan::Restore { path, trash_id } => Applied::new(restore(ctx, path, trash_id)?),
        })
    }

    fn inverse(&self, done: &TrashDone) -> Option<TrashPlan> {
        Some(match done {
            TrashDone::Trashed { path, trash_id, .. } => TrashPlan::Restore {
                path: path.clone(),
                trash_id: trash_id.clone(),
            },
            TrashDone::Restored {
                path,
                size,
                modified,
                ..
            } => TrashPlan::Trash {
                path: path.clone(),
                expect: Some(Expect {
                    size: *size,
                    modified: *modified,
                    hash: None,
                }),
            },
        })
    }
}

/// [`Skip::InvalidPath`] for the root or a hidden path, the own folder and its trash among
/// them: the trash moves reach hidden entries, so they check their visible side here.
fn visible(ws: &Workspace, app: &dyn WorkspaceApp, path: &WsPath) -> Result<(), Skip> {
    if path.is_root() || ws.is_hidden(app, path) {
        Err(Skip::InvalidPath)
    } else {
        Ok(())
    }
}

/// [`Skip::InvalidPath`] if `path`'s folder is hidden as the disk spells it, which the moves
/// with [`Access::App`] do not judge: on a file system that ignores case, `META/x` may be
/// the hidden `meta/x`. [`Skip::NotFound`] if the folder is missing.
fn visible_folder(ws: &Workspace, app: &dyn WorkspaceApp, path: &WsPath) -> Result<(), Skip> {
    match path.parent() {
        Some(parent) if !parent.is_root() => ws.entry(app, &parent, Access::Visible).map(|_| ()),
        _ => Ok(()),
    }
}

/// `<own>/trash/<trash_id>`, or [`Skip::InvalidPath`] if `trash_id` is not a trash id.
fn trash_dir<A: WorkspaceApp>(ctx: &FsCtx<A>, trash_id: &str) -> Result<WsPath, Skip> {
    // Only the ids `fs.trash` makes: one plain segment.
    match trash_id.parse::<GrainId>() {
        Ok(id) if id.to_string() == trash_id => {}
        _ => return Err(Skip::InvalidPath),
    }
    ctx.own_folder()
        .join("trash")
        .and_then(|t| t.join(trash_id))
        .map_err(|_| Skip::InvalidPath)
}

/// Where the entry trashed from `path` under `trash_id` is.
fn trashed_at<A: WorkspaceApp>(
    ctx: &FsCtx<A>,
    trash_id: &str,
    path: &WsPath,
) -> Result<WsPath, Skip> {
    let mut at = trash_dir(ctx, trash_id)?;
    for segment in path.segments() {
        at = at.join(segment).map_err(|_| Skip::InvalidPath)?;
    }
    Ok(at)
}

/// Trashes the entry at `path`, with the app's follow-ups as the children.
fn trash<A: WorkspaceApp>(
    ctx: &FsCtx<A>,
    path: &WsPath,
    expect: Option<&Expect>,
) -> Result<Applied<TrashDone>, Skip> {
    let (ws, app) = (ctx.workspace(), ctx.app());
    visible(ws, app, path)?;
    // Before making any folder; the move checks it again.
    let resolved = ws.entry(app, path, Access::Visible)?;
    if let Some(expect) = expect {
        expect::check(&resolved, expect)?;
    }
    drop(resolved);

    let trash_id = GrainId::random().to_string();
    let own = ctx.own_folder();
    let trash = own.join("trash").map_err(|_| Skip::InvalidPath)?;
    for shared in [&own, &trash] {
        match ws.make_dir(app, shared, Access::App) {
            Ok(()) | Err(Skip::Exists) => {}
            Err(e) => return Err(e),
        }
    }
    // The id's folder, then one for each of `path`'s folders: each made by its folder's
    // handle, never through a symlink.
    let mut made = Vec::new();
    let result = (|| {
        let mut at = trash_dir(ctx, &trash_id)?;
        ws.make_dir(app, &at, Access::App)?;
        made.push(at.clone());
        let parent = path.parent().unwrap_or_else(WsPath::root);
        for segment in parent.segments() {
            at = at.join(segment).map_err(|_| Skip::InvalidPath)?;
            ws.make_dir(app, &at, Access::App)?;
            made.push(at.clone());
        }
        let to = at
            .join(path.name().ok_or(Skip::InvalidPath)?)
            .map_err(|_| Skip::InvalidPath)?;
        ws.move_entry(app, path, &to, Access::App, expect, false)
    })();
    let (size, modified) = match result {
        Ok(state) => state,
        Err(e) => {
            remove_emptied(ws, app, made.iter().rev());
            return Err(e);
        }
    };
    let op = FsOp::Trash {
        path: path.as_str().to_owned(),
        expect: expect.cloned(),
    };
    let trashed = FsDone::Trashed {
        path: path.clone(),
        trash_id: trash_id.clone(),
    };
    Ok(Applied {
        children: app.follow(&op, &trashed),
        done: TrashDone::Trashed {
            path: path.clone(),
            trash_id,
            size,
            modified,
        },
    })
}

fn restore<A: WorkspaceApp>(
    ctx: &FsCtx<A>,
    path: &WsPath,
    trash_id: &str,
) -> Result<TrashDone, Skip> {
    let (ws, app) = (ctx.workspace(), ctx.app());
    visible(ws, app, path)?;
    let top = trash_dir(ctx, trash_id)?;
    let from = trashed_at(ctx, trash_id, path)?;
    visible_folder(ws, app, path)?;
    let (size, modified) = ws.move_entry(app, &from, path, Access::App, None, false)?;
    // The folders it was in, from the deepest up to the id's own.
    let mut emptied = Vec::new();
    let mut at = from.parent();
    while let Some(dir) = at {
        let done = dir == top;
        at = dir.parent();
        emptied.push(dir);
        if done {
            break;
        }
    }
    remove_emptied(ws, app, emptied.iter());
    Ok(TrashDone::Restored {
        path: path.clone(),
        trash_id: trash_id.to_owned(),
        size,
        modified,
    })
}

/// Removes `dirs`, in order, while they are empty; best effort.
fn remove_emptied<'a>(
    ws: &Workspace,
    app: &dyn WorkspaceApp,
    dirs: impl Iterator<Item = &'a WsPath>,
) {
    for dir in dirs {
        if let Err(e) = ws.remove_empty_dir(app, dir, Access::App) {
            tracing::debug!("the trash folder {dir} is left: {e:?}");
            break;
        }
    }
}
