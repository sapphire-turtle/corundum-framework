//! [`AudioPreview`]: a sound file's tags, read off the UI thread, with a play button and a
//! seek bar over the app's [`Player`].

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

use corundum_framework_audio::decode::{Info, probe};
use corundum_framework_audio::{Command, Player, PlayerError, PlayerState, Status};

use crate::{icons, tr};

/// How many probed files are kept.
const KEPT: usize = 8;

/// The music icon's size above the title, in points.
const ICON: f32 = 48.0;

/// The play and pause icons' size, in points.
const BUTTON_ICON: f32 = 16.0;

/// The seek bar's widest, in points.
const SEEK_WIDTH: f32 = 320.0;

/// What the worker made of a file.
enum Probed {
    /// Audio, with what its tags and container say.
    Audio(Info),
    /// The file could not be opened (gone, or not ours to read): nothing is known, and the
    /// player says why when it is played.
    Unread,
    /// Opened, and not audio the decoder knows.
    NotAudio,
}

/// The worker thread's end: paths out, answers in.
struct Worker {
    requests: Sender<PathBuf>,
    answers: Receiver<(PathBuf, Probed)>,
    /// Requests sent and not yet answered or passed over. The thread holds a clone for as
    /// long as it runs, which is how the tests see that it ended, and when it is idle.
    pending: Arc<AtomicUsize>,
}

impl Worker {
    /// Start the thread. It takes the newest request it has, passing over older ones, and
    /// ends when the [`AudioPreview`] is dropped.
    fn spawn(ctx: egui::Context) -> Worker {
        let (requests, inbox) = channel::<PathBuf>();
        let (outbox, answers) = channel();
        let pending = Arc::new(AtomicUsize::new(0));
        let count = pending.clone();
        std::thread::Builder::new()
            .name("corundum-audio-probe".into())
            .spawn(move || {
                while let Ok(mut path) = inbox.recv() {
                    let mut taken = 1;
                    while let Ok(newer) = inbox.try_recv() {
                        path = newer;
                        taken += 1;
                    }
                    let probed = answer(&path);
                    let sent = outbox.send((path, probed));
                    count.fetch_sub(taken, Ordering::SeqCst);
                    if sent.is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
            })
            .expect("spawn the audio preview's thread");
        Worker {
            requests,
            answers,
            pending,
        }
    }
}

/// Read the tags and the length of the file at `path`.
fn answer(path: &Path) -> Probed {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) => {
            tracing::debug!(
                "the audio preview could not open {}: {error}",
                path.display()
            );
            return Probed::Unread;
        }
    };
    let ext = path.extension().and_then(|e| e.to_str());
    // A panic in the demuxer would end the thread, and the next frame would start another to
    // probe the file again: it is not audio instead, once.
    match catch_unwind(AssertUnwindSafe(|| probe(Box::new(file), ext))) {
        Ok(Ok(info)) => Probed::Audio(info),
        Ok(Err(error)) => {
            tracing::debug!("{} is not audio: {error}", path.display());
            Probed::NotAudio
        }
        Err(_) => Probed::NotAudio,
    }
}

/// A sound file in the detail pane: the music icon, its title (else its file name) and its
/// artist and album when the tags have them, a play button, a seek bar and the time, and
/// why it did not play, if it did not.
///
/// The tags are read on a worker thread (the newest file picked wins, the last eight are
/// kept), with a spinner meanwhile; the UI thread never touches the file. Nothing plays on
/// its own: Play replaces what `player` has with this file, and the file that played before
/// keeps playing while another is only shown. A file the decoder does not know says so and
/// offers no play.
///
/// Make one and keep it across frames; the thread it starts ends when it is dropped. The
/// player is the app's, spawned with a repaint of its window, so the time moves.
#[derive(Default)]
pub struct AudioPreview {
    /// Started on the first frame that needs it.
    worker: Option<Worker>,
    /// The last path asked for, until it is answered, so a frame asks again only for
    /// something new.
    asked: Option<PathBuf>,
    /// The probed files, one per path, the most recently shown last.
    kept: Vec<(PathBuf, Probed)>,
    /// Where the seek bar is held, in ms, for the path it is held on, until it is let go.
    held: Option<(PathBuf, f64)>,
    /// Whether the last frame's seek bar was on, `None` when it drew none.
    #[cfg(test)]
    seek_enabled: Option<bool>,
}

