//! The Files screen: `explorer::FileExplorer` over a sample folder tree the demo makes in
//! its cache directory — moved and renamed for real, there alone — or, on a desktop, over
//! any folder picked, to look at only.
//!
//! The listing is read on a thread and handed in when it arrives, a page at a time, as an
//! app listing a server's folder would.

use std::path::{Path, PathBuf};
use std::sync::mpsc;

use corundum_framework::gui::explorer::{Entry, ExplorerEvent, FileExplorer, MenuItem};
use corundum_framework::gui::i18n::Message;
use corundum_framework::gui::{egui, icons, platform, tr};
use corundum_framework::platform::dirs::AppDirs;

/// Entries handed in at a time: the Many files folder takes a few pages.
const PAGE: usize = 60;
/// An SVG this small is read for its tile's thumbnail.
const THUMBNAIL_MAX: u64 = 512 * 1024;

/// A folder's entries, as read on a thread.
type Listing = std::io::Result<Vec<Entry>>;

/// What the screen keeps between frames.
pub(crate) struct State {
    explorer: FileExplorer,
    /// The folder the explorer's root is; `None` where the demo has no cache directory.
    root: Option<PathBuf>,
    /// Whether `root` is the sample, which the demo changes, or a folder picked.
    sample: bool,
    /// A listing being read: the location it is for, and where it arrives.
    reading: Option<(Vec<String>, mpsc::Receiver<Listing>)>,
    /// What has been read but not yet handed in, for the next page.
    rest: Vec<Entry>,
}

impl Default for State {
    fn default() -> State {
        State {
            explorer: sample_explorer(),
            root: sample_root(),
            sample: true,
            reading: None,
            rest: Vec::new(),
        }
    }
}

/// The explorer over the sample: moves and renames on, and the demo's menu items.
fn sample_explorer() -> FileExplorer {
    FileExplorer::new("files", "Sample")
        .movable(true)
        .renamable(true)
        .menu(menu)
}

/// The demo's items in an entry's menu: copy the names; on a desktop, show it in the file
/// manager.
fn menu(ctx: &egui::Context, _: &[&Entry]) -> Vec<MenuItem> {
    let mut items =
        vec![MenuItem::new("copy", tr!(ctx, "demo-files-copy")).icon(icons::clipboard_copy())];
    if platform::file_manager_available() {
        items.push(MenuItem::new("show", tr!(ctx, "demo-files-show")).icon(icons::folder()));
    }
    items
}

/// Where the sample is kept: under the cache directory, or a temporary one in the tests.
fn sample_root() -> Option<PathBuf> {
    if cfg!(test) {
        let dir = std::env::temp_dir().join(format!("corundum-demo-files-{}", std::process::id()));
        return Some(dir.join("sample"));
    }
    AppDirs::new(crate::APP_NAME).map(|dirs| dirs.cache().join("explorer-sample"))
}

pub(super) fn ui(ui: &mut egui::Ui, state: &mut State, log: &mut Message) {
    egui::CentralPanel::default().show(ui, |ui| {
        super::heading(ui, "demo-screen-files");
        let Some(root) = state.root.clone() else {
            ui.weak(tr!(ui, "demo-files-no-dir"));
            return;
        };
        if state.sample {
            ui.label(tr!(ui, "demo-files-intro"));
        } else {
            ui.label(tr!(
                ui,
                "demo-files-picked",
                path = root.display().to_string()
            ));
        }
        ui.horizontal_wrapped(|ui| {
            if state.sample {
                if ui.button(tr!(ui, "demo-files-reset")).clicked() {
                    let _ = std::fs::remove_dir_all(&root);
                    state.explorer.go_to(Vec::new());
                    state.read(ui.ctx(), Vec::new());
                    *log = Message::new("demo-status-files-reset");
                }
            } else if ui.button(tr!(ui, "demo-files-back")).clicked() {
                *state = State::default();
            }
            if platform::folder_picker_available()
                && ui.button(tr!(ui, "demo-files-pick")).clicked()
                && let Some(folder) = platform::pick_folder(&tr!(ui, "demo-files-pick"))
            {
                let name = folder.file_name().map_or_else(
                    || folder.display().to_string(),
                    |n| n.to_string_lossy().into(),
                );
                // Someone's own folder: looked at, never changed.
                *state = State {
                    explorer: FileExplorer::new(("files", &folder), name).menu(menu),
                    root: Some(folder),
                    sample: false,
                    reading: None,
                    rest: Vec::new(),
                };
            }
        });
        ui.add_space(8.0);

        if state.reading.is_none()
            && state.explorer.is_loading()
            && state.explorer.entries().is_empty()
        {
            let location = state.explorer.location().to_vec();
            state.read(ui.ctx(), location);
        }
        state.receive();
        for event in state.explorer.ui(ui) {
            state.act(ui.ctx(), event, log);
        }
    });
}

impl State {
    /// The folder at `location` under the root.
    fn path(&self, location: &[String]) -> Option<PathBuf> {
        let mut path = self.root.clone()?;
        path.extend(location);
        Some(path)
    }

