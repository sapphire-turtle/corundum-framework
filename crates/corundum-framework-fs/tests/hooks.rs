mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::{names, workspace};
use corundum_framework_fs::api::{BatchRecords, DryRun, FsOp, Outcome, codes};
use corundum_framework_fs::journal::{
    Actor, Applied, BatchBegin, JournalConfig, MemoryStore, OpKind, Operation, Planned, Registry,
    Skip,
};
use corundum_framework_fs::ops::FsDone;
use corundum_framework_fs::{Access, FsCtx, FsError, FsService, Route, WorkspaceApp, WsPath};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Hides `*.xmp`, moves a photo's sidecar with it, archives albums instead of trashing
/// them, denies anything under `locked`, and lets no agent undo.
struct Photos {
    routed: Arc<AtomicUsize>,
}

const LOCKED: &str = "locked is read-only";

fn under_locked(path: &str) -> bool {
    path == "locked" || path.starts_with("locked/")
}

impl WorkspaceApp for Photos {
    fn hidden(&self, path: &WsPath) -> bool {
        path.as_str().ends_with(".xmp")
    }

    fn route(&self, op: &FsOp) -> Route {
        self.routed.fetch_add(1, Ordering::SeqCst);
        let paths: Vec<&str> = match op {
            FsOp::Move { from, to, .. } => vec![from, to],
            FsOp::Mkdir { path } => vec![path],
            FsOp::Trash { path, .. } => vec![path],
        };
        if paths.iter().any(|p| under_locked(p)) {
            return Route::Deny(LOCKED.into());
        }
        match op {
            FsOp::Trash { path, .. } if path.ends_with(".album") => {
                Route::Replace(vec![Planned::new::<Archive>(&ArchivePlan {
                    path: WsPath::parse(path).unwrap(),
                    back: false,
                })])
            }
            _ => Route::Fs,
        }
    }

    fn follow(&self, op: &FsOp, done: &FsDone) -> Vec<Planned> {
        match (op, done) {
            (FsOp::Move { from, .. }, FsDone::Moved { from: f, to }) if from.ends_with(".jpg") => {
                vec![Planned::new::<MoveSidecar>(&SidecarPlan {
                    from: WsPath::parse(&format!("{f}.xmp")).unwrap(),
                    to: WsPath::parse(&format!("{to}.xmp")).unwrap(),
                })]
            }
            _ => Vec::new(),
        }
    }

