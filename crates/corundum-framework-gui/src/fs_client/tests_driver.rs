use std::time::{Duration, Instant};

use corundum_framework_fs::journal::Actor;
use corundum_framework_fs::{FsService, Workspace, WorkspaceApp, WsPath};
use corundum_framework_fs_api::{BatchRecords, Entry, Managed};
use corundum_framework_platform::pending::{self, Pending};
use serde_json::{Value, json};

use super::{DriverEvent, ExplorerDriver, FsClient, FsError, FsTransport, LocalTransport};
use crate::explorer::ExplorerEvent;

/// A workspace on a tempdir, served on a runtime of its own, and a driver over it, drawn
/// headless frame by frame.
struct Rig {
    dir: tempfile::TempDir,
    _runtime: tokio::runtime::Runtime,
    client: FsClient,
    driver: ExplorerDriver,
    ctx: egui::Context,
    said: Vec<DriverEvent>,
}

impl Rig {
    /// A workspace of `names` (a trailing `/` makes a folder; a file holds `"a"`), served
    /// for `app`, its transport made by `wrap`.
    fn with<A: WorkspaceApp>(
        names: &[&str],
        app: A,
        wrap: impl FnOnce(LocalTransport) -> FsClient,
    ) -> Rig {
        let dir = tempfile::tempdir().unwrap();
        for name in names {
            let path = dir.path().join(name.trim_end_matches('/'));
            if name.ends_with('/') {
                std::fs::create_dir_all(path).unwrap();
            } else {
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, b"a").unwrap();
            }
        }
        let service = FsService::new(Workspace::open(dir.path()).unwrap(), app);
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .build()
            .unwrap();
        let transport =
            LocalTransport::new(service, Actor::person("p"), runtime.handle().clone(), || {});
        let client = wrap(transport);
        Rig {
            dir,
            _runtime: runtime,
            driver: ExplorerDriver::new(client.clone(), "Files"),
            client,
            ctx: egui::Context::default(),
            said: Vec::new(),
        }
    }

    fn new(names: &[&str]) -> Rig {
        Rig::with(names, (), FsClient::new)
    }

    fn frame(&mut self) {
        frame(&mut self.driver, &self.ctx, &mut self.said);
    }

    /// Frames until no request is in flight, for up to 5 s.
    fn settle(&mut self) {
        settle(&mut self.driver, &self.ctx, &mut self.said);
    }

    /// The person going into `location`: the explorer goes there, and says so.
    fn navigate(&mut self, location: &[&str]) {
        let location: Vec<String> = location.iter().map(|s| s.to_string()).collect();
        self.driver.explorer_mut().go_to(location.clone());
        self.feed(ExplorerEvent::Navigated(location));
    }

    fn feed(&mut self, event: ExplorerEvent) -> Option<DriverEvent> {
        self.driver.handle(&self.ctx, event)
    }

    fn names(&self) -> Vec<String> {
        names(&self.driver)
    }

    /// The workspace's latest batch, with its operations.
    fn last_batch(&self) -> BatchRecords {
        let history = pending_done(self.client.history(None)).unwrap();
        let latest = &history.batches[0].summary.begin.batch_id;
        pending_done(self.client.status(latest)).unwrap()
    }
}

fn names(driver: &ExplorerDriver) -> Vec<String> {
    driver
        .explorer()
        .entries()
        .iter()
        .map(|e| e.name.clone())
        .collect()
}

fn frame(driver: &mut ExplorerDriver, ctx: &egui::Context, said: &mut Vec<DriverEvent>) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(600.0, 400.0),
        )),
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        egui::CentralPanel::default().show(ui, |ui| said.extend(driver.ui(ui)));
    });
    output.textures_delta.clear();
}

fn settle(driver: &mut ExplorerDriver, ctx: &egui::Context, said: &mut Vec<DriverEvent>) {
    let start = Instant::now();
    frame(driver, ctx, said);
    while driver.is_busy() {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "still busy after 5 s"
        );
        std::thread::sleep(Duration::from_millis(5));
        frame(driver, ctx, said);
    }
}

fn strings(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| s.to_string()).collect()
}

