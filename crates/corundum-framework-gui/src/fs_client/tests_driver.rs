use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
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
    /// The texts the last frame drew, and where.
    drawn: Vec<(String, egui::Rect)>,
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
            drawn: Vec::new(),
        }
    }

    fn new(names: &[&str]) -> Rig {
        Rig::with(names, (), FsClient::new)
    }

    fn frame(&mut self) {
        self.frame_with(vec![]);
    }

    /// A frame with `events` as its input.
    fn frame_with(&mut self, events: Vec<egui::Event>) {
        self.drawn = frame(&mut self.driver, &self.ctx, &mut self.said, events);
    }

    /// Frames until no request is in flight, for up to 5 s.
    fn settle(&mut self) {
        self.drawn = settle(&mut self.driver, &self.ctx, &mut self.said);
    }

    /// Where the last frame drew `text`.
    fn drawn_at(&self, text: &str) -> egui::Pos2 {
        self.drawn
            .iter()
            .find(|(t, _)| t == text)
            .unwrap_or_else(|| panic!("{text:?} was not drawn: {:?}", self.drawn))
            .1
            .center()
    }

    /// Whether the last frame drew a text containing `needle`.
    fn shows(&self, needle: &str) -> bool {
        self.drawn.iter().any(|(t, _)| t.contains(needle))
    }

    /// A click on the text `text`, where the last frame drew it, over the frames a click
    /// takes; then the driver settles.
    fn click_on(&mut self, text: &str) {
        let at = self.drawn_at(text);
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.frame_with(vec![egui::Event::PointerMoved(at)]);
        self.frame_with(vec![button(true)]);
        self.frame_with(vec![button(false)]);
        self.settle();
    }

    /// `key` pressed with `modifiers`, `times` times in one frame; then the modifiers let go.
    fn press(&mut self, key: egui::Key, modifiers: egui::Modifiers, times: usize) {
        let event = |pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        };
        let mut events = vec![egui::Event::ModifiersChanged(modifiers)];
        for _ in 0..times {
            events.extend([event(true), event(false)]);
        }
        self.frame_with(events);
        self.frame_with(vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE)]);
    }

    /// Ctrl+Z (⌘Z), `times` times in one frame, and the driver settled.
    fn undo(&mut self, times: usize) {
        self.press(egui::Key::Z, egui::Modifiers::COMMAND, times);
        self.settle();
    }

    /// Ctrl+Shift+Z (⌘⇧Z), and the driver settled.
    fn redo(&mut self) {
        let modifiers = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
        self.press(egui::Key::Z, modifiers, 1);
        self.settle();
    }

    /// The rename of `name` to `to`, applied and settled.
    fn rename(&mut self, name: &str, to: &str) {
        self.feed(ExplorerEvent::Rename {
            name: name.into(),
            to: to.into(),
        });
        self.settle();
    }

    /// The workspace's undo batches (those undoing another), newest first.
    fn undo_batches(&self) -> Vec<BatchRecords> {
        let history = pending_done(self.client.history(None)).unwrap();
        history
            .batches
            .iter()
            .filter(|b| b.summary.begin.undo_of.is_some())
            .map(|b| pending_done(self.client.status(&b.summary.begin.batch_id)).unwrap())
            .collect()
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

/// A frame of the driver's explorer on the left and its detail pane on the right, with
/// `events` as its input; the texts it drew, and where.
fn frame(
    driver: &mut ExplorerDriver,
    ctx: &egui::Context,
    said: &mut Vec<DriverEvent>,
    events: Vec<egui::Event>,
) -> Vec<(String, egui::Rect)> {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(900.0, 400.0),
        )),
        events,
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        egui::Panel::right("detail")
            .resizable(false)
            .default_size(300.0)
            .show(ui, |ui| {
                let _ = driver.detail_ui(ui);
            });
        egui::CentralPanel::default().show(ui, |ui| said.extend(driver.ui(ui)));
    });
    output.textures_delta.clear();
    let mut drawn = Vec::new();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut drawn);
    }
    drawn
}

