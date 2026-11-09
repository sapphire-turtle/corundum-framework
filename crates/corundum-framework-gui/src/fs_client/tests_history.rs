use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use corundum_framework_fs::journal::Actor;
use corundum_framework_fs::{FsService, Workspace};
use corundum_framework_fs_api::{FsOp, HistoryBatch};

use super::tests_driver::{Wire, collect, pending_done};
use super::{FsClient, HistoryPage, LocalTransport};

/// A workspace on a tempdir, served on a runtime of its own, and a history page over it as
/// `Actor::person("p")`, drawn headless frame by frame.
struct Rig {
    dir: tempfile::TempDir,
    runtime: tokio::runtime::Runtime,
    service: FsService<()>,
    client: FsClient,
    page: HistoryPage,
    ctx: egui::Context,
    /// The texts the last frame drew, and where.
    drawn: Vec<(String, egui::Rect)>,
}

impl Rig {
    fn new() -> Rig {
        Rig::with(FsClient::new)
    }

    /// A rig whose page's client has the transport `wrap` makes.
    fn with(wrap: impl FnOnce(LocalTransport) -> FsClient) -> Rig {
        let dir = tempfile::tempdir().unwrap();
        let service = FsService::new(Workspace::open(dir.path()).unwrap(), ());
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .build()
            .unwrap();
        let client = wrap(transport_over(&service, &runtime, Actor::person("p")));
        let page = HistoryPage::new(client.clone()).time_format(|_| "14:02".to_owned());
        Rig {
            dir,
            runtime,
            service,
            client,
            page,
            ctx: egui::Context::default(),
            drawn: Vec::new(),
        }
    }

    /// Another client of the same workspace (the clones of a service share its journal),
    /// as `actor`, with an origin of its own.
    fn client_as(&self, actor: Actor) -> FsClient {
        client_over(&self.service, &self.runtime, actor)
    }

    /// The folder `name` made by `client`, as one batch labelled `label`.
    fn mkdir(&self, client: &FsClient, name: &str, label: &str) {
        let ops = vec![FsOp::Mkdir { path: name.into() }];
        pending_done(client.apply(ops, Some(label.into()))).unwrap();
    }

    fn frame(&mut self) {
        self.frame_with(vec![]);
    }

    /// A frame of the page filling a tall screen (so that 60 rows and Load more fit), with
    /// `events` as its input.
    fn frame_with(&mut self, events: Vec<egui::Event>) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 4000.0),
            )),
            events,
            ..Default::default()
        };
        let page = &mut self.page;
        let mut output = self.ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| page.ui(ui));
        });
        output.textures_delta.clear();
        self.drawn.clear();
        for clipped in &output.shapes {
            collect(&clipped.shape, &mut self.drawn);
        }
    }

    /// Frames until no request is in flight, for up to 5 s, and one more to draw what the
    /// last answer brought.
    fn settle(&mut self) {
        let start = Instant::now();
        self.frame();
        while self.page.is_busy() {
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "still busy after 5 s"
            );
            std::thread::sleep(Duration::from_millis(5));
            self.frame();
        }
        self.frame();
    }

    /// Where the last frame drew `text`: the first time, or with `last` the last.
    fn drawn_at(&self, text: &str, last: bool) -> egui::Rect {
        let mut found = self.drawn.iter().filter(|(t, _)| t == text);
        let found = if last {
            found.next_back()
        } else {
            found.next()
        };
        found
            .unwrap_or_else(|| panic!("{text:?} was not drawn: {:?}", self.drawn))
            .1
    }

    /// Whether the last frame drew a text containing `needle`.
    fn shows(&self, needle: &str) -> bool {
        self.drawn.iter().any(|(t, _)| t.contains(needle))
    }

    /// How many times the last frame drew exactly `text`.
    fn count(&self, text: &str) -> usize {
        self.drawn.iter().filter(|(t, _)| t == text).count()
    }

    /// A click on the first `text` drawn, over the frames a click takes; then the page
    /// settles.
    fn click_on(&mut self, text: &str) {
        self.click_at(self.drawn_at(text, false).center());
    }

    /// A click on the last `text` drawn: a modal's, drawn over the page.
    fn click_on_last(&mut self, text: &str) {
        self.click_at(self.drawn_at(text, true).center());
    }

    fn click_at(&mut self, at: egui::Pos2) {
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

    /// The workspace's batches as the page's client sees them, newest first.
    fn batches(&self) -> Vec<HistoryBatch> {
        pending_done(self.client.history(None)).unwrap().batches
    }
}