    fn may_undo(&self, actor: &Actor, batch: &BatchBegin) -> bool {
        !actor.agent && actor.principal == batch.actor.principal
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SidecarPlan {
    from: WsPath,
    to: WsPath,
}

struct MoveSidecar;

impl OpKind for MoveSidecar {
    const KIND: &'static str = "test.move_sidecar";
    type Plan = SidecarPlan;
    type Done = SidecarPlan;
}

impl Operation<FsCtx<Photos>> for MoveSidecar {
    fn apply(&self, ctx: &FsCtx<Photos>, plan: &SidecarPlan) -> Result<Applied<SidecarPlan>, Skip> {
        ctx.workspace()
            .rename(ctx.app(), &plan.from, &plan.to, Access::App)?;
        Ok(Applied::new(plan.clone()))
    }

    fn inverse(&self, done: &SidecarPlan) -> Option<SidecarPlan> {
        Some(SidecarPlan {
            from: done.to.clone(),
            to: done.from.clone(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ArchivePlan {
    path: WsPath,
    /// Move it back out of `archive`.
    #[serde(default)]
    back: bool,
}

struct Archive;

impl OpKind for Archive {
    const KIND: &'static str = "test.archive";
    type Plan = ArchivePlan;
    type Done = ArchivePlan;
}

impl Operation<FsCtx<Photos>> for Archive {
    fn apply(&self, ctx: &FsCtx<Photos>, plan: &ArchivePlan) -> Result<Applied<ArchivePlan>, Skip> {
        let archived = WsPath::parse(&format!("archive/{}", plan.path.name().unwrap())).unwrap();
        let (from, to) = if plan.back {
            (&archived, &plan.path)
        } else {
            (&plan.path, &archived)
        };
        ctx.workspace().rename(ctx.app(), from, to, Access::App)?;
        Ok(Applied::new(plan.clone()))
    }

    fn inverse(&self, done: &ArchivePlan) -> Option<ArchivePlan> {
        Some(ArchivePlan {
            path: done.path.clone(),
            back: !done.back,
        })
    }
}

fn service(files: &[&str]) -> (tempfile::TempDir, FsService<Photos>, Arc<AtomicUsize>) {
    let (dir, ws) = workspace(files);
    let routed = Arc::new(AtomicUsize::new(0));
    let mut kinds = Registry::new();
    kinds.register(MoveSidecar).register(Archive);
    let s = FsService::with_journal(
        ws,
        Photos {
            routed: Arc::clone(&routed),
        },
        kinds,
        MemoryStore::default(),
        JournalConfig::default(),
    );
    (dir, s, routed)
}

fn person() -> Actor {
    Actor::person("p")
}

async fn call(s: &FsService<Photos>, method: &str, params: Value) -> Result<Value, FsError> {
    s.dispatch(method, params, &person()).await
}

async fn apply(s: &FsService<Photos>, ops: Value) -> BatchRecords {
    serde_json::from_value(
        call(s, "fs.apply", json!({"ops": ops, "origin": "gui"}))
            .await
            .unwrap(),
    )
    .unwrap()
}

fn mv(from: &str, to: &str) -> Value {
    json!({"kind": "fs.move", "from": from, "to": to})
}

fn skipped(o: &Outcome) -> &str {
    match o {
        Outcome::Skipped(s) => &s.reason,
        other => panic!("not skipped: {other:?}"),
    }
}

#[tokio::test]
async fn a_sidecar_moves_with_its_photo() {
    let (dir, s, _) = service(&["p.jpg", "p.jpg.xmp"]);
    let b = apply(&s, json!([mv("p.jpg", "q.jpg")])).await;
    assert_eq!(b.ops.len(), 2, "{b:?}");
    assert!(
        b.ops.iter().all(|o| matches!(o.outcome, Outcome::Done(_))),
        "{b:?}"
    );
    assert_eq!(b.ops[0].kind, "fs.move");
    assert_eq!(b.ops[1].kind, "test.move_sidecar");
    assert_eq!(b.ops[1].parent, Some(b.ops[0].op_id.clone()));
    assert_eq!(names(dir.path(), ""), ["q.jpg", "q.jpg.xmp"]);
}

#[tokio::test]
async fn the_undo_moves_the_sidecar_back_too() {
    let (dir, s, _) = service(&["p.jpg", "p.jpg.xmp"]);
    apply(&s, json!([mv("p.jpg", "q.jpg")])).await;
    let u: BatchRecords = serde_json::from_value(
        call(&s, "journal.undo", json!({"origin": "gui"}))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(u.ops.len(), 2, "{u:?}");
    assert!(
        u.ops.iter().all(|o| matches!(o.outcome, Outcome::Done(_))),
        "{u:?}"
    );
    assert_eq!(names(dir.path(), ""), ["p.jpg", "p.jpg.xmp"]);
}

#[tokio::test]
async fn a_replaced_op_runs_the_apps_kinds_instead() {
    let (dir, s, _) = service(&["a.album", "archive/"]);
    let b = apply(&s, json!([{"kind": "fs.trash", "path": "a.album"}])).await;
    assert_eq!(b.ops.len(), 1, "{b:?}");
    assert_eq!(b.ops[0].kind, "test.archive");
    assert!(matches!(b.ops[0].outcome, Outcome::Done(_)), "{b:?}");
    assert_eq!(names(dir.path(), ""), ["archive"]);
    assert_eq!(names(dir.path(), "archive"), ["a.album"]);
    call(&s, "journal.undo", json!({"origin": "gui"}))
        .await
        .unwrap();
    assert_eq!(names(dir.path(), ""), ["a.album", "archive"]);
}

#[tokio::test]
async fn a_denied_op_is_skipped_with_its_reason() {
    let (dir, s, _) = service(&["locked/a"]);
    let b = apply(
        &s,
        json!([{"kind": "fs.trash", "path": "locked/a"}, {"kind": "fs.mkdir", "path": "d"}]),
    )
    .await;
    assert_eq!(b.ops.len(), 2, "{b:?}");
    assert_eq!(b.ops[0].kind, "fs.deny");
    assert_eq!(skipped(&b.ops[0].outcome), "denied");
    assert_eq!(b.ops[0].plan["reason"], LOCKED);
    assert_eq!(b.ops[0].plan["op"]["path"], "locked/a");
    assert!(matches!(b.ops[1].outcome, Outcome::Done(_)), "{b:?}");
    assert_eq!(names(dir.path(), ""), ["d", "locked"]);
    assert_eq!(names(dir.path(), "locked"), ["a"]);
}

#[tokio::test]
async fn a_hidden_path_is_never_routed() {
    let (dir, s, routed) = service(&["p.jpg.xmp"]);
    let b = apply(&s, json!([mv("p.jpg.xmp", "z.xmp")])).await;
    assert_eq!(b.ops.len(), 1, "{b:?}");
    assert_eq!(b.ops[0].kind, "fs.move");
    assert_eq!(skipped(&b.ops[0].outcome), "invalid_path");
    assert_eq!(routed.load(Ordering::SeqCst), 0);
    // Only the destination hidden: still not routed.
    let b = apply(&s, json!([mv("locked", "z.xmp")])).await;
    assert_eq!(b.ops[0].kind, "fs.move");
    assert_eq!(skipped(&b.ops[0].outcome), "invalid_path");
    assert_eq!(routed.load(Ordering::SeqCst), 0);
    assert_eq!(names(dir.path(), ""), ["p.jpg.xmp"]);
}

#[tokio::test]
async fn may_undo_decides_who_undoes() {
    let (dir, s, _) = service(&["a"]);
    let b = apply(&s, json!([mv("a", "b")])).await;
    let params = json!({"batch_id": b.summary.begin.batch_id, "origin": "history"});
    let e = s
        .dispatch("journal.undo_batch", params.clone(), &Actor::agent("p"))
        .await
        .unwrap_err();
    assert_eq!(e.code(), codes::FORBIDDEN);
    assert_eq!(names(dir.path(), ""), ["b"]);
    s.dispatch("journal.undo_batch", params, &Actor::person("p"))
        .await
        .unwrap();
    assert_eq!(names(dir.path(), ""), ["a"]);
}

#[tokio::test]
async fn a_dry_run_reports_a_denial() {
    let (_dir, s, _) = service(&["locked/a"]);
    let d: DryRun = serde_json::from_value(
        call(
            &s,
            "fs.apply",
            json!({"ops": [{"kind": "fs.trash", "path": "locked/a"}], "origin": "gui", "dry_run": true}),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    assert_eq!(d.checks.len(), 1);
    assert_eq!(d.checks[0].kind, "fs.deny");
    assert_eq!(d.checks[0].skip.as_ref().unwrap().reason, "denied");
}

/// Replaces every `fs.mkdir` with nothing.
struct Swallows;

impl WorkspaceApp for Swallows {
    fn route(&self, op: &FsOp) -> Route {
        match op {
            FsOp::Mkdir { .. } => Route::Replace(Vec::new()),
            _ => Route::Fs,
        }
    }
}

#[tokio::test]
async fn an_op_replaced_with_nothing_vanishes_and_an_empty_batch_is_refused() {
    let (dir, ws) = workspace(&["a"]);
    let s = FsService::new(ws, Swallows);
    let b: BatchRecords = serde_json::from_value(
        s.dispatch(
            "fs.apply",
            json!({"ops": [{"kind": "fs.mkdir", "path": "d"}, mv("a", "b")], "origin": "gui"}),
            &person(),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    assert_eq!(b.ops.len(), 1, "{b:?}");
    assert_eq!(b.ops[0].kind, "fs.move");
    let e = s
        .dispatch(
            "fs.apply",
            json!({"ops": [{"kind": "fs.mkdir", "path": "d"}], "origin": "gui"}),
            &person(),
        )
        .await
        .unwrap_err();
    assert_eq!(e.code(), codes::INVALID_PARAMS);
    let h: Value = s
        .dispatch("journal.history", json!({}), &person())
        .await
        .unwrap();
    assert_eq!(h["batches"].as_array().unwrap().len(), 1);
    assert_eq!(names(dir.path(), ""), ["b"]);
}
