//! [`AudioPreview`]: a sound the app hands in, its tags read off the UI thread, with a play
//! button and a seek bar over the app's [`Player`] of [`Content`]; and [`ContentOpener`],
//! which that player plays it with.

use std::io::{Read, Seek, SeekFrom};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

use corundum_framework_audio::decode::{Info, Source, probe};
use corundum_framework_audio::feed::{FailedFeed, spawn_feed};
use corundum_framework_audio::{
    Command, Feed, Opener, OutputConfig, Player, PlayerError, PlayerState, QueueItem, Status,
    TrackError,
};

use super::content::{Content, PreviewResponse, Reader, State, status_ui};
use crate::{icons, tr};

impl QueueItem for Content {
    fn title(&self) -> String {
        self.name.clone()
    }
}

/// A reader as something to decode from; its length is found once, when it is made.
struct AsSource {
    reader: Box<dyn Reader>,
    len: Option<u64>,
}

impl AsSource {
    fn new(mut reader: Box<dyn Reader>) -> AsSource {
        let len = reader.seek(SeekFrom::End(0)).ok();
        if reader.seek(SeekFrom::Start(0)).is_err() {
            return AsSource { reader, len: None };
        }
        AsSource { reader, len }
    }
}

impl Read for AsSource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.reader.read(buf)
    }
}

impl Seek for AsSource {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.reader.seek(pos)
    }
}

impl Source for AsSource {
    fn byte_len(&self) -> Option<u64> {
        self.len
    }
}

/// Plays [`Content`]: bytes from memory, a reader from its opener; the extension is the
/// format hint. Spawn the preview's [`Player`] with it.
#[derive(Clone, Copy, Debug, Default)]
pub struct ContentOpener;

impl Opener<Content> for ContentOpener {
    fn open(&mut self, content: &Content, at_ms: u64, target: OutputConfig) -> Box<dyn Feed> {
        // This runs on the player's thread, not the UI's.
        match content.open() {
            Ok(reader) => spawn_feed(
                Box::new(AsSource::new(reader)),
                None,
                content.ext(),
                at_ms,
                target,
            ),
            Err(e) => Box::new(FailedFeed(TrackError::Unplayable(e.to_string()))),
        }
    }

    fn preload(&mut self, _next: Option<&Content>) {}

    fn clear(&mut self) {}
}

/// How many probed files are kept.
const KEPT: usize = 8;

/// The music icon's size above the title, in points.
const ICON: f32 = 48.0;

/// The play and pause icons' size, in points.
const BUTTON_ICON: f32 = 16.0;

/// The seek bar's widest, in points.
const SEEK_WIDTH: f32 = 320.0;
/// How far an arrow key on the seek bar moves, in ms.
const SEEK_KEY_STEP_MS: f64 = 5_000.0;

/// What the worker made of a content.
enum Probed {
    /// Audio, with what its tags and container say.
    Audio(Info),
    /// The content could not be opened (gone, or not ours to read): nothing is known, and the
    /// player says why when it is played.
    Unread,
    /// Opened, and not audio the decoder knows.
    NotAudio,
}

/// The worker thread's end: contents out, answers in.
struct Worker {
    requests: Sender<Content>,
    answers: Receiver<(String, Probed)>,
    /// Requests sent and not yet answered or passed over. The thread holds a clone for as
    /// long as it runs, which is how the tests see that it ended, and when it is idle.
    pending: Arc<AtomicUsize>,
}