#[test]
fn the_root_is_listed_on_the_first_frame() {
    let mut rig = Rig::new(&["a.txt", "b/"]);
    rig.frame();
    assert!(rig.driver.is_busy(), "the first frame asks");
    rig.settle();
    assert_eq!(rig.names(), ["a.txt", "b"]);
    let entries = rig.driver.explorer().entries();
    assert!(!entries[0].is_folder());
    assert_eq!(entries[0].size, Some(1));
    assert!(entries[0].modified.is_some());
    assert!(entries[1].is_folder());
}

/// A transport that answers the listing of `a` 300 ms late.
struct SlowA(LocalTransport);

impl FsTransport for SlowA {
    fn call(&self, method: &str, params: Value) -> Pending<Result<Value, FsError>> {
        let slow = method == "fs.list" && params["path"] == "a";
        let answer = self.0.call(method, params);
        if !slow {
            return answer;
        }
        let (late, sender) = pending::channel(|| {});
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            sender.send(Some(answer.wait()));
        });
        late.map(|answer| answer.unwrap_or_else(|| Err(FsError::default())))
    }
}

#[test]
fn a_late_listing_for_a_left_folder_is_dropped() {
    let mut rig = Rig::with(&["a/x.txt", "b/y.txt"], (), |t| FsClient::new(SlowA(t)));
    rig.settle();
    rig.navigate(&["a"]);
    rig.frame();
    rig.navigate(&["b"]);
    rig.settle();
    assert_eq!(rig.names(), ["y.txt"]);
    // Well after a's answer would have come.
    std::thread::sleep(Duration::from_millis(400));
    rig.settle();
    assert_eq!(rig.driver.explorer().location(), ["b"]);
    assert_eq!(rig.names(), ["y.txt"]);
}

#[test]
fn paging_appends() {
    let files: Vec<String> = (0..250).map(|i| format!("f{i:03}")).collect();
    let files: Vec<&str> = files.iter().map(String::as_str).collect();
    let mut rig = Rig::new(&files);
    rig.settle();
    assert_eq!(rig.names().len(), 200);
    assert_eq!(rig.feed(ExplorerEvent::LoadMore), None);
    rig.settle();
    let names = rig.names();
    assert_eq!(names.len(), 250);
    assert_eq!(names[249], "f249");
}

#[test]
fn a_rename_applies_one_move_with_its_expect() {
    let mut rig = Rig::new(&["a.txt", "b/"]);
    rig.settle();
    rig.feed(ExplorerEvent::Rename {
        name: "a.txt".into(),
        to: "c.txt".into(),
    });
    rig.settle();
    assert!(rig.dir.path().join("c.txt").exists());
    assert!(!rig.dir.path().join("a.txt").exists());
    let batch = rig.last_batch();
    assert_eq!(batch.ops.len(), 1);
    assert_eq!(batch.ops[0].plan["expect"]["size"], 1);
    assert_eq!(batch.ops[0].plan["from"], "a.txt");
    assert_eq!(batch.ops[0].plan["to"], "c.txt");
    assert_eq!(
        batch.summary.begin.label.as_deref(),
        Some("Rename \u{201c}a.txt\u{201d}")
    );
    assert_eq!(rig.names(), ["b", "c.txt"]);
    assert_eq!(rig.driver.notice(), None);
}

#[test]
fn a_rename_of_a_changed_file_is_skipped_and_said() {
    let mut rig = Rig::new(&["a.txt", "b/"]);
    rig.settle();
    std::fs::write(rig.dir.path().join("a.txt"), b"changed!").unwrap();
    rig.feed(ExplorerEvent::Rename {
        name: "a.txt".into(),
        to: "c.txt".into(),
    });
    rig.settle();
    assert!(rig.dir.path().join("a.txt").exists());
    assert!(!rig.dir.path().join("c.txt").exists());
    let notice = rig.driver.notice().expect("a notice");
    assert!(notice.contains("skipped"), "{notice}");
    assert_eq!(notice, "Rename \u{201c}a.txt\u{201d} (1 skipped)");
    // Listed again: the explorer has the file's new size.
    let a = &rig.driver.explorer().entries()[0];
    assert_eq!((a.name.as_str(), a.size), ("a.txt", Some(8)));
}

