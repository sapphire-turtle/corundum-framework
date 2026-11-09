//! A temporary workspace for the tests.

#![allow(dead_code)]

use std::fs;
use std::path::Path;

use corundum_framework_fs::{Workspace, WorkspaceApp, WsPath};

/// A temporary folder with `files` in it (a trailing `/` makes a folder), and its workspace.
pub fn workspace(files: &[&str]) -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().unwrap();
    for f in files {
        let path = dir.path().join(f);
        if let Some(folder) = f.strip_suffix('/') {
            fs::create_dir_all(dir.path().join(folder)).unwrap();
        } else {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, f.as_bytes()).unwrap();
        }
    }
    let ws = Workspace::open(dir.path()).unwrap();
    (dir, ws)
}

pub fn write(root: &Path, rel: &str, contents: &[u8]) {
    fs::write(root.join(rel), contents).unwrap();
}

pub fn p(s: &str) -> WsPath {
    WsPath::parse(s).unwrap()
}

/// An app that hides `*.xmp` and everything under `meta`.
pub struct HidesXmp;

impl WorkspaceApp for HidesXmp {
    fn hidden(&self, path: &WsPath) -> bool {
        path.as_str().ends_with(".xmp") || path.as_str() == "meta"
    }
}

/// An app that manages `*.jpg` as photos, and checks the folder it is told of.
pub struct ManagesJpg {
    pub expect_dir: WsPath,
}

impl WorkspaceApp for ManagesJpg {
    fn annotate(&self, dir: &WsPath, entries: &mut [corundum_framework_fs::api::Entry]) {
        assert_eq!(dir, &self.expect_dir);
        for e in entries {
            if e.name.ends_with(".jpg") {
                e.managed = Some(corundum_framework_fs::api::Managed {
                    kind: "photo".into(),
                    item_id: e.name.clone(),
                });
            }
        }
    }
}

use corundum_framework_fs::{FsCtx, register_builtin};
use corundum_framework_journal::{
    Actor, Journal, JournalConfig, MemoryStore, Origin, Outcome, Planned, Registry,
};
use std::sync::Arc;

/// A journal running the built-in kinds over `ws`, for `app`, in memory.
pub fn journal<A: WorkspaceApp>(ws: Workspace, app: A) -> Journal<FsCtx<A>> {
    let mut registry = Registry::new();
    register_builtin(&mut registry);
    Journal::new(
        registry,
        FsCtx::new(Arc::new(ws), Arc::new(app)),
        MemoryStore::default(),
        JournalConfig::default(),
    )
}

pub fn origin() -> Origin {
    Origin::new("test")
}

pub fn actor() -> Actor {
    Actor::person("u1")
}

/// Runs `op` as a batch of its own, and gives its outcome.
pub fn run<A: WorkspaceApp>(j: &Journal<FsCtx<A>>, op: Planned) -> Outcome {
    let id = j.submit(vec![op], &origin(), &actor(), None).unwrap();
    let b = j.wait(id).unwrap();
    assert_eq!(b.ops.len(), 1);
    b.ops[0].outcome.clone()
}

/// Undoes the newest batch, and gives the outcome of its one operation.
pub fn undo<A: WorkspaceApp>(j: &Journal<FsCtx<A>>) -> Outcome {
    let id = j.undo(&origin(), &actor()).unwrap();
    let b = j.wait(id).unwrap();
    assert_eq!(b.ops.len(), 1);
    b.ops[0].outcome.clone()
}

/// The names in the folder `rel` under `root`, sorted, straight from the file system.
pub fn names(root: &Path, rel: &str) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(root.join(rel))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    v.sort();
    v
}
