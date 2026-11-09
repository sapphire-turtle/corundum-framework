//! A [`FileExplorer`] showing a workspace, its changes applied over an [`FsClient`].

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, SystemTime};

use corundum_framework_fs_api::{
    BatchRecords, Entry as FsEntry, EntryKind as FsKind, Expect, FsOp, Listing, Managed, OpRecord,
    Outcome, Skip, codes,
};
use corundum_framework_platform::pending::Pending;
use serde_json::Value;

use super::{FsClient, FsError};
use crate::explorer::{self, ExplorerEvent, FileExplorer, MenuItem};
use crate::preview::{FileInfo, FilePreview, PreviewResponse, State};
use crate::{icons, tr};

/// The id of the entry's menu item that moves it to the trash.
const TRASH: &str = "trash";

/// What the app does to an explorer entry it manages, given what the server says of it.
type DecorateFn = dyn Fn(&mut explorer::Entry, &Managed);

/// What the person did that is the app's to act on.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DriverEvent {
    /// A file the app manages was opened: its item.
    OpenManaged {
        /// The app's id of the item ([`Managed::item_id`]).
        item_id: String,
    },
    /// A file the app does not manage was opened.
    Opened {
        /// Its path in the workspace.
        path: String,
    },
}

/// A listing asked for: of which folder, and whether it is a next page.
struct ListRequest {
    location: Vec<String>,
    append: bool,
    answer: Pending<Result<Listing, FsError>>,
}

/// A batch applied, and its label.
struct Change {
    label: String,
    answer: Pending<Result<BatchRecords, FsError>>,
}

/// A press of Ctrl+Z or Ctrl+Shift+Z.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Undo,
    Redo,
}

/// Where the undo or redo in flight is.
enum Stepping {
    /// Sent; its answer is awaited.
    Sent {
        step: Step,
        answer: Pending<Result<BatchRecords, FsError>>,
    },
    /// Done, as `records` say; the batch it undid or redid is asked for its label.
    Naming {
        step: Step,
        records: Box<BatchRecords>,
        answer: Pending<Result<BatchRecords, FsError>>,
    },
}

/// The one-line notice above the explorer, and the operations it can list.
struct Notice {
    text: String,
    /// The batch's operations that were skipped, listed when the notice is open.
    skipped: Vec<OpRecord>,
    open: bool,
}

impl Notice {
    /// A notice of `text` alone.
    fn plain(text: String) -> Notice {
        Notice {
            text,
            skipped: Vec::new(),
            open: false,
        }
    }

    /// A notice of `text` about `batch`, listing its skipped operations.
    fn of(text: String, batch: BatchRecords) -> Notice {
        let skipped = batch
            .ops
            .into_iter()
            .filter(|op| matches!(op.outcome, Outcome::Skipped(_)))
            .collect();
        Notice {
            text,
            skipped,
            open: false,
        }
    }
}

/// A [`FileExplorer`] over a workspace: it lists each folder the person goes to with
/// `fs.list`, a page at a time, and applies what they do — a rename, a drag into a folder,
/// Move to the trash from the menu — as one `fs.apply` batch each, each operation expecting
/// the entry as listed, so that a file changed since is skipped rather than overwritten.
/// After a change it lists the folder again; a skip or an error shows in a notice above the
/// explorer, which opens to list what was skipped and why.
///
/// With the explorer focused (and no name being edited), Ctrl+Z (⌘Z) undoes this driver's
/// latest change with `journal.undo`, and Ctrl+Shift+Z (⌘⇧Z) redoes it with `journal.redo`:
/// each press one step, sent one at a time in order. The notice says what was undone or
/// redone.
///
/// Hold one for the view and draw it with [`ui`](Self::ui) each frame; act on the
/// [`DriverEvent`]s it returns. [`detail_ui`](Self::detail_ui) draws a preview of the file
/// picked, read over `fs.read`, for the detail pane. Every call is polled, so the UI thread
/// never waits.
pub struct ExplorerDriver {
    client: FsClient,
    explorer: FileExplorer,
    decorate: Option<Box<DecorateFn>>,
    /// Whether the first frame asked the server's version.
    started: bool,
    version: Option<Pending<Result<(), FsError>>>,
    /// The listing in flight; a newer one replaces it, so a late answer is dropped.
    listing: Option<ListRequest>,
    /// The next page's cursor of the folder shown.
    cursor: Option<String>,
    /// The folder's entries as listed, by name, for each operation's `expect`.
    listed: HashMap<String, FsEntry>,
    changes: Vec<Change>,
    /// The presses of Ctrl+Z and Ctrl+Shift+Z not yet sent, in order.
    steps: VecDeque<Step>,
    /// The undo or redo in flight.
    stepping: Option<Stepping>,
    notice: Option<Notice>,
    preview: FilePreview,
    /// How many times the preview's Retry was pressed: a part of the content's key, so that
    /// each retry reads the file afresh.
    retries: u32,
}