pub(super) fn collect(shape: &egui::Shape, drawn: &mut Vec<(String, egui::Rect)>) {
    match shape {
        egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, drawn)),
        egui::Shape::Text(text) => {
            drawn.push((text.galley.text().to_owned(), text.visual_bounding_rect()));
        }
        _ => {}
    }
}

/// Frames until no request is in flight, for up to 5 s, and one more to draw what the last
/// answer brought; what that one drew.
fn settle(
    driver: &mut ExplorerDriver,
    ctx: &egui::Context,
    said: &mut Vec<DriverEvent>,
) -> Vec<(String, egui::Rect)> {
    let start = Instant::now();
    frame(driver, ctx, said, vec![]);
    while driver.is_busy() {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "still busy after 5 s"
        );
        std::thread::sleep(Duration::from_millis(5));
        frame(driver, ctx, said, vec![]);
    }
    frame(driver, ctx, said, vec![])
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

/// A transport that writes down each method called, in order, and passes it on: the
/// methods in `late` sent 300 ms after they are called (on a thread), and the first
/// `fs.version` failed as a transport error if `fail_first_version`.
pub(super) struct Wire {
    inner: Arc<LocalTransport>,
    pub(super) calls: Arc<Mutex<Vec<String>>>,
    late: &'static [&'static str],
    fail_first_version: AtomicBool,
}

impl Wire {
    pub(super) fn new(
        inner: LocalTransport,
        late: &'static [&'static str],
        fail_first_version: bool,
    ) -> Wire {
        Wire {
            inner: Arc::new(inner),
            calls: Arc::new(Mutex::new(Vec::new())),
            late,
            fail_first_version: AtomicBool::new(fail_first_version),
        }
    }
}

impl FsTransport for Wire {
    fn call(&self, method: &str, params: Value) -> Pending<Result<Value, FsError>> {
        self.calls.lock().unwrap().push(method.to_owned());
        if method == "fs.version" && self.fail_first_version.swap(false, Ordering::SeqCst) {
            return Pending::ready(Err(FsError::Transport("the line dropped".into())));
        }
        if !self.late.contains(&method) {
            return self.inner.call(method, params);
        }
        let (late, sender) = pending::channel(|| {});
        let (inner, method) = (Arc::clone(&self.inner), method.to_owned());
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            sender.send(Some(inner.call(&method, params).wait()));
        });
        late.map(|answer| answer.unwrap_or_else(|| Err(FsError::default())))
    }
}

/// A rig whose transport is a [`Wire`], and the methods it was called with.
fn wired(
    names: &[&str],
    late: &'static [&'static str],
    fail_first_version: bool,
) -> (Rig, Arc<Mutex<Vec<String>>>) {
    let mut calls = None;
    let rig = Rig::with(names, (), |t| {
        let wire = Wire::new(t, late, fail_first_version);
        calls = Some(Arc::clone(&wire.calls));
        FsClient::new(wire)
    });
    (rig, calls.expect("wired"))
}

