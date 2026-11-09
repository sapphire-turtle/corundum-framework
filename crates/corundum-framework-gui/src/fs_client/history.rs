//! A page of the workspace's history, a batch a row, each undone with a click.

use std::collections::{HashMap, HashSet};
use std::time::SystemTime;

use corundum_framework_fs_api::{
    BatchRecords, BatchSummary, History, HistoryBatch, OpRecord, codes,
};
use corundum_framework_platform::pending::Pending;

use super::driver::time_of;
use super::notice::{Notice, notice_ui, ops_ui};
use super::{FsClient, FsError};
use crate::{explorer, icons, tr};

/// A page of the history asked for, and whether it is a next page.
struct Load {
    append: bool,
    answer: Pending<Result<History, FsError>>,
}

/// A batch's operations, from `journal.status`.
enum Ops {
    Loading(Pending<Result<BatchRecords, FsError>>),
    Loaded(Vec<OpRecord>),
    Failed(String),
}

/// An undo sent, and the label of the batch it undoes.
struct Undo {
    label: Option<String>,
    answer: Pending<Result<BatchRecords, FsError>>,
}

/// What the person did to a row in this frame.
enum RowAction {
    Toggle(String),
    Undo(usize),
}

/// The workspace's history over an [`FsClient`]: its batches, newest first, a page at a
/// time from `journal.history`, with Load more for the next. Each row says what the batch
/// was (its label), when, who made it (with a badge when an agent acted for them), whether
/// it was this session's, how many of its operations were done and skipped, and whether it
/// was undone; opened, it lists its operations from `journal.status`.
///
/// Undo, enabled when the server says the batch may be undone by this client's actor,
/// undoes it with `journal.undo_batch`; a batch of another origin (another window, an
/// agent) asks first. A notice says what came of it, and the page is loaded again.
///
/// Hold one for the screen and draw it with [`ui`](Self::ui) each frame. Every call is
/// polled, so the UI thread never waits.
pub struct HistoryPage {
    client: FsClient,
    time: Box<dyn Fn(SystemTime) -> String>,
    /// Whether the first frame asked the server's version.
    started: bool,
    version: Option<Pending<Result<(), FsError>>>,
    /// The page in flight; a newer one replaces it.
    loading: Option<Load>,
    batches: Vec<HistoryBatch>,
    /// The next page's cursor.
    cursor: Option<String>,
    /// Why the history could not be loaded.
    failed: Option<String>,
    /// The batches opened, by id.
    open: HashSet<String>,
    /// The operations of the batches opened, by id.
    ops: HashMap<String, Ops>,
    /// The batch of another origin whose undo is being asked about.
    confirm: Option<BatchSummary>,
    undos: Vec<Undo>,
    notice: Option<Notice>,
}

impl HistoryPage {
    /// The history of `client`'s workspace, its undos made as `client`.
    pub fn new(client: FsClient) -> HistoryPage {
        HistoryPage {
            client,
            time: Box::new(explorer::format_time),
            started: false,
            version: None,
            loading: None,
            batches: Vec::new(),
            cursor: None,
            failed: None,
            open: HashSet::new(),
            ops: HashMap::new(),
            confirm: None,
            undos: Vec::new(),
            notice: None,
        }
    }

    /// Write each batch's time with `f`, in the rows and the question before an undo; by
    /// default [`explorer::format_time`].
    pub fn time_format(mut self, f: impl Fn(SystemTime) -> String + 'static) -> HistoryPage {
        self.time = Box::new(f);
        self
    }

    /// Load the history's first page again, in place of any page in flight.
    pub fn refresh(&mut self) {
        // Before the first frame, that frame loads it, after the version.
        if self.started && self.version.is_none() {
            self.loading = Some(Load {
                append: false,
                answer: self.client.history(None),
            });
        }
    }

