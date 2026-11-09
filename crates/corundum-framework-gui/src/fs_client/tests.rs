use std::time::{Duration, Instant};

use corundum_framework_fs::journal::Actor;
use corundum_framework_fs::{FsService, Workspace};
use corundum_framework_fs_api::{API_VERSION, EntryKind, FsOp, codes};
use corundum_framework_platform::pending::Pending;
use serde_json::{Value, json};

use super::{FsClient, FsError, FsTransport, LocalTransport};

/// The answer, polled every 5 ms for up to 5 s.
fn done<T>(mut p: Pending<T>) -> T {
    let start = Instant::now();
    loop {
        if let Some(answer) = p.poll() {
            return answer;
        }
        assert!(start.elapsed() < Duration::from_secs(5), "no answer in 5 s");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// A workspace with `a.txt` ("abcd") and `b/`, served on a runtime of its own.
struct Served {
    dir: tempfile::TempDir,
    service: FsService<()>,
    runtime: tokio::runtime::Runtime,
}

impl Served {
    fn new() -> Served {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), b"abcd").unwrap();
        std::fs::create_dir(dir.path().join("b")).unwrap();
        let service = FsService::new(Workspace::open(dir.path()).unwrap(), ());
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .build()
            .unwrap();
        Served {
            dir,
            service,
            runtime,
        }
    }

    fn client(&self) -> FsClient {
        FsClient::new(LocalTransport::new(
            self.service.clone(),
            Actor::person("p"),
            self.runtime.handle().clone(),
            || {},
        ))
    }
}

/// A server answering every call with `answer`.
struct Fake(Result<Value, FsError>);

impl FsTransport for Fake {
    fn call(&self, _method: &str, _params: Value) -> Pending<Result<Value, FsError>> {
        Pending::ready(self.0.clone())
    }
}

#[test]
fn a_listing_comes_back_typed() {
    let s = Served::new();
    let listing = done(s.client().list("", None)).unwrap();
    let names: Vec<_> = listing.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["a.txt", "b"]);
    let kinds: Vec<_> = listing.entries.iter().map(|e| e.kind).collect();
    assert_eq!(kinds, [EntryKind::File, EntryKind::Dir]);
}

#[test]
fn apply_then_undo_through_the_client() {
    let s = Served::new();
    let client = s.client();
    let ops = vec![FsOp::Move {
        from: "a.txt".into(),
        to: "c.txt".into(),
        expect: None,
    }];
    let applied = done(client.apply(ops, Some("move".into()))).unwrap();
    assert!(applied.summary.begin.origin.starts_with("gui:"));
    assert_eq!(applied.summary.begin.origin, client.origin());
    assert!(s.dir.path().join("c.txt").exists());
    let undone = done(client.undo()).unwrap();
    assert_eq!(
        undone.summary.begin.undo_of.as_deref(),
        Some(applied.summary.begin.batch_id.as_str())
    );
    assert!(s.dir.path().join("a.txt").exists());
    assert!(!s.dir.path().join("c.txt").exists());
}

#[test]
fn each_client_has_its_own_origin() {
    let s = Served::new();
    let (first, second) = (s.client(), s.client());
    assert_ne!(first.origin(), second.origin());
    let ops = vec![FsOp::Mkdir { path: "d".into() }];
    done(first.apply(ops, None)).unwrap();
    let err = done(second.undo()).unwrap_err();
    assert!(
        matches!(err, FsError::Rpc { code, .. } if code == codes::NOTHING_TO_UNDO),
        "{err:?}"
    );
}

#[test]
fn a_server_error_keeps_its_code() {
    let s = Served::new();
    let err = done(s.client().list("nope", None)).unwrap_err();
    assert!(
        matches!(err, FsError::Rpc { code, .. } if code == codes::NOT_FOUND),
        "{err:?}"
    );
    assert_eq!(err.code(), codes::NOT_FOUND);
}

#[test]
fn the_version_matches() {
    let s = Served::new();
    assert_eq!(done(s.client().check_version()), Ok(()));
}

#[test]
fn a_mismatched_version_fails_every_call() {
    let client = FsClient::new(Fake(Ok(json!({"api": 99}))));
    let mismatch = FsError::ApiMismatch {
        server: 99,
        client: API_VERSION,
    };
    assert_eq!(API_VERSION, 1);
    assert_eq!(done(client.check_version()), Err(mismatch.clone()));
    assert_eq!(done(client.list("", None)), Err(mismatch.clone()));
    assert_eq!(mismatch.code(), codes::API_MISMATCH);
}

#[test]
fn a_server_without_fs_version_is_a_mismatch() {
    let client = FsClient::new(Fake(Err(FsError::Rpc {
        code: codes::METHOD_NOT_FOUND,
        message: "fs.version".into(),
    })));
    assert!(matches!(
        done(client.check_version()),
        Err(FsError::ApiMismatch { server: 0, .. })
    ),);
}

#[test]
fn read_decodes_the_bytes() {
    let s = Served::new();
    let client = s.client();
    assert_eq!(
        done(client.read_with_total("a.txt", 1, 2)).unwrap(),
        (b"bc".to_vec(), 4)
    );
    assert_eq!(done(client.read("a.txt", 1, 2)).unwrap(), b"bc");
}

#[test]
fn a_request_lost_with_its_runtime_is_a_transport_error() {
    let s = Served::new();
    let client = s.client();
    let Served {
        dir: _dir, runtime, ..
    } = s;
    runtime.shutdown_background();
    assert_eq!(done(client.stat("a.txt")), Err(FsError::default()));
}