impl ExplorerDriver {
    /// A driver of a new explorer at the workspace's root, named `root_label` in the trail:
    /// its entries movable and renamable, with Move to the trash in their menu.
    pub fn new(client: FsClient, root_label: impl Into<String>) -> ExplorerDriver {
        let explorer = FileExplorer::new(("corundum_fs_driver", client.origin()), root_label)
            .movable(true)
            .renamable(true)
            .menu(|ctx, _entries| {
                vec![MenuItem::new(TRASH, tr!(ctx, "corundum-fs-trash")).icon(icons::trash_2())]
            });
        ExplorerDriver {
            client,
            explorer,
            decorate: None,
            started: false,
            version: None,
            listing: None,
            cursor: None,
            listed: HashMap::new(),
            changes: Vec::new(),
            steps: VecDeque::new(),
            stepping: None,
            notice: None,
            preview: FilePreview::new(),
            retries: 0,
        }
    }

    /// Preview the file picked with `preview`: one made with the app's player, say.
    pub fn with_preview(mut self, preview: FilePreview) -> ExplorerDriver {
        self.preview = preview;
        self
    }

    /// Let the app dress each entry it manages — a photo's icon or thumbnail, say — as it
    /// is listed: `f` is given the explorer's entry and what the server says of it.
    pub fn decorate(
        mut self,
        f: impl Fn(&mut explorer::Entry, &Managed) + 'static,
    ) -> ExplorerDriver {
        self.decorate = Some(Box::new(f));
        self
    }

    /// The explorer driven: where it is, what is listed and picked.
    pub fn explorer(&self) -> &FileExplorer {
        &self.explorer
    }