impl AudioPreview {
    /// An empty preview; its thread starts on the first frame that needs it.
    pub fn new() -> AudioPreview {
        AudioPreview::default()
    }

    /// Draw `path`, played by `player`, in the space `ui` has left.
    pub fn ui(&mut self, ui: &mut egui::Ui, path: &Path, player: &Player<PathBuf>) {
        self.receive();
        #[cfg(test)]
        {
            self.seek_enabled = None;
        }
        let info = match self.kept.iter().position(|(p, _)| p == path) {
            Some(at) => {
                let kept = self.kept.remove(at);
                self.kept.push(kept);
                match &self.kept[self.kept.len() - 1].1 {
                    Probed::Audio(info) => Some(Some(info.clone())),
                    Probed::Unread => Some(None),
                    Probed::NotAudio => {
                        not_audio(ui);
                        return;
                    }
                }
            }
            None => {
                self.ask(ui.ctx(), path);
                None
            }
        };
        // `None` while the worker has not answered; `Some(None)` when it found nothing.
        let probing = info.is_none();
        let info = info.flatten();
        let state = player.state();
        let here = plays(&state, path);

        ui.vertical_centered(|ui| {
            ui.add(icons::image(ui, icons::music(), ICON));
            let title = info
                .as_ref()
                .and_then(|i| i.title.clone())
                .unwrap_or_else(|| file_name(path));
            ui.add(egui::Label::new(egui::RichText::new(title).strong()).wrap());
            // The same height whatever it holds, so the button does not move under a finger
            // when the tags come.
            let row = [ui.available_width(), ui.spacing().interact_size.y];
            if probing {
                ui.add_sized(row, egui::Spinner::new());
            } else {
                let byline = info.as_ref().and_then(byline).unwrap_or_default();
                ui.add_sized(row, egui::Label::new(byline).truncate());
            }

            self.button(ui, path, player, &state, here);

            let length = if here {
                state.duration_ms
            } else {
                info.as_ref().and_then(|i| i.duration_ms)
            };
            let position = self.seek_bar(ui, path, player, &state, here, length);
            let time = format!(
                "{} / {}",
                clock(position),
                length.map_or_else(|| "--:--".to_owned(), clock)
            );
            ui.label(time);

            if here && let Some(error) = &state.error {
                ui.add(egui::Label::new(error_text(ui, error)).wrap());
            }
        });
    }

    /// The play / pause button.
    fn button(
        &self,
        ui: &mut egui::Ui,
        path: &Path,
        player: &Player<PathBuf>,
        state: &PlayerState<PathBuf>,
        here: bool,
    ) {
        let active = here
            && matches!(
                state.status,
                Status::Playing | Status::Buffering | Status::Preparing
            );
        let (icon, label) = if active {
            (icons::pause(), tr!(ui, "corundum-preview-pause"))
        } else {
            (icons::play(), tr!(ui, "corundum-preview-play"))
        };
        let button = egui::Button::image_and_text(icons::image(ui, icon, BUTTON_ICON), label);
        if ui.add(button).clicked() {
            if here {
                player.send(Command::Toggle);
            } else {
                player.send(Command::Replace {
                    items: vec![path.to_owned()],
                    start: 0,
                });
            }
        }
    }

    /// The seek bar, on only while this file plays and its length is known; letting it go
    /// seeks there. Returns the position to show, in ms.
    fn seek_bar(
        &mut self,
        ui: &mut egui::Ui,
        path: &Path,
        player: &Player<PathBuf>,
        state: &PlayerState<PathBuf>,
        here: bool,
        length: Option<u64>,
    ) -> u64 {
        let enabled = here && length.is_some();
        if !enabled {
            self.held = None;
        }
        let end = length.filter(|_| here).unwrap_or(0).max(1) as f64;
        let held = self
            .held
            .as_ref()
            .filter(|(p, _)| p == path)
            .map(|(_, ms)| *ms);
        let mut value = match held {
            Some(ms) => ms,
            None if here => state.position_ms as f64,
            None => 0.0,
        }
        .min(end);
        ui.spacing_mut().slider_width = (ui.available_width() - 16.0).clamp(0.0, SEEK_WIDTH);
        let slider = egui::Slider::new(&mut value, 0.0..=end).show_value(false);
        let response = ui.add_enabled(enabled, slider);
        #[cfg(test)]
        {
            self.seek_enabled = Some(response.enabled());
        }
        if response.dragged() || response.changed() {
            self.held = Some((path.to_owned(), value));
        }
        if (response.drag_stopped() || response.clicked())
            && let Some((_, ms)) = self.held.take()
        {
            player.send(Command::Seek(ms.max(0.0) as u64));
        }
        value.max(0.0) as u64
    }

