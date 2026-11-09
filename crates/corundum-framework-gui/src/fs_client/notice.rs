//! The one-line notice of the explorer driver and the history page, and the rows of a
//! batch's operations they list.

use corundum_framework_fs_api::{BatchRecords, OpRecord, Outcome, Skip};
use serde_json::Value;

use crate::{icons, tr};

/// A one-line notice, and the operations it can list.
pub(super) struct Notice {
    pub(super) text: String,
    /// The batch's operations that were skipped, listed when the notice is open.
    skipped: Vec<OpRecord>,
    open: bool,
}

impl Notice {
    /// A notice of `text` alone.
    pub(super) fn plain(text: String) -> Notice {
        Notice {
            text,
            skipped: Vec::new(),
            open: false,
        }
    }

    /// A notice of `text` about `batch`, listing its skipped operations.
    pub(super) fn of(text: String, batch: BatchRecords) -> Notice {
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

/// Draw `notice`, if any, with a button to close it; when something was skipped, a click
/// opens it to list what and why, in a grid told apart by `id`.
pub(super) fn notice_ui(ui: &mut egui::Ui, notice: &mut Option<Notice>, id: impl egui::AsIdSalt) {
    let Some(shown) = notice else {
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
                if shown.skipped.is_empty() {
                    ui.add(egui::Label::new(shown.text.as_str()).truncate());
                    return;
                }
                let (icon, hint) = if shown.open {
                    (icons::chevron_down(), "corundum-fs-hide-skipped")
                } else {
                    (icons::chevron_right(), "corundum-fs-show-skipped")
                };
                toggle |= ui
                    .add(icons::button(icon, 14.0))
                    .on_hover_text(tr!(ui, hint))
                    .clicked();
                let label = egui::Label::new(shown.text.as_str())
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
        shown.open = !shown.open;
    }
    if shown.open {
        ops_ui(ui, ("corundum_fs_skipped", id), &shown.skipped);
    }
    ui.separator();
    if close {
        *notice = None;
    }
}

/// The operations, one row each: its kind, the path it was for, and that it was done or
/// why it was skipped; in a grid of the id `id`.
pub(super) fn ops_ui(ui: &mut egui::Ui, id: impl egui::AsIdSalt, ops: &[OpRecord]) {
    egui::Grid::new(id)
        .num_columns(3)
        .striped(true)
        .show(ui, |ui| {
            for op in ops {
                ui.label(op.kind.as_str());
                ui.label(plan_path(ui.ctx(), &op.plan));
                ui.label(match &op.outcome {
                    Outcome::Done(_) => tr!(ui, "corundum-fs-op-done"),
                    Outcome::Skipped(skip) => reason_text(ui.ctx(), skip),
                });
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