fn client_over(
    service: &FsService<()>,
    runtime: &tokio::runtime::Runtime,
    actor: Actor,
) -> FsClient {
    FsClient::new(transport_over(service, runtime, actor))
}

fn transport_over(
    service: &FsService<()>,
    runtime: &tokio::runtime::Runtime,
    actor: Actor,
) -> LocalTransport {
    LocalTransport::new(service.clone(), actor, runtime.handle().clone(), || {})
}

/// A rig over a [`Wire`], and the methods it was called with.
fn wired(
    late: &'static [&'static str],
    fail_first_version: bool,
) -> (Rig, Arc<Mutex<Vec<String>>>) {
    let mut calls = None;
    let rig = Rig::with(|t| {
        let wire = Wire::new(t, late, fail_first_version);
        calls = Some(Arc::clone(&wire.calls));
        FsClient::new(wire)
    });
    (rig, calls.expect("wired"))
}

fn calls_of(calls: &Mutex<Vec<String>>) -> Vec<String> {
    calls.lock().unwrap().clone()
}

const CONFIRM_AGENT: &str = "This change was made by the agent at 14:02. Undo it?";
const CONFIRM_OTHER: &str = "This change was made in another window at 14:02. Undo it?";

#[test]
fn rows_show_newest_first_with_their_counts() {
    let mut rig = Rig::new();
    rig.mkdir(&rig.client, "a", "First");
    rig.mkdir(&rig.client, "b", "Second");
    rig.frame();
    assert!(rig.page.is_busy(), "the first frame asks");
    rig.settle();
    let (first, second) = (rig.drawn_at("First", false), rig.drawn_at("Second", false));
    assert!(second.top() < first.top(), "{:?}", rig.drawn);
    assert_eq!(rig.count("1 done"), 2, "{:?}", rig.drawn);
    assert_eq!(rig.count("14:02"), 2);
    assert_eq!(rig.count("p"), 2);
    assert_eq!(rig.count("This session"), 2);
    assert!(!rig.shows("Agent"));
    assert!(!rig.shows("Undone"));
    // Expanded, a row lists its operations from journal.status.
    assert!(!rig.shows("fs.mkdir"));
    rig.click_on("Second");
    assert!(rig.shows("fs.mkdir"), "{:?}", rig.drawn);
    assert_eq!(rig.count("b"), 1, "{:?}", rig.drawn);
    assert_eq!(rig.count("Done"), 1, "{:?}", rig.drawn);
    // And a batch without a label is unnamed.
    pending_done(
        rig.client
            .apply(vec![FsOp::Mkdir { path: "c".into() }], None),
    )
    .unwrap();
    rig.page.refresh();
    rig.settle();
    assert!(rig.shows("Unnamed change"), "{:?}", rig.drawn);
}

#[test]
fn an_agents_batch_has_a_badge_and_asks_first() {
    let mut rig = Rig::new();
    let agent = rig.client_as(Actor::agent("p"));
    rig.mkdir(&agent, "a", "Tidy up");
    rig.settle();
    assert!(rig.shows("Agent"), "{:?}", rig.drawn);
    assert!(!rig.shows("This session"));
    rig.click_on("Undo");
    assert!(rig.shows(CONFIRM_AGENT), "{:?}", rig.drawn);
    assert!(rig.shows("by the agent at"));
    assert!(
        rig.dir.path().join("a").exists(),
        "not undone before confirmed"
    );
    // Cancelled, nothing is undone; confirmed, it is.
    rig.click_on_last("Cancel");
    assert!(!rig.shows(CONFIRM_AGENT));
    assert!(rig.dir.path().join("a").exists());
    rig.click_on("Undo");
    rig.click_on_last("Undo");
    assert!(!rig.shows(CONFIRM_AGENT));
    assert!(!rig.dir.path().join("a").exists());
    assert_eq!(rig.page.notice(), Some("Undid 'Tidy up'"));
    // Refreshed: the undo is the newest row, and the batch is marked undone.
    assert!(rig.shows("Undone"), "{:?}", rig.drawn);
    let batches = rig.batches();
    assert_eq!(batches.len(), 2);
    assert!(batches[1].summary.undone_by.is_some());
}