    /// Send the worker `path`, unless it is the one still unanswered.
    fn ask(&mut self, ctx: &egui::Context, path: &Path) {
        if self.asked.as_deref() == Some(path) {
            return;
        }
        let worker = self
            .worker
            .get_or_insert_with(|| Worker::spawn(ctx.clone()));
        worker.pending.fetch_add(1, Ordering::SeqCst);
        if worker.requests.send(path.to_owned()).is_err() {
            // The thread is gone: start another on the next frame.
            self.worker = None;
            self.asked = None;
            return;
        }
        self.asked = Some(path.to_owned());
    }

    /// Keep what the worker has finished; whether it is shown is the frame's to decide.
    fn receive(&mut self) {
        let Some(worker) = &self.worker else {
            return;
        };
        loop {
            match worker.answers.try_recv() {
                Ok((path, probed)) => {
                    if self.asked.as_ref() == Some(&path) {
                        self.asked = None;
                    }
                    self.kept.retain(|(p, _)| *p != path);
                    self.kept.push((path, probed));
                    if self.kept.len() > KEPT {
                        self.kept.remove(0);
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.worker = None;
                    self.asked = None;
                    break;
                }
            }
        }
    }

    /// Whether the last frame's seek bar was on, `None` when it drew none, for a test.
    #[cfg(test)]
    pub(super) fn seek_enabled(&self) -> Option<bool> {
        self.seek_enabled
    }

    /// Something the worker thread holds while it runs.
    #[cfg(test)]
    pub(super) fn thread_token(&self) -> Option<std::sync::Weak<AtomicUsize>> {
        self.worker.as_ref().map(|w| Arc::downgrade(&w.pending))
    }

    /// Whether the worker has nothing left to do.
    #[cfg(test)]
    pub(super) fn idle(&self) -> bool {
        self.worker
            .as_ref()
            .is_none_or(|w| w.pending.load(Ordering::SeqCst) == 0)
    }
}

/// Whether `state` plays `path`, alone, as this preview's play leaves it.
fn plays(state: &PlayerState<PathBuf>, path: &Path) -> bool {
    state.queue.len() == 1 && state.queue[0] == path && state.current == Some(0)
}

/// A file the decoder does not know: the music icon and why there is nothing to play.
fn not_audio(ui: &mut egui::Ui) {
    ui.vertical_centered(|ui| {
        ui.add(icons::image(ui, icons::music(), ICON));
        ui.add(egui::Label::new(tr!(ui, "corundum-preview-unsupported")).wrap());
    });
}

/// The file name of `path`, for a file without a title.
fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// "artist · album", or whichever of them is known.
fn byline(info: &Info) -> Option<String> {
    let parts: Vec<&str> = [&info.artist, &info.album]
        .into_iter()
        .filter_map(|p| p.as_deref())
        .filter(|p| !p.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// Why this file did not play.
fn error_text(ui: &egui::Ui, error: &PlayerError) -> String {
    match error {
        PlayerError::Unplayable { reason, .. } => {
            tr!(ui, "corundum-preview-failed", reason = reason.as_str())
        }
        PlayerError::Output(reason) => {
            tr!(
                ui,
                "corundum-preview-audio-device",
                reason = reason.as_str()
            )
        }
        other => tr!(ui, "corundum-preview-failed", reason = format!("{other:?}")),
    }
}

/// `ms` as m:ss.
fn clock(ms: u64) -> String {
    let secs = ms / 1000;
    format!("{}:{:02}", secs / 60, secs % 60)
}
