mod common;

use common::workspace;
use corundum_framework_fs::api::{Entry, Listing, ReadResult};
use corundum_framework_fs::journal::Actor;
use corundum_framework_fs::{FsError, FsService};
use serde_json::{Value, json};

fn actor() -> Actor {
    Actor::person("p")
}

fn service() -> (tempfile::TempDir, FsService<()>) {
    let (dir, ws) = workspace(&["a.txt", "b/"]);
    (dir, FsService::new(ws, ()))
}

#[tokio::test]
async fn dispatches_each_method() {
    let (_dir, s) = service();
    let l: Listing = serde_json::from_value(
        s.dispatch("fs.list", json!({"path": ""}), &actor())
            .await
            .unwrap(),
    )
    .unwrap();
    let names: Vec<_> = l.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["a.txt", "b"]);
    let e: Entry = serde_json::from_value(
        s.dispatch("fs.stat", json!({"path": "a.txt"}), &actor())
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(e.size, 5);
    let r: ReadResult = serde_json::from_value(
        s.dispatch(
            "fs.read",
            json!({"path": "a.txt", "offset": 0, "len": 5}),
            &actor(),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    assert_eq!(r.total, 5);
}

#[tokio::test]
async fn unknown_methods_are_method_not_found() {
    let (_dir, s) = service();
    let e = s
        .dispatch("folder.list", json!({}), &actor())
        .await
        .unwrap_err();
    assert!(matches!(e, FsError::MethodNotFound(_)));
    assert_eq!(e.code(), -32601);
}

#[tokio::test]
async fn bad_params_are_invalid_params() {
    let (_dir, s) = service();
    let e = s
        .dispatch("fs.read", json!({"path": 3}), &actor())
        .await
        .unwrap_err();
    assert!(matches!(e, FsError::InvalidParams(_)));
    assert_eq!(e.code(), -32602);
    let e = s
        .dispatch("fs.list", Value::Null, &actor())
        .await
        .unwrap_err();
    assert!(matches!(e, FsError::InvalidParams(_)));
}

#[tokio::test]
async fn errors_keep_their_codes() {
    let (_dir, s) = service();
    let e = s
        .dispatch("fs.stat", json!({"path": "../x"}), &actor())
        .await
        .unwrap_err();
    assert_eq!(e.code(), -32101);
}
