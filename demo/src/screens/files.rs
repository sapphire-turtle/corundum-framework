//! The Files screen: `explorer::FileExplorer` over a sample folder tree the demo makes in
//! its cache directory — moved and renamed for real, there alone — or, on a desktop, over
//! any folder picked, to look at only.
//!
//! The listing is read on a thread and handed in when it arrives, a page at a time, as an
//! app listing a server's folder would.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::time::UNIX_EPOCH;

use corundum_framework::audio::{CpalOutputs, Player};
use corundum_framework::gui::detail::DetailPane;
use corundum_framework::gui::explorer::{Entry, EntryKind, ExplorerEvent, FileExplorer, MenuItem};
use corundum_framework::gui::i18n::Message;
use corundum_framework::gui::preview::{
    Content, ContentOpener, FileInfo, FilePreview, InfoCard, Kind, PreviewResponse,
    State as PreviewState, kind_of,
};
use corundum_framework::gui::{egui, icons, platform, tr};
use corundum_framework::platform::dirs::AppDirs;

/// Entries handed in at a time: the Many files folder takes a few pages.
const PAGE: usize = 60;
/// An SVG this small is read for its tile's thumbnail.
const THUMBNAIL_MAX: u64 = 512 * 1024;

/// The most of a text file the demo's own view shows.
const TEXT_MAX: u64 = 4096;

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
    /// The detail pane and what it shows; taken out while the screen is drawn.
    detail: Option<(DetailPane, Detail)>,
}

impl Default for State {
    fn default() -> State {
        State {
            explorer: sample_explorer(),
            root: sample_root(),
            sample: true,
            reading: None,
            rest: Vec::new(),
            detail: None,
        }
    }
}

/// The one file the explorer has picked: what the detail shows.
struct Selected {
    info: FileInfo,
    path: PathBuf,
    /// The app's key for its content: it changes when the file does.
    key: String,
}

/// The detail pane, and the previews it draws.
struct Detail {
    preview: FilePreview,
    /// The player for sounds, spawned when a sound is first picked.
    player: Option<Arc<Player<Content>>>,
    /// Whether spawning the player has been tried: it is, once.
    player_tried: bool,
    /// The file shown in the last frame, to open the pane when it changes.
    last: Option<String>,
    /// The demo's own view of a text file: its key, and its text or why there is none.
    text: Option<(String, Result<String, String>)>,
}

impl Default for Detail {
    fn default() -> Detail {
        Detail {
            preview: FilePreview::new(),
            player: None,
            player_tried: false,
            last: None,
            text: None,
        }
    }
}

impl Detail {
    /// Spawn the player, once, the first time a sound is picked.
    fn ensure_player(&mut self, ctx: &egui::Context) {
        if self.player_tried {
            return;
        }
        self.player_tried = true;
        let repaint = ctx.clone();
        match Player::spawn(Box::new(ContentOpener), Box::new(CpalOutputs), move || {
            repaint.request_repaint()
        }) {
            Ok(player) => {
                let player = Arc::new(player);
                self.preview = std::mem::take(&mut self.preview).with_player(player.clone());
                self.player = Some(player);
            }
            Err(e) => tracing::warn!("corundum demo: could not start the player: {e}"),
        }
    }

    /// Draw the picked file, or a word that none is. `album` is whether it is in the sample's
    /// Music folder.
    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        selected: Option<&Selected>,
        album: bool,
    ) -> PreviewResponse {
        let Some(file) = selected else {
            ui.weak(tr!(ui, "demo-files-none"));
            return PreviewResponse::default();
        };
        if kind_of(&file.info.name) == Kind::Audio {
            self.ensure_player(ui.ctx());
        }
        if file.info.name.to_ascii_lowercase().ends_with(".txt") {
            // The demo's own view of text, with the details as a footer under it.
            let stale = self.text.as_ref().is_none_or(|(key, _)| *key != file.key);
            if stale {
                self.text = Some((file.key.clone(), read_text(&file.path)));
            }
            match self.text.as_ref().map(|(_, text)| text) {
                Some(Ok(text)) => {
                    ui.label(egui::RichText::new(text).monospace());
                }
                Some(Err(e)) => {
                    ui.weak(e);
                }
                None => {}
            }
            ui.add_space(8.0);
            let _ = InfoCard::new(&file.info).footer().ui(ui);
            return PreviewResponse::default();
        }
        let content = Content::file(file.key.clone(), file.path.clone());
        if album && self.preview.wants_content(&file.info.name) {
            // The Add composition: the app's own section, then the default previews.
            ui.weak(tr!(ui, "demo-files-album"));
            ui.add_space(4.0);
        }
        self.preview.ui(
            ui,
            &file.info,
            PreviewState::Ready(&content),
            platform::open_with_system_available(),
        )
    }
}