#[test]
fn another_windows_batch_asks_first() {
    let mut rig = Rig::new();
    let other = rig.client_as(Actor::person("p"));
    rig.mkdir(&other, "a", "Tidy up");
    rig.settle();
    assert!(!rig.shows("Agent"));
    rig.click_on("Undo");
    assert!(rig.shows(CONFIRM_OTHER), "{:?}", rig.drawn);
    rig.click_on_last("Undo");
    assert!(!rig.dir.path().join("a").exists());
}

#[test]
fn this_sessions_batch_undoes_without_asking() {
    let mut rig = Rig::new();
    rig.mkdir(&rig.client, "a", "Tidy up");
    rig.settle();
    rig.click_on("Undo");
    assert!(!rig.shows("Undo it?"), "{:?}", rig.drawn);
    assert!(!rig.dir.path().join("a").exists());
    assert_eq!(rig.page.notice(), Some("Undid 'Tidy up'"));
    assert!(rig.shows("Undone"), "{:?}", rig.drawn);
}

#[test]
fn undo_is_disabled_when_not_undoable() {
    let mut rig = Rig::new();
    let q = rig.client_as(Actor::person("q"));
    rig.mkdir(&q, "a", "Theirs");
    rig.settle();
    assert!(rig.shows("q"));
    assert!(!rig.batches()[0].undoable);
    rig.click_on("Undo");
    assert!(!rig.shows("Undo it?"), "{:?}", rig.drawn);
    assert!(rig.dir.path().join("a").exists());
    assert_eq!(rig.page.notice(), None);
}

#[test]
fn a_stale_undo_says_already_undone_and_refreshes() {
    let mut rig = Rig::new();
    rig.mkdir(&rig.client, "a", "Tidy up");
    rig.settle();
    // Undone elsewhere, the page not told.
    pending_done(rig.client.undo()).unwrap();
    rig.frame();
    assert!(!rig.shows("Undone"));
    rig.click_on("Undo");
    assert_eq!(rig.page.notice(), Some("This change was already undone"));
    assert!(rig.shows("Undone"), "{:?}", rig.drawn);
}

#[test]
fn load_more_fetches_the_next_page() {
    let mut rig = Rig::new();
    for i in 0..60 {
        rig.mkdir(&rig.client, &format!("d{i:02}"), &format!("Make d{i:02}"));
    }
    rig.settle();
    assert_eq!(rig.count("1 done"), 50, "{:?}", rig.drawn);
    assert!(rig.shows("Make d59"));
    assert!(!rig.shows("Make d09"));
    rig.click_on("Load more");
    assert_eq!(rig.count("1 done"), 60);
    assert!(rig.shows("Make d00"));
    assert!(!rig.shows("Load more"));
}

#[test]
fn a_version_check_that_failed_is_asked_again_before_loading() {
    let (mut rig, calls) = wired(&[], true);
    rig.mkdir(&rig.client, "a", "Tidy up");
    calls.lock().unwrap().clear();
    rig.settle();
    assert!(rig.shows("the line dropped"), "{:?}", rig.drawn);
    assert_eq!(calls_of(&calls), ["fs.version"]);
    // Refresh asks the version again, and loads once it is confirmed.
    rig.page.refresh();
    rig.settle();
    assert_eq!(
        calls_of(&calls),
        ["fs.version", "fs.version", "journal.history"]
    );
    assert!(rig.shows("Tidy up"), "{:?}", rig.drawn);
}

#[test]
fn nothing_is_loaded_before_the_version_answers() {
    let (mut rig, calls) = wired(&["fs.version"], false);
    rig.frame();
    rig.page.refresh();
    rig.frame();
    assert_eq!(calls_of(&calls), ["fs.version"]);
    rig.settle();
    assert_eq!(calls_of(&calls), ["fs.version", "journal.history"]);
}