    /// Draw the notice, if any, and the explorer filling the rest of `ui`; return what the
    /// app is to act on this frame.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> Vec<DriverEvent> {
        let ctx = ui.ctx().clone();
        if !self.started {
            self.started = true;
            self.version = Some(self.client.check_version());
        }
        let came = self.poll(&ctx);
        self.notice_ui(ui);
        let said: Vec<DriverEvent> = self
            .explorer
            .ui(ui)
            .into_iter()
            .filter_map(|event| self.handle(&ctx, event))
            .collect();
        self.take_steps(&ctx);
        // What came, or was asked, in this frame may have left something to do: a call to
        // send, or an answer ready at once (after a version mismatch, say) to draw.
        if self.poll(&ctx) || came {
            ctx.request_repaint();
        }
        said
    }

    /// Draw a preview of the file picked, for the detail pane: its content read over
    /// `fs.read`, its details as listed. Nothing is drawn unless one file is picked, and one
    /// the app manages is the app's to show. Retry reads the file again; act on the rest of
    /// the response as the app would.
    pub fn detail_ui(&mut self, ui: &mut egui::Ui) -> PreviewResponse {
        let picked = self.explorer.selected();
        let [entry] = picked.as_slice() else {
            return PreviewResponse::default();
        };
        let listed = self.listed.get(&entry.name);
        let Some(listed) = listed.filter(|l| !entry.is_folder() && l.managed.is_none()) else {
            return PreviewResponse::default();
        };
        let file = FileInfo::from(*entry);
        let path = path(self.explorer.location(), Some(&entry.name));
        let mut content = self.client.content(&path, listed);
        if self.retries > 0 {
            content.key = format!("{}\0{}", content.key, self.retries);
        }
        let response = self.preview.ui(ui, &file, State::Ready(&content), false);
        if response.retry {
            self.retries += 1;
        }
        response
    }

    /// The one-line notice, with a button to close it; when something was skipped, a click
    /// opens it to list what and why.
    fn notice_ui(&mut self, ui: &mut egui::Ui) {
        let Some(notice) = &mut self.notice else {
            return;
        };
        let mut close = false;
        let mut toggle = false;
        let size = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
        ui.allocate_ui_with_layout(
            size,
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                close = ui
                    .add(icons::button(icons::x(), 14.0))
                    .on_hover_text(tr!(ui, "corundum-fs-close"))
                    .clicked();
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    if notice.skipped.is_empty() {
                        ui.add(egui::Label::new(notice.text.as_str()).truncate());
                        return;
                    }
                    let (icon, hint) = if notice.open {
                        (icons::chevron_down(), "corundum-fs-hide-skipped")
                    } else {
                        (icons::chevron_right(), "corundum-fs-show-skipped")
                    };
                    toggle |= ui
                        .add(icons::button(icon, 14.0))
                        .on_hover_text(tr!(ui, hint))
                        .clicked();
                    let label = egui::Label::new(notice.text.as_str())
                        .truncate()
                        .sense(egui::Sense::click());
                    toggle |= ui
                        .add(label)
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked();
                });
            },
        );
        if toggle {
            notice.open = !notice.open;
        }
        if notice.open {
            skipped_ui(ui, self.client.origin(), &notice.skipped);
        }
        ui.separator();
        if close {
            self.notice = None;
        }
    }

    /// Take in the answers that came; `true` if any did.
    fn poll(&mut self, ctx: &egui::Context) -> bool {
        let mut came = false;
        if let Some(answer) = self.version.as_mut().and_then(Pending::poll) {
            self.version = None;
            came = true;
            match answer {
                Ok(()) => self.list(self.explorer.location().to_vec()),
                Err(e) => {
                    let location = self.explorer.location().to_vec();
                    self.explorer.set_failed(&location, e.text(ctx));
                }
            }
        }
        if let Some(answer) = self.listing.as_mut().and_then(|r| r.answer.poll()) {
            let request = self.listing.take().expect("polled above");
            came = true;
            self.listed(ctx, &request, answer);
        }
        let mut i = 0;
        while i < self.changes.len() {
            if let Some(answer) = self.changes[i].answer.poll() {
                let change = self.changes.remove(i);
                came = true;
                self.changed(ctx, &change.label, answer);
            } else {
                i += 1;
            }
        }
        came |= self.poll_step(ctx);
        self.send_step();
        came
    }

    /// Queue the presses of Ctrl+Z and Ctrl+Shift+Z in this frame's input, in order, taking
    /// them from it; only while the explorer is focused and no name is being edited, the
    /// keys being another's (a text field's own undo) otherwise.
    fn take_steps(&mut self, ctx: &egui::Context) {
        if !self.explorer.is_focused(ctx) || self.explorer.is_renaming() {
            return;
        }
        let steps = ctx.input_mut(|input| {
            let mut steps = Vec::new();
            input.events.retain(|event| match event {
                egui::Event::Key {
                    key: egui::Key::Z,
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.command && !modifiers.alt => {
                    steps.push(if modifiers.shift {
                        Step::Redo
                    } else {
                        Step::Undo
                    });
                    false
                }
                _ => true,
            });
            steps
        });
        self.steps.extend(steps);
    }

    /// Send the next undo or redo queued, unless one is in flight.
    fn send_step(&mut self) {
        if self.stepping.is_some() {
            return;
        }
        let Some(step) = self.steps.pop_front() else {
            return;
        };
        let answer = match step {
            Step::Undo => self.client.undo(),
            Step::Redo => self.client.redo(),
        };
        self.stepping = Some(Stepping::Sent { step, answer });
    }

    /// Take in the undo or redo's answer, or its target's label; `true` if either came. An
    /// answer lists the folder again; a label (or an error) sets the notice.
    fn poll_step(&mut self, ctx: &egui::Context) -> bool {
        let answer = match &mut self.stepping {
            Some(Stepping::Sent { answer, .. } | Stepping::Naming { answer, .. }) => answer.poll(),
            None => None,
        };
        let Some(answer) = answer else {
            return false;
        };
        match self.stepping.take().expect("polled above") {
            Stepping::Sent { step, .. } => {
                self.list(self.explorer.location().to_vec());
                match answer {
                    Ok(records) => {
                        let begin = &records.summary.begin;
                        match begin.undo_of.as_ref().or(begin.redo_of.as_ref()) {
                            Some(target) => {
                                let answer = self.client.status(target);
                                self.stepping = Some(Stepping::Naming {
                                    step,
                                    records: Box::new(records),
                                    answer,
                                });
                            }
                            None => self.stepped(ctx, step, records, None),
                        }
                    }
                    Err(e) => {
                        let text = match e.code() {
                            codes::NOTHING_TO_UNDO => tr!(ctx, "corundum-fs-nothing-to-undo"),
                            codes::NOTHING_TO_REDO => tr!(ctx, "corundum-fs-nothing-to-redo"),
                            _ => e.text(ctx),
                        };
                        self.notice = Some(Notice::plain(text));
                    }
                }
            }
            Stepping::Naming { step, records, .. } => {
                let label = answer.ok().and_then(|target| target.summary.begin.label);
                self.stepped(ctx, step, *records, label);
            }
        }
        true
    }

    /// Say that `step` was done, as `records` say, to the batch called `label`.
    fn stepped(
        &mut self,
        ctx: &egui::Context,
        step: Step,
        records: BatchRecords,
        label: Option<String>,
    ) {
        let label = label.unwrap_or_else(|| tr!(ctx, "corundum-fs-unnamed"));
        let id = match step {
            Step::Undo => "corundum-fs-undid",
            Step::Redo => "corundum-fs-redid",
        };
        let skipped = records.summary.skipped;
        let text = tr!(ctx, id, label = label, skipped = skipped);
        self.notice = Some(Notice::of(text, records));
    }

    /// Act on what the person did in the explorer.
    pub(crate) fn handle(
        &mut self,
        ctx: &egui::Context,
        event: ExplorerEvent,
    ) -> Option<DriverEvent> {
        match event {
            ExplorerEvent::Navigated(location) => self.list(location),
            ExplorerEvent::Refresh => self.list(self.explorer.location().to_vec()),
            ExplorerEvent::LoadMore => self.load_more(),
            ExplorerEvent::Opened(name) => return Some(self.opened(&name)),
            ExplorerEvent::Rename { name, to } => {
                let to = path(self.explorer.location(), Some(&to));
                let ops = vec![self.move_op(&name, to)];
                let label = tr!(ctx, "corundum-fs-label-rename", name = name.as_str());
                self.apply(ops, label);
            }
            ExplorerEvent::Move { names, to } => {
                let ops = names
                    .iter()
                    .map(|name| self.move_op(name, path(&to, Some(name))))
                    .collect();
                let label = tr!(ctx, "corundum-fs-label-move", count = names.len());
                self.apply(ops, label);
            }
            ExplorerEvent::Menu { id, names } if id == TRASH => {
                let ops = names
                    .iter()
                    .map(|name| FsOp::Trash {
                        path: path(self.explorer.location(), Some(name)),
                        expect: self.expect(name),
                    })
                    .collect();
                let label = tr!(ctx, "corundum-fs-label-trash", count = names.len());
                self.apply(ops, label);
            }
            ExplorerEvent::Menu { .. } => {}
        }
        None
    }

    /// Ask for the first page of the folder at `location`, in place of any listing in
    /// flight.
    fn list(&mut self, location: Vec<String>) {
        let answer = self.client.list(&path(&location, None), None);
        self.listing = Some(ListRequest {
            location,
            append: false,
            answer,
        });
    }

    /// Ask for the next page of the folder shown, unless a listing is in flight.
    fn load_more(&mut self) {
        if self.listing.is_some() {
            return;
        }
        let Some(cursor) = self.cursor.clone() else {
            return;
        };
        let location = self.explorer.location().to_vec();
        let answer = self.client.list(&path(&location, None), Some(cursor));
        self.listing = Some(ListRequest {
            location,
            append: true,
            answer,
        });
    }

    /// Hand a listing's answer to the explorer, for the folder it was asked for.
    fn listed(
        &mut self,
        ctx: &egui::Context,
        request: &ListRequest,
        answer: Result<Listing, FsError>,
    ) {
        let listing = match answer {
            Ok(listing) => listing,
            Err(e) => {
                self.explorer.set_failed(&request.location, e.text(ctx));
                return;
            }
        };
        let more = listing.next_cursor.is_some();
        let entries = listing.entries.iter().map(|e| self.entry(e)).collect();
        let shown = if request.append {
            self.explorer
                .append_entries(&request.location, entries, more)
        } else {
            self.explorer.set_entries(&request.location, entries, more)
        };
        if !shown {
            return;
        }
        if !request.append {
            self.listed.clear();
        }
        self.cursor = listing.next_cursor;
        self.listed
            .extend(listing.entries.into_iter().map(|e| (e.name.clone(), e)));
    }

    /// The explorer's entry for `listed`, dressed by the app if it manages it.
    fn entry(&self, listed: &FsEntry) -> explorer::Entry {
        let mut entry = match listed.kind {
            FsKind::Dir => explorer::Entry::folder(&listed.name),
            FsKind::File | FsKind::Symlink => explorer::Entry::file(&listed.name).size(listed.size),
        };
        if let Some(at) = listed.modified.and_then(time_of) {
            entry = entry.modified(at);
        }
        if let (Some(managed), Some(decorate)) = (&listed.managed, &self.decorate) {
            decorate(&mut entry, managed);
        }
        entry
    }

    /// What opening the file `name` of the folder shown is for the app.
    fn opened(&self, name: &str) -> DriverEvent {
        match self.listed.get(name).and_then(|e| e.managed.as_ref()) {
            Some(managed) => DriverEvent::OpenManaged {
                item_id: managed.item_id.clone(),
            },
            None => DriverEvent::Opened {
                path: path(self.explorer.location(), Some(name)),
            },
        }
    }

    /// Moving `name` of the folder shown to the path `to`.
    fn move_op(&self, name: &str, to: String) -> FsOp {
        FsOp::Move {
            from: path(self.explorer.location(), Some(name)),
            to,
            expect: self.expect(name),
        }
    }

    /// How the entry `name` was listed, for an operation to expect.
    fn expect(&self, name: &str) -> Option<Expect> {
        self.listed.get(name).map(|e| Expect {
            size: e.size,
            modified: e.modified,
            hash: None,
        })
    }

    fn apply(&mut self, ops: Vec<FsOp>, label: String) {
        let answer = self.client.apply(ops, Some(label.clone()));
        self.changes.push(Change { label, answer });
    }

    /// Say what came of the change `label` if not all of it was done, and list the folder
    /// again.
    fn changed(&mut self, ctx: &egui::Context, label: &str, answer: Result<BatchRecords, FsError>) {
        match answer {
            Ok(batch) if batch.summary.skipped > 0 => {
                let skipped = batch.summary.skipped;
                let text = tr!(ctx, "corundum-fs-done", label = label, skipped = skipped);
                self.notice = Some(Notice::of(text, batch));
            }
            Ok(_) => {}
            Err(e) => self.notice = Some(Notice::plain(e.text(ctx))),
        }
        self.list(self.explorer.location().to_vec());
    }

    /// The explorer, to go somewhere as the person would.
    #[cfg(all(test, feature = "fs-local"))]
    pub(super) fn explorer_mut(&mut self) -> &mut FileExplorer {
        &mut self.explorer
    }

    /// Whether any call is still in flight.
    #[cfg(all(test, feature = "fs-local"))]
    pub(super) fn is_busy(&self) -> bool {
        self.version.is_some()
            || self.listing.is_some()
            || !self.changes.is_empty()
            || self.stepping.is_some()
            || !self.steps.is_empty()
    }

    #[cfg(all(test, feature = "fs-local"))]
    pub(super) fn notice(&self) -> Option<&str> {
        self.notice.as_ref().map(|n| n.text.as_str())
    }
}

