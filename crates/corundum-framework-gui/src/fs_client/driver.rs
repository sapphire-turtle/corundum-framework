//! A [`FileExplorer`] showing a workspace, its changes applied over an [`FsClient`].

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use corundum_framework_fs_api::{
    BatchRecords, Entry as FsEntry, EntryKind as FsKind, Expect, FsOp, Listing, Managed,
};
use corundum_framework_platform::pending::Pending;

use super::{FsClient, FsError};
use crate::explorer::{self, ExplorerEvent, FileExplorer, MenuItem};
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

/// A [`FileExplorer`] over a workspace: it lists each folder the person goes to with
/// `fs.list`, a page at a time, and applies what they do — a rename, a drag into a folder,
/// Move to the trash from the menu — as one `fs.apply` batch each, each operation expecting
/// the entry as listed, so that a file changed since is skipped rather than overwritten.
/// After a change it lists the folder again; a skip or an error shows in a notice above the
/// explorer.
///
/// Hold one for the view and draw it with [`ui`](Self::ui) each frame; act on the
/// [`DriverEvent`]s it returns. Every call is polled, so the UI thread never waits.
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
    /// The one-line notice above the explorer.
    notice: Option<String>,
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
            notice: None,
        }
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
        self.poll(&ctx);
        self.notice_ui(ui);
        let said: Vec<DriverEvent> = self
            .explorer
            .ui(ui)
            .into_iter()
            .filter_map(|event| self.handle(&ctx, event))
            .collect();
        // An answer ready at once (after a version mismatch, say) is drawn next frame.
        if self.poll(&ctx) {
            ctx.request_repaint();
        }
        said
    }

    /// The one-line notice, with a button to close it.
    fn notice_ui(&mut self, ui: &mut egui::Ui) {
        let Some(text) = &self.notice else {
            return;
        };
        let mut close = false;
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
                    ui.add(egui::Label::new(text.as_str()).truncate());
                });
            },
        );
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
        came
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
                self.notice = Some(format!(
                    "{}{}",
                    tr!(ctx, "corundum-fs-done", label = label),
                    tr!(ctx, "corundum-fs-skipped", count = batch.summary.skipped)
                ));
            }
            Ok(_) => {}
            Err(e) => self.notice = Some(e.text(ctx)),
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
        self.version.is_some() || self.listing.is_some() || !self.changes.is_empty()
    }

    #[cfg(all(test, feature = "fs-local"))]
    pub(super) fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
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