fn calls_of(calls: &Mutex<Vec<String>>) -> Vec<String> {
    calls.lock().unwrap().clone()
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

#[test]
fn ctrl_z_undoes_the_last_change() {
    let mut rig = Rig::new(&["a.txt", "b/"]);
    rig.settle();
    rig.rename("a.txt", "c.txt");
    // A click on the entry gives the explorer the keyboard.
    rig.click_on("c.txt");
    assert!(rig.driver.explorer().is_focused(&rig.ctx));
    rig.undo(1);
    assert!(rig.dir.path().join("a.txt").exists());
    assert!(!rig.dir.path().join("c.txt").exists());
    assert_eq!(rig.names(), ["a.txt", "b"]);
    assert_eq!(
        rig.driver.notice(),
        Some("Undid 'Rename \u{201c}a.txt\u{201d}'")
    );
    assert_eq!(rig.undo_batches().len(), 1);
}

#[test]
fn ctrl_shift_z_redoes_it() {
    let mut rig = Rig::new(&["a.txt", "b/"]);
    rig.settle();
    rig.rename("a.txt", "c.txt");
    rig.click_on("c.txt");
    rig.undo(1);
    assert!(rig.dir.path().join("a.txt").exists());
    rig.redo();
    assert!(rig.dir.path().join("c.txt").exists());
    assert!(!rig.dir.path().join("a.txt").exists());
    assert_eq!(rig.names(), ["b", "c.txt"]);
    assert_eq!(
        rig.driver.notice(),
        Some("Redid 'Rename \u{201c}a.txt\u{201d}'")
    );
}

#[test]
fn two_quick_undos_undo_two_batches() {
    let mut rig = Rig::new(&["a.txt", "b.txt"]);
    rig.settle();
    rig.rename("a.txt", "c.txt");
    rig.rename("b.txt", "d.txt");
    rig.click_on("c.txt");
    // Both in one frame: the second is pressed while the first is still to be sent.
    rig.undo(2);
    assert!(rig.dir.path().join("a.txt").exists());
    assert!(rig.dir.path().join("b.txt").exists());
    assert!(!rig.dir.path().join("c.txt").exists());
    assert!(!rig.dir.path().join("d.txt").exists());
    assert_eq!(rig.names(), ["a.txt", "b.txt"]);
    // Newest first: b's rename was undone first, then a's.
    let labels: Vec<_> = rig
        .undo_batches()
        .iter()
        .map(|b| b.summary.begin.label.clone().unwrap())
        .collect();
    assert_eq!(
        labels,
        [
            "Rename \u{201c}a.txt\u{201d}",
            "Rename \u{201c}b.txt\u{201d}"
        ]
    );
    assert_eq!(
        rig.driver.notice(),
        Some("Undid 'Rename \u{201c}a.txt\u{201d}'")
    );
}

#[test]
fn ctrl_z_waits_for_the_change_in_flight() {
    let (mut rig, _calls) = wired(&["a.txt", "b.txt"], &["fs.apply"], false);
    rig.settle();
    rig.rename("a.txt", "c.txt");
    rig.click_on("c.txt");
    // Ctrl+Z pressed while the rename of b.txt is still to be answered.
    rig.feed(ExplorerEvent::Rename {
        name: "b.txt".into(),
        to: "d.txt".into(),
    });
    rig.press(egui::Key::Z, egui::Modifiers::COMMAND, 1);
    rig.settle();
    // That rename is the one undone; the one before stays.
    assert!(rig.dir.path().join("b.txt").exists());
    assert!(!rig.dir.path().join("d.txt").exists());
    assert!(rig.dir.path().join("c.txt").exists());
    assert_eq!(
        rig.driver.notice(),
        Some("Undid 'Rename \u{201c}b.txt\u{201d}'")
    );
}

#[test]
fn a_version_check_that_failed_is_asked_again_before_listing() {
    let (mut rig, calls) = wired(&["a.txt"], &[], true);
    rig.settle();
    let failed = rig.driver.explorer().failed().expect("failed");
    assert_eq!(failed, "the line dropped");
    assert_eq!(calls_of(&calls), ["fs.version"]);
    // Retry (or Refresh) asks the version again, and lists once it is confirmed.
    rig.feed(ExplorerEvent::Refresh);
    rig.settle();
    assert_eq!(calls_of(&calls), ["fs.version", "fs.version", "fs.list"]);
    assert_eq!(rig.names(), ["a.txt"]);
}

#[test]
fn nothing_is_sent_before_the_version_answers() {
    let (mut rig, calls) = wired(&["a.txt", "b/c.txt"], &["fs.version"], false);
    rig.frame();
    rig.feed(ExplorerEvent::Refresh);
    rig.navigate(&["b"]);
    // The explorer given the keyboard for the press (it has no entry to click yet).
    let focus = rig.driver.explorer().focus_id();
    rig.ctx.memory_mut(|m| m.request_focus(focus));
    rig.press(egui::Key::Z, egui::Modifiers::COMMAND, 1);
    rig.frame();
    assert_eq!(calls_of(&calls), ["fs.version"]);
    // Once it answers: the folder gone to is listed, and the press is sent.
    rig.settle();
    let calls = calls_of(&calls);
    assert_eq!(calls[..3], ["fs.version", "fs.list", "journal.undo"]);
    assert_eq!(rig.names(), ["c.txt"]);
    assert_eq!(rig.driver.notice(), Some("Nothing to undo"));
}

#[test]
fn ctrl_z_in_the_rename_field_sends_no_undo() {
    // The explorer not focused: nothing was clicked.
    let mut rig = Rig::new(&["a.txt", "b/"]);
    rig.settle();
    rig.rename("a.txt", "c.txt");
    assert!(!rig.driver.explorer().is_focused(&rig.ctx));
    rig.undo(1);
    assert!(rig.undo_batches().is_empty());
    assert!(rig.dir.path().join("c.txt").exists());

    // The rename field open, over the frames it takes to open and focus.
    rig.click_on("c.txt");
    rig.press(egui::Key::F2, egui::Modifiers::NONE, 1);
    rig.settle();
    rig.frame();
    assert!(rig.driver.explorer().is_renaming());
    rig.undo(1);
    assert!(rig.undo_batches().is_empty());
    assert!(rig.dir.path().join("c.txt").exists());
    assert_eq!(rig.driver.notice(), None);
}

#[test]
fn an_undo_that_skips_says_how_many() {
    let mut rig = Rig::new(&["a.txt", "b/"]);
    rig.settle();
    rig.rename("a.txt", "c.txt");
    // A new a.txt where the undo would put the old one back.
    std::fs::write(rig.dir.path().join("a.txt"), b"new").unwrap();
    rig.click_on("c.txt");
    rig.undo(1);
    let notice = rig.driver.notice().expect("a notice").to_owned();
    assert_eq!(notice, "Undid 'Rename \u{201c}a.txt\u{201d}' (1 skipped)");
    assert!(rig.dir.path().join("c.txt").exists());
    assert_eq!(std::fs::read(rig.dir.path().join("a.txt")).unwrap(), b"new");
    // Listed again: the new a.txt is there.
    assert_eq!(rig.names(), ["a.txt", "b", "c.txt"]);
    // Opened, the notice lists the skipped move with its reason.
    let reason = "Something is already there";
    assert!(!rig.shows(reason));
    rig.click_on(&notice);
    assert!(rig.shows(reason), "{:?}", rig.drawn);
    assert!(rig.shows("fs.move"), "{:?}", rig.drawn);
    assert!(rig.shows("c.txt \u{2192} a.txt"), "{:?}", rig.drawn);
    // And closed again by another click.
    rig.click_on(&notice);
    assert!(!rig.shows(reason));
}

#[test]
fn nothing_to_undo_is_said() {
    let mut rig = Rig::new(&["a.txt", "b/"]);
    rig.settle();
    rig.click_on("a.txt");
    rig.undo(1);
    assert_eq!(rig.driver.notice(), Some("Nothing to undo"));
    rig.redo();
    assert_eq!(rig.driver.notice(), Some("Nothing to redo"));
    assert_eq!(rig.names(), ["a.txt", "b"]);
}

#[cfg(feature = "text-preview")]
#[test]
fn a_picked_file_is_previewed_over_fs_read() {
    let mut rig = Rig::new(&["a.txt", "b/"]);
    std::fs::write(rig.dir.path().join("a.txt"), b"hello").unwrap();
    rig.settle();
    assert!(!rig.shows("hello"));
    rig.click_on("a.txt");
    let start = Instant::now();
    while !rig.shows("hello") {
        assert!(start.elapsed() < Duration::from_secs(10), "{:?}", rig.drawn);
        std::thread::sleep(Duration::from_millis(5));
        rig.frame();
    }
    // A folder picked: nothing is drawn for it.
    rig.click_on("b");
    rig.frame();
    assert!(!rig.shows("hello"));
}

pub(super) fn pending_done<T>(mut p: Pending<T>) -> T {
    let start = Instant::now();
    loop {
        if let Some(answer) = p.poll() {
            return answer;
        }
        assert!(start.elapsed() < Duration::from_secs(5), "no answer in 5 s");
        std::thread::sleep(Duration::from_millis(5));
    }
}