/// The skipped operations, one row each: its kind, the path it was for, and why; in a grid
/// told apart by the client's `origin`.
fn skipped_ui(ui: &mut egui::Ui, origin: &str, skipped: &[OpRecord]) {
    egui::Grid::new(("corundum_fs_skipped", origin))
        .num_columns(3)
        .striped(true)
        .show(ui, |ui| {
            for op in skipped {
                ui.label(op.kind.as_str());
                ui.label(plan_path(ui.ctx(), &op.plan));
                if let Outcome::Skipped(skip) = &op.outcome {
                    ui.label(reason_text(ui.ctx(), skip));
                }
                ui.end_row();
            }
        });
}

/// The path an operation's plan is for: its `path`, or where it moves `from` and `to`.
fn plan_path(ctx: &egui::Context, plan: &Value) -> String {
    let field = |name: &str| plan.get(name).and_then(Value::as_str);
    match (field("path"), field("from"), field("to")) {
        (Some(path), _, _) => path.to_owned(),
        (None, Some(from), Some(to)) => tr!(ctx, "corundum-fs-plan-move", from = from, to = to),
        (None, from, to) => from.or(to).unwrap_or_default().to_owned(),
    }
}

/// Why an operation was skipped, for a person: the spec's reasons in words, another as the
/// server named it; with the server's message, if any.
fn reason_text(ctx: &egui::Context, skip: &Skip) -> String {
    let id = match skip.reason.as_str() {
        "precondition_failed" => Some("corundum-fs-reason-precondition-failed"),
        "exists" => Some("corundum-fs-reason-exists"),
        "not_found" => Some("corundum-fs-reason-not-found"),
        "invalid_path" => Some("corundum-fs-reason-invalid-path"),
        "denied" => Some("corundum-fs-reason-denied"),
        "io" => Some("corundum-fs-reason-io"),
        "unknown_kind" => Some("corundum-fs-reason-unknown-kind"),
        "not_undoable" => Some("corundum-fs-reason-not-undoable"),
        _ => None,
    };
    let reason = id.map_or_else(|| skip.reason.clone(), |id| tr!(ctx, id));
    match &skip.message {
        Some(message) => tr!(
            ctx,
            "corundum-fs-reason-with-message",
            reason = reason,
            message = message.as_str()
        ),
        None => reason,
    }
}

/// The workspace path of `name` in the folder at `location` (of the folder itself, without
/// a name): its names joined by `/`, `""` for the root.
fn path(location: &[String], name: Option<&str>) -> String {
    let mut parts: Vec<&str> = location.iter().map(String::as_str).collect();
    parts.extend(name);
    parts.join("/")
}

/// `ms` milliseconds from the Unix epoch, if a `SystemTime` can hold it.
fn time_of(ms: i64) -> Option<SystemTime> {
    let offset = Duration::from_millis(ms.unsigned_abs());
    if ms >= 0 {
        SystemTime::UNIX_EPOCH.checked_add(offset)
    } else {
        SystemTime::UNIX_EPOCH.checked_sub(offset)
    }
}