/// The first [`TEXT_MAX`] bytes of the text file at `path`.
fn read_text(path: &Path) -> Result<String, String> {
    // On the UI thread only because the file is tiny and capped at 4 KiB: a real app reads off it.
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|f| f.take(TEXT_MAX).read_to_end(&mut bytes))
        .map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
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
    let (mut pane, mut detail) = state
        .detail
        .take()
        .unwrap_or_else(|| (DetailPane::new("files-detail"), Detail::default()));
    let selected = state.selected();
    // Files under the sample's Music folder get a section of the demo's own above the default.
    let album = state.sample
        && selected
            .as_ref()
            .zip(state.root.as_ref())
            .is_some_and(|(file, root)| file.path.starts_with(root.join("Music")));
    let mut response = PreviewResponse::default();
    pane.ui(
        ui,
        |ui| main_ui(ui, state, log),
        |ui| response = detail.ui(ui, selected.as_ref(), album),
    );
    if response.retry {
        state.read(ui.ctx(), state.explorer.location().to_vec());
    }
    if response.open
        && let Some(file) = &selected
    {
        platform::open_with_system(&file.path);
    }
    // Picking a file opens the pane, for the card and the phone's layouts.
    let now = state.selected().map(|file| file.key);
    if now != detail.last {
        if now.is_some() {
            pane.open();
        }
        detail.last = now;
        ui.ctx().request_repaint();
    }
    state.detail = Some((pane, detail));
}

/// The explorer, with its heading and buttons: the pane's main view.
fn main_ui(ui: &mut egui::Ui, state: &mut State, log: &mut Message) {
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
                    detail: None,
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
    /// The file picked, when exactly one is.
    fn selected(&self) -> Option<Selected> {
        let picked = self.explorer.selected();
        let [entry] = picked.as_slice() else {
            return None;
        };
        if entry.kind != EntryKind::File {
            return None;
        }
        let path = self.path(self.explorer.location())?.join(&entry.name);
        let secs = entry
            .modified
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());
        let key = format!(
            "{}:{secs}:{}",
            path.display(),
            entry.size.unwrap_or_default()
        );
        Some(Selected {
            info: FileInfo::from(*entry),
            path,
            key,
        })
    }

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
    let sample_png = include_bytes!("../../assets/sample.png");
    file(root.join("Pictures/sample.png"), sample_png)?;
    file(root.join("Music/First album/cover.png"), sample_png)?;
    file(root.join("Music/First album/00 Tone.wav"), &tone_wav())?;
    file(
        root.join("Music/First album/album.cue"),
        ALBUM_CUE.as_bytes(),
    )?;
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

/// A cue sheet, in UTF-8 like every text file the preview shows.
const ALBUM_CUE: &str = "REM Sample cue sheet\nPERFORMER \"コランダム\"\nTITLE \"最初のアルバム\"\nFILE \"00 Tone.wav\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"トーン\"\n    INDEX 01 00:00:00\n";

/// A 2 s, 44.1 kHz, mono, 16-bit WAV of a 440 Hz tone, its 44-byte header written by hand.
fn tone_wav() -> Vec<u8> {
    const RATE: u32 = 44_100;
    let frames = RATE * 2;
    let data_len = frames * 2;
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
    bytes.extend_from_slice(&RATE.to_le_bytes());
    bytes.extend_from_slice(&(RATE * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..frames {
        let t = i as f32 / RATE as f32;
        let v = ((t * 440.0 * std::f32::consts::TAU).sin() * 0.5 * f32::from(i16::MAX)) as i16;
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Read, Seek, SeekFrom};

    use corundum_framework::audio::decode::{self, Source};

    use super::tone_wav;

    /// A sound in memory, to probe.
    struct Memory(Cursor<Vec<u8>>);

    impl Read for Memory {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.0.read(buf)
        }
    }

    impl Seek for Memory {
        fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
            self.0.seek(pos)
        }
    }

    impl Source for Memory {
        fn byte_len(&self) -> Option<u64> {
            Some(self.0.get_ref().len() as u64)
        }
    }

    #[test]
    fn the_tone_is_a_wav_the_decoder_reads() {
        let source = Memory(Cursor::new(tone_wav()));
        let info = decode::probe(Box::new(source), Some("wav")).expect("a wav the decoder reads");
        let ms = info.duration_ms.expect("a length");
        assert!(ms.abs_diff(2000) <= 50, "{ms} ms");
    }
}