    /// Draw the page filling `ui`: a bar with Refresh, the notice, the rows, and the
    /// question before an undo.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if !self.started {
            self.started = true;
            self.version = Some(self.client.check_version());
        }
        let came = self.poll(&ctx);
        ui.horizontal(|ui| {
            let refresh = ui
                .add(icons::button(icons::refresh_cw(), 14.0))
                .on_hover_text(tr!(ui, "corundum-fs-refresh"));
            if refresh.clicked() {
                self.refresh();
            }
            if self.loading.is_some() || self.version.is_some() {
                ui.add(egui::Spinner::new());
            }
        });
        notice_ui(
            ui,
            &mut self.notice,
            ("corundum_fs_history", self.client.origin()),
        );
        if let Some(failed) = &self.failed {
            ui.label(failed.as_str());
        } else if self.batches.is_empty() && self.loading.is_none() && self.version.is_none() {
            ui.weak(tr!(ui, "corundum-fs-history-empty"));
        }
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| self.rows_ui(ui));
        self.confirm_ui(ui);
        // What came, or was asked, in this frame may have left something to do: a call to
        // send, or an answer ready at once (after a version mismatch, say) to draw.
        if self.poll(&ctx) || came {
            ctx.request_repaint();
        }
    }

    /// The rows, each opened one with its operations, and Load more after them.
    fn rows_ui(&mut self, ui: &mut egui::Ui) {
        let mut actions = Vec::new();
        for (i, batch) in self.batches.iter().enumerate() {
            let id = &batch.summary.begin.batch_id;
            let open = self.open.contains(id);
            actions.extend(self.row_ui(ui, i, batch, open));
            if open {
                ui.indent(("corundum_fs_history_indent", id), |ui| {
                    match self.ops.get(id) {
                        Some(Ops::Loaded(ops)) => ops_ui(ui, ("corundum_fs_history_ops", id), ops),
                        Some(Ops::Failed(text)) => {
                            ui.label(text.as_str());
                        }
                        Some(Ops::Loading(_)) | None => {
                            ui.add(egui::Spinner::new());
                        }
                    }
                });
            }
            ui.separator();
        }
        if self.cursor.is_some() {
            let more = egui::Button::new(tr!(ui, "corundum-fs-load-more"));
            if ui.add_enabled(self.loading.is_none(), more).clicked() {
                self.load_more();
            }
        }
        for action in actions {
            match action {
                RowAction::Toggle(id) => {
                    if !self.open.remove(&id) {
                        self.open.insert(id);
                    }
                }
                RowAction::Undo(i) => {
                    let summary = self.batches[i].summary.clone();
                    if summary.begin.origin == self.client.origin() {
                        self.undo(&summary);
                    } else {
                        self.confirm = Some(summary);
                    }
                }
            }
        }
    }

    /// The row of `batch`, the `i`th, `open` or not; what the person did to it.
    fn row_ui(
        &self,
        ui: &mut egui::Ui,
        i: usize,
        batch: &HistoryBatch,
        open: bool,
    ) -> Option<RowAction> {
        let summary = &batch.summary;
        let begin = &summary.begin;
        let mut action = None;
        ui.horizontal(|ui| {
            let (icon, hint) = if open {
                (icons::chevron_down(), "corundum-fs-hide-ops")
            } else {
                (icons::chevron_right(), "corundum-fs-show-ops")
            };
            let mut toggle = ui
                .add(icons::button(icon, 14.0))
                .on_hover_text(tr!(ui, hint))
                .clicked();
            let label = begin
                .label
                .clone()
                .unwrap_or_else(|| tr!(ui, "corundum-fs-unnamed"));
            let label =
                egui::Label::new(egui::RichText::new(label).strong()).sense(egui::Sense::click());
            toggle |= ui
                .add(label)
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked();
            if toggle {
                action = Some(RowAction::Toggle(begin.batch_id.clone()));
            }
            ui.weak(self.time_of(begin.at));
            ui.label(begin.actor.principal.as_str());
            if begin.actor.agent {
                badge(ui, tr!(ui, "corundum-fs-agent"));
            }
            if begin.origin == self.client.origin() {
                badge(ui, tr!(ui, "corundum-fs-this-session"));
            }
            let counts = tr!(
                ui,
                "corundum-fs-counts",
                done = summary.done,
                skipped = summary.skipped
            );
            ui.weak(counts);
            if summary.undone_by.is_some() {
                badge(ui, tr!(ui, "corundum-fs-undone"));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let undo = egui::Button::new(tr!(ui, "corundum-fs-undo"));
                if ui.add_enabled(batch.undoable, undo).clicked() {
                    action = Some(RowAction::Undo(i));
                }
            });
        });
        action
    }

    /// The question before undoing a batch of another origin: Undo or Cancel.
    fn confirm_ui(&mut self, ui: &mut egui::Ui) {
        let Some(summary) = &self.confirm else {
            return;
        };
        let begin = &summary.begin;
        let id = if begin.actor.agent {
            "corundum-fs-confirm-agent"
        } else {
            "corundum-fs-confirm-other"
        };
        let text = tr!(ui, id, time = self.time_of(begin.at));
        let mut answer = None;
        let modal = egui::Modal::new(egui::Id::unique((
            "corundum_fs_history_confirm",
            self.client.origin(),
        )))
        .show(ui.ctx(), |ui| {
            ui.label(text);
            ui.horizontal(|ui| {
                if ui.button(tr!(ui, "corundum-fs-undo")).clicked() {
                    answer = Some(true);
                }
                if ui.button(tr!(ui, "corundum-fs-cancel")).clicked() {
                    answer = Some(false);
                }
            });
        });
        if modal.should_close() && answer.is_none() {
            answer = Some(false);
        }
        if let Some(yes) = answer {
            let summary = self.confirm.take().expect("checked above");
            if yes {
                self.undo(&summary);
            }
        }
    }

    /// The batch's time, in the page's format.
    fn time_of(&self, at: i64) -> String {
        (self.time)(time_of(at).unwrap_or(SystemTime::UNIX_EPOCH))
    }

    /// Undo the batch of `summary`. Its Undo is disabled until the page is loaded again,
    /// so that it is not sent twice.
    fn undo(&mut self, summary: &BatchSummary) {
        let id = &summary.begin.batch_id;
        let answer = self.client.undo_batch(id);
        for batch in &mut self.batches {
            if &batch.summary.begin.batch_id == id {
                batch.undoable = false;
            }
        }
        self.undos.push(Undo {
            label: summary.begin.label.clone(),
            answer,
        });
    }

    /// Ask for the next page, unless a page is in flight.
    fn load_more(&mut self) {
        if self.loading.is_some() {
            return;
        }
        if let Some(cursor) = self.cursor.clone() {
            self.loading = Some(Load {
                append: true,
                answer: self.client.history(Some(cursor)),
            });
        }
    }

    /// Take in the answers that came, and ask for the operations of the batches opened
    /// that have none; `true` if any answer came.
    fn poll(&mut self, ctx: &egui::Context) -> bool {
        let mut came = false;
        if let Some(answer) = self.version.as_mut().and_then(Pending::poll) {
            self.version = None;
            came = true;
            match answer {
                Ok(()) => self.refresh(),
                Err(e) => self.failed = Some(e.text(ctx)),
            }
        }
        if let Some(answer) = self.loading.as_mut().and_then(|l| l.answer.poll()) {
            let load = self.loading.take().expect("polled above");
            came = true;
            self.loaded(ctx, load.append, answer);
        }
        for ops in self.ops.values_mut() {
            let Ops::Loading(answer) = ops else {
                continue;
            };
            if let Some(answer) = answer.poll() {
                came = true;
                *ops = match answer {
                    Ok(records) => Ops::Loaded(records.ops),
                    Err(e) => Ops::Failed(e.text(ctx)),
                };
            }
        }
        let mut i = 0;
        while i < self.undos.len() {
            if let Some(answer) = self.undos[i].answer.poll() {
                let undo = self.undos.remove(i);
                came = true;
                self.undone(ctx, undo.label, answer);
            } else {
                i += 1;
            }
        }
        for batch in &self.batches {
            let id = &batch.summary.begin.batch_id;
            if self.open.contains(id) && !self.ops.contains_key(id) {
                self.ops
                    .insert(id.clone(), Ops::Loading(self.client.status(id)));
            }
        }
        came
    }

    /// Show a page of the history: in place of the rows, or after them.
    fn loaded(&mut self, ctx: &egui::Context, append: bool, answer: Result<History, FsError>) {
        let history = match answer {
            Ok(history) => history,
            Err(e) => {
                self.failed = Some(e.text(ctx));
                return;
            }
        };
        self.failed = None;
        if !append {
            self.batches.clear();
            // The batches may have changed since (undone, say): their operations are asked
            // for again when shown.
            self.ops.clear();
        }
        self.batches.extend(history.batches);
        self.cursor = history.next_cursor;
    }

    /// Say what came of the undo of the batch `label`, and load the page again.
    fn undone(
        &mut self,
        ctx: &egui::Context,
        label: Option<String>,
        answer: Result<BatchRecords, FsError>,
    ) {
        self.notice = Some(match answer {
            Ok(records) => {
                let label = label.unwrap_or_else(|| tr!(ctx, "corundum-fs-unnamed"));
                let skipped = records.summary.skipped;
                let text = tr!(ctx, "corundum-fs-undid", label = label, skipped = skipped);
                Notice::of(text, records)
            }
            Err(e) if e.code() == codes::ALREADY_UNDONE => {
                Notice::plain(tr!(ctx, "corundum-fs-already-undone"))
            }
            Err(e) => Notice::plain(e.text(ctx)),
        });
        self.refresh();
    }

    /// Whether any call is still in flight.
    #[cfg(all(test, feature = "fs-local"))]
    pub(super) fn is_busy(&self) -> bool {
        self.version.is_some()
            || self.loading.is_some()
            || !self.undos.is_empty()
            || self.ops.values().any(|ops| matches!(ops, Ops::Loading(_)))
    }

    #[cfg(all(test, feature = "fs-local"))]
    pub(super) fn notice(&self) -> Option<&str> {
        self.notice.as_ref().map(|n| n.text.as_str())
    }
}

/// `text` small, on a tinted ground.
fn badge(ui: &mut egui::Ui, text: String) {
    egui::Frame::new()
        .fill(ui.visuals().widgets.inactive.bg_fill)
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(4, 0))
        .show(ui, |ui| ui.label(egui::RichText::new(text).small()));
}