impl Worker {
    /// Start the thread. It takes the newest request it has, passing over older ones, and
    /// ends when the [`AudioPreview`] is dropped.
    fn spawn(ctx: egui::Context) -> Worker {
        let (requests, inbox) = channel::<Content>();
        let (outbox, answers) = channel();
        let pending = Arc::new(AtomicUsize::new(0));
        let count = pending.clone();
        std::thread::Builder::new()
            .name("corundum-audio-probe".into())
            .spawn(move || {
                while let Ok(mut content) = inbox.recv() {
                    let mut taken = 1;
                    while let Ok(newer) = inbox.try_recv() {
                        content = newer;
                        taken += 1;
                    }
                    let probed = answer(&content);
                    let sent = outbox.send((content.key, probed));
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

/// Read the tags and the length of `content`.
fn answer(content: &Content) -> Probed {
    let reader = match content.open() {
        Ok(reader) => reader,
        Err(error) => {
            tracing::debug!("the audio preview could not open {}: {error}", content.name);
            return Probed::Unread;
        }
    };
    let ext = content.ext();
    // A panic in the demuxer would end the thread, and the next frame would start another to
    // probe the content again: it is not audio instead, once.
    let probed = catch_unwind(AssertUnwindSafe(|| {
        probe(Box::new(AsSource::new(reader)), ext.as_deref())
    }));
    match probed {
        Ok(Ok(info)) => Probed::Audio(info),
        Ok(Err(error)) => {
            tracing::debug!("{} is not audio: {error}", content.name);
            Probed::NotAudio
        }
        Err(_) => Probed::NotAudio,
    }
}

/// A sound in the detail pane: the music icon, its title (else its name) and its artist and
/// album when the tags have them, a play button, a seek bar and the time, and why it did not
/// play, if it did not.
///
/// What it draws is the [`Content`] the app hands in with a [`State`]; the app's key names
/// the sound. The tags are read on a worker thread (the newest content shown wins, the last
/// eight are kept by key), with a spinner meanwhile; the UI thread never opens or reads the
/// content. Nothing plays on its own: Play replaces what `player` has with this content, and
/// the sound that played before keeps playing while another is only shown. A sound the
/// decoder does not know says so and offers no play.
///
/// Make one and keep it across frames; the thread it starts ends when it is dropped. The
/// player is the app's, a `Player<Content>` spawned with [`ContentOpener`] and a repaint of
/// its window, so the time moves.
#[derive(Default)]
pub struct AudioPreview {
    /// Started on the first frame that needs it.
    worker: Option<Worker>,
    /// The key of the last content asked for, until it is answered, so a frame asks again
    /// only for something new.
    asked: Option<String>,
    /// The probed contents, one per key, the most recently shown last.
    kept: Vec<(String, Probed)>,
    /// Where the seek bar is held, in ms, for the key it is held on, until it is let go.
    held: Option<(String, f64)>,
    /// Whether the last frame's seek bar was on, `None` when it drew none.
    #[cfg(test)]
    seek_enabled: Option<bool>,
    /// The last seek bar's id: to tell a keyboard step from a drag, and for a test to focus it.
    seek_id: Option<egui::Id>,
}

impl AudioPreview {
    /// An empty preview; its thread starts on the first frame that needs it.
    pub fn new() -> AudioPreview {
        AudioPreview::default()
    }

    /// Draw the sound of `state`, played by `player`, in the space `ui` has left. For
    /// Loading and Failed it draws the spinner, or the reason and a Retry button, and sets
    /// `retry` in the response when that is pressed.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        state: State<'_>,
        player: &Player<Content>,
    ) -> PreviewResponse {
        let mut response = PreviewResponse::default();
        self.receive();
        #[cfg(test)]
        {
            self.seek_enabled = None;
        }
        let Some(content) = status_ui(ui, state, &mut response) else {
            return response;
        };
        let info = match self.kept.iter().position(|(k, _)| *k == content.key) {
            Some(at) => {
                let kept = self.kept.remove(at);
                self.kept.push(kept);
                match &self.kept[self.kept.len() - 1].1 {
                    Probed::Audio(info) => Some(Some(info.clone())),
                    Probed::Unread => Some(None),
                    Probed::NotAudio => {
                        not_audio(ui);
                        return response;
                    }
                }
            }
            None => {
                self.ask(ui.ctx(), content);
                None
            }
        };
        // `None` while the worker has not answered; `Some(None)` when it found nothing.
        let probing = info.is_none();
        let info = info.flatten();
        let state = player.state();
        let here = plays(&state, content);

        ui.vertical_centered(|ui| {
            ui.add(icons::image(ui, icons::music(), ICON));
            let title = info
                .as_ref()
                .and_then(|i| i.title.clone())
                .unwrap_or_else(|| content.name.clone());
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

            self.button(ui, content, player, &state, here);

            let probed = info.as_ref().and_then(|i| i.duration_ms);
            let length = shown_length(here, state.duration_ms, probed);
            let position = self.seek_bar(ui, content, player, &state, here, length);
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
        response
    }

    /// The play / pause button.
    fn button(
        &self,
        ui: &mut egui::Ui,
        content: &Content,
        player: &Player<Content>,
        state: &PlayerState<Content>,
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
                    items: vec![content.clone()],
                    start: 0,
                });
            }
        }
    }

    /// The seek bar, on only while this file plays and its length is known; letting it go,
    /// or a key on it, seeks there. Returns the position to show, in ms: the player's (or
    /// where the bar is held), whatever the bar can show.
    fn seek_bar(
        &mut self,
        ui: &mut egui::Ui,
        content: &Content,
        player: &Player<Content>,
        state: &PlayerState<Content>,
        here: bool,
        length: Option<u64>,
    ) -> u64 {
        let enabled = here && length.is_some();
        if !enabled {
            self.held = None;
        }
        let held = self
            .held
            .as_ref()
            .filter(|(k, _)| *k == content.key)
            .map(|(_, ms)| *ms);
        let position = shown_position(here, held, state.position_ms);
        let end = length.filter(|_| enabled).unwrap_or(0).max(1) as f64;
        let mut value = seek_value(enabled, position, end as u64);
        ui.spacing_mut().slider_width = (ui.available_width() - 16.0).clamp(0.0, SEEK_WIDTH);
        let mut slider = egui::Slider::new(&mut value, 0.0..=end).show_value(false);
        // An arrow key on the focused bar moves five seconds, not a point of the rail. The
        // step also snaps the value, so it applies only to a keyboard (the bar has focus and
        // no pointer is down): dragging stays smooth.
        let focused = self
            .seek_id
            .is_some_and(|id| ui.memory(|m| m.has_focus(id)));
        if focused && !ui.input(|i| i.pointer.any_down()) {
            slider = slider.step_by(SEEK_KEY_STEP_MS);
        }
        let response = ui.add_enabled(enabled, slider);
        self.seek_id = Some(response.id);
        #[cfg(test)]
        {
            self.seek_enabled = Some(response.enabled());
        }
        let (held, seek) = seek_step(
            held,
            value,
            response.dragged(),
            response.changed(),
            response.drag_stopped(),
        );
        self.held = held.map(|ms| (content.key.clone(), ms));
        if let Some(ms) = seek {
            player.send(Command::Seek(ms));
        }
        held.map_or(position, |ms| ms.max(0.0) as u64)
    }

    /// Send the worker `content`, unless it is the one still unanswered.
    fn ask(&mut self, ctx: &egui::Context, content: &Content) {
        if self.asked.as_deref() == Some(content.key.as_str()) {
            return;
        }
        let worker = self
            .worker
            .get_or_insert_with(|| Worker::spawn(ctx.clone()));
        worker.pending.fetch_add(1, Ordering::SeqCst);
        if worker.requests.send(content.clone()).is_err() {
            // The thread is gone: start another on the next frame.
            self.worker = None;
            self.asked = None;
            return;
        }
        self.asked = Some(content.key.clone());
    }

    /// Keep what the worker has finished; whether it is shown is the frame's to decide.
    fn receive(&mut self) {
        let Some(worker) = &self.worker else {
            return;
        };
        loop {
            match worker.answers.try_recv() {
                Ok((key, probed)) => {
                    if self.asked.as_ref() == Some(&key) {
                        self.asked = None;
                    }
                    self.kept.retain(|(k, _)| *k != key);
                    self.kept.push((key, probed));
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

    /// The last seek bar's id, for a test.
    #[cfg(test)]
    pub(super) fn seek_id(&self) -> Option<egui::Id> {
        self.seek_id
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

/// Whether `state` plays `content`, alone, as this preview's play leaves it. Contents are
/// the same when their keys are.
fn plays(state: &PlayerState<Content>, content: &Content) -> bool {
    state.queue.len() == 1 && state.queue[0] == *content && state.current == Some(0)
}

/// A sound the decoder does not know: the music icon and why there is nothing to play.
fn not_audio(ui: &mut egui::Ui) {
    ui.vertical_centered(|ui| {
        ui.add(icons::image(ui, icons::music(), ICON));
        ui.add(egui::Label::new(tr!(ui, "corundum-preview-unsupported")).wrap());
    });
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

/// The length to show: while this content plays, the player's, else the probe's (the player
/// learns it only once the decoder has opened the file); otherwise the probe's.
pub(super) fn shown_length(here: bool, player: Option<u64>, probed: Option<u64>) -> Option<u64> {
    if here { player.or(probed) } else { probed }
}

/// The seek bar's value in ms: the position within `end`, or 0 while the bar is off.
pub(super) fn seek_value(enabled: bool, position_ms: u64, end_ms: u64) -> f64 {
    if enabled {
        position_ms.min(end_ms) as f64
    } else {
        0.0
    }
}

/// The position to show, in ms: where the bar is held, else the player's while this file
/// plays, else its head.
pub(super) fn shown_position(here: bool, held: Option<f64>, position_ms: u64) -> u64 {
    match held {
        Some(ms) => ms.max(0.0) as u64,
        None if here => position_ms,
        None => 0,
    }
}

/// What a frame of the seek bar does: where it is now held, and where to seek, if anywhere.
/// A drag holds the bar and seeks when it is let go; a change without a drag (an arrow key,
/// a press on the rail) seeks at once.
pub(super) fn seek_step(
    held: Option<f64>,
    value: f64,
    dragged: bool,
    changed: bool,
    released: bool,
) -> (Option<f64>, Option<u64>) {
    let ms = |v: f64| v.max(0.0) as u64;
    if dragged {
        (Some(value), None)
    } else if released {
        (None, held.or(changed.then_some(value)).map(ms))
    } else if changed {
        (None, Some(ms(value)))
    } else {
        (held, None)
    }
}