#[test]
fn a_drag_moves_each_name_in_one_batch() {
    let mut rig = Rig::new(&["a.txt", "b.txt", "d/"]);
    rig.settle();
    rig.feed(ExplorerEvent::Move {
        names: strings(&["a.txt", "b.txt"]),
        to: strings(&["d"]),
    });
    rig.settle();
    let batch = rig.last_batch();
    assert_eq!(batch.ops.len(), 2);
    assert_eq!(batch.summary.begin.label.as_deref(), Some("Move 2 items"));
    let history = pending_done(rig.client.history(None)).unwrap();
    assert_eq!(history.batches.len(), 1);
    assert!(rig.dir.path().join("d/a.txt").exists());
    assert!(rig.dir.path().join("d/b.txt").exists());
    assert_eq!(rig.names(), ["d"]);
}

#[test]
fn trash_from_the_menu() {
    let mut rig = Rig::new(&["a.txt", "b/"]);
    rig.settle();
    rig.feed(ExplorerEvent::Menu {
        id: "trash".into(),
        names: strings(&["a.txt"]),
    });
    rig.settle();
    assert_eq!(rig.names(), ["b"]);
    let batch = rig.last_batch();
    assert_eq!(batch.ops[0].kind, "fs.trash");
    assert_eq!(batch.ops[0].plan["expect"]["size"], 1);
}

/// An app that manages `x.jpg` as photo 7.
struct Photos;

impl WorkspaceApp for Photos {
    fn annotate(&self, _dir: &WsPath, entries: &mut [Entry]) {
        for e in entries {
            if e.name == "x.jpg" {
                e.managed = Some(Managed {
                    kind: "photo".into(),
                    item_id: "7".into(),
                });
            }
        }
    }
}

#[test]
fn a_managed_entry_is_decorated_and_opens_as_managed() {
    let mut rig = Rig::with(&["a.txt", "x.jpg", "d/e.txt"], Photos, FsClient::new);
    rig.driver = ExplorerDriver::new(rig.client.clone(), "Photos").decorate(
        |entry: &mut crate::explorer::Entry, managed: &Managed| {
            assert_eq!(managed.kind, "photo");
            entry.icon = Some(crate::icons::picture());
        },
    );
    rig.settle();
    let entries = rig.driver.explorer().entries();
    let icon = |name: &str| {
        entries
            .iter()
            .find(|e| e.name == name)
            .unwrap()
            .icon
            .is_some()
    };
    assert!(icon("x.jpg"));
    assert!(!icon("a.txt"));
    assert_eq!(
        rig.feed(ExplorerEvent::Opened("x.jpg".into())),
        Some(DriverEvent::OpenManaged {
            item_id: "7".into()
        })
    );
    assert_eq!(
        rig.feed(ExplorerEvent::Opened("a.txt".into())),
        Some(DriverEvent::Opened {
            path: "a.txt".into()
        })
    );
    rig.navigate(&["d"]);
    rig.settle();
    assert_eq!(
        rig.feed(ExplorerEvent::Opened("e.txt".into())),
        Some(DriverEvent::Opened {
            path: "d/e.txt".into()
        })
    );
}

/// A server of another API version.
struct Api99;

impl FsTransport for Api99 {
    fn call(&self, _method: &str, _params: Value) -> Pending<Result<Value, FsError>> {
        Pending::ready(Ok(json!({"api": 99})))
    }
}

#[test]
fn a_version_mismatch_shows_as_failed() {
    let ctx = egui::Context::default();
    let mut driver = ExplorerDriver::new(FsClient::new(Api99), "Files");
    let mut said = Vec::new();
    settle(&mut driver, &ctx, &mut said);
    let failed = driver.explorer().failed().expect("failed");
    assert_eq!(
        failed,
        "This app's file API (version 1) does not match the server's (version 99)"
    );
    assert!(driver.explorer().entries().is_empty());
}

fn pending_done<T>(mut p: Pending<T>) -> T {
    let start = Instant::now();
    loop {
        if let Some(answer) = p.poll() {
            return answer;
        }
        assert!(start.elapsed() < Duration::from_secs(5), "no answer in 5 s");
        std::thread::sleep(Duration::from_millis(5));
    }
}