    /// Start reading the folder at `location`, on a thread; the sample is made first if
    /// it is not there.
    fn read(&mut self, ctx: &egui::Context, location: Vec<String>) {
        let Some(path) = self.path(&location) else {
            return;
        };
        let make = self.sample.then(|| self.root.clone()).flatten();
        let (send, receive) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            if let Some(root) = make
                && !root.exists()
                && let Err(e) = make_sample(&root)
            {
                tracing::warn!("corundum demo: could not make the sample folder: {e}");
            }
            let _ = send.send(list(&path));
            ctx.request_repaint();
        });
        self.rest.clear();
        self.reading = Some((location, receive));
    }

    /// Hand in a listing that has arrived: its first page.
    fn receive(&mut self) {
        let Some((location, receive)) = &self.reading else {
            return;
        };
        let Ok(listed) = receive.try_recv() else {
            return;
        };
        let location = location.clone();
        self.reading = None;
        match listed {
            Ok(mut entries) => {
                self.rest = entries.split_off(entries.len().min(PAGE));
                let more = !self.rest.is_empty();
                self.explorer.set_entries(&location, entries, more);
            }
            Err(e) => {
                self.explorer.set_failed(&location, e.to_string());
            }
        }
    }

    fn act(&mut self, ctx: &egui::Context, event: ExplorerEvent, log: &mut Message) {
        let here = self.explorer.location().to_vec();
        match event {
            ExplorerEvent::Navigated(location) => self.read(ctx, location),
            ExplorerEvent::Refresh => self.read(ctx, here),
            ExplorerEvent::LoadMore => {
                let next: Vec<Entry> = self.rest.drain(..self.rest.len().min(PAGE)).collect();
                let more = !self.rest.is_empty();
                self.explorer.append_entries(&here, next, more);
            }
            ExplorerEvent::Opened(name) => {
                *log = Message::new("demo-status-opened").arg("name", name);
            }
            ExplorerEvent::Move { names, to } => {
                let (Some(from), Some(to_path)) = (self.path(&here), self.path(&to)) else {
                    return;
                };
                let moved = names
                    .iter()
                    .filter(|n| std::fs::rename(from.join(n), to_path.join(n)).is_ok())
                    .count();
                *log = Message::new("demo-status-moved")
                    .arg("count", moved)
                    .arg("folder", to.last().cloned().unwrap_or_default());
                self.read(ctx, here);
            }
            ExplorerEvent::Rename { name, to } => {
                let Some(dir) = self.path(&here) else {
                    return;
                };
                *log = match std::fs::rename(dir.join(&name), dir.join(&to)) {
                    Ok(()) => Message::new("demo-status-renamed")
                        .arg("name", name)
                        .arg("to", to),
                    Err(e) => Message::new("demo-status-failed").arg("error", e.to_string()),
                };
                self.read(ctx, here);
            }
            ExplorerEvent::Menu { id, names } => match id.as_str() {
                "copy" => ctx.copy_text(names.join("\n")),
                "show" => {
                    if let Some(dir) = self.path(&here) {
                        platform::open_in_file_manager(&dir);
                    }
                }
                _ => {}
            },
        }
    }
}

/// The entries of the folder at `path`: SVG pictures with their thumbnail.
fn list(path: &Path) -> std::io::Result<Vec<Entry>> {
    let mut entries = Vec::new();
    for item in std::fs::read_dir(path)? {
        let item = item?;
        let name = item.file_name().to_string_lossy().into_owned();
        let Ok(meta) = item.metadata() else {
            continue;
        };
        let mut entry = if meta.is_dir() {
            Entry::folder(name)
        } else {
            Entry::file(name).size(meta.len())
        };
        if let Ok(modified) = meta.modified() {
            entry = entry.modified(modified);
        }
        if !meta.is_dir()
            && entry.name.to_ascii_lowercase().ends_with(".svg")
            && meta.len() <= THUMBNAIL_MAX
            && let Ok(bytes) = std::fs::read(item.path())
        {
            entry = entry.thumbnail(egui::ImageSource::Bytes {
                uri: format!("bytes://{}", item.path().display()).into(),
                bytes: bytes.into(),
            });
        }
        entries.push(entry);
    }
    Ok(entries)
}

/// The sample: pictures, an album, documents, and a folder of many files to page through.
fn make_sample(root: &Path) -> std::io::Result<()> {
    let file = |path: PathBuf, bytes: &[u8]| -> std::io::Result<()> {
        std::fs::create_dir_all(path.parent().expect("under the root"))?;
        std::fs::write(path, bytes)
    };
    file(
        root.join("Pictures/corundum.svg"),
        include_bytes!("../../assets/corundum.svg"),
    )?;
    file(
        root.join("Pictures/app-icon.svg"),
        include_bytes!("../../assets/app-icon.svg"),
    )?;
    for n in 1..=12 {
        let song = format!("Music/First album/{n:02} Track {n}.mp3");
        file(root.join(song), &vec![0; n * 40_000])?;
    }
    file(root.join("Music/cover.png"), &[0; 1_200])?;
    file(
        root.join("Documents/notes.txt"),
        b"Drag me into another folder.\n",
    )?;
    file(root.join("Documents/Old/plan.md"), b"# Plan\n")?;
    for n in 1..=150 {
        file(
            root.join(format!("Many files/file {n}.txt")),
            n.to_string().as_bytes(),
        )?;
    }
    file(
        root.join("readme.txt"),
        b"The corundum demo's sample folder.\n",
    )
}
