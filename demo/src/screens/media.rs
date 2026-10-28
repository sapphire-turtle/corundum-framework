//! The Media screen: a player under the OS's media keys and in its "now playing", through
//! corundum's media session. The player plays nothing (on iOS, silence): it runs a clock
//! over a list of made-up tracks, so that what the OS shows and sends can be tried apart
//! from any audio.
//!
//! The player follows the OS from every screen, not only this one: its events are read
//! each frame, before the shell draws ([`State::update`]).

use std::time::{Duration, Instant};

#[cfg(not(target_os = "android"))]
use corundum_framework::gui::media_session::{
    self, Cover, MediaEvent, MediaSession, MediaSessionSpec, Metadata, Playback,
};
use corundum_framework::gui::{egui, tr};

use super::{heading, section};

/// A made-up track: its title, artist, album and length in seconds.
struct Track {
    title: &'static str,
    artist: &'static str,
    album: &'static str,
    seconds: u64,
    /// Whether the OS is given a cover for it: the last track has none, to see the last
    /// one's taken away.
    cover: bool,
}

const TRACKS: [Track; 3] = [
    Track {
        title: "Hexagon",
        artist: "The Sapphires",
        album: "Corundum",
        seconds: 95,
        cover: true,
    },
    Track {
        title: "Sparkle",
        artist: "Asteria",
        album: "Corundum",
        seconds: 140,
        cover: true,
    },
    Track {
        title: "Silence in Blue",
        artist: "The Sapphires",
        album: "B-sides",
        seconds: 30,
        cover: false,
    },
];

/// How far Previous goes back to the start of the track rather than to the one before.
const RESTART_AFTER: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Status {
    Stopped,
    Paused,
    Playing,
}

/// The player, and the OS's view of it.
pub(crate) struct State {
    track: usize,
    status: Status,
    /// The position when the player last started, paused or sought.
    at: Duration,
    /// When it last started, while it plays.
    since: Option<Instant>,
    /// The last thing the OS sent, as the screen shows it.
    last_event: Option<String>,
    /// Where the progress bar is dragged to, until it is let go.
    scrub: Option<f32>,
    #[cfg(not(target_os = "android"))]
    session: MediaSession,
    /// Why there is no session, when making one failed.
    error: Option<String>,
    /// The cover the OS is given: the demo's icon, drawn once.
    #[cfg(not(target_os = "android"))]
    cover: Option<std::sync::Arc<[u8]>>,
    #[cfg(target_os = "ios")]
    silence: Option<silence::Silence>,
}

impl State {
    pub(crate) fn new() -> State {
        State {
            track: 0,
            status: Status::Stopped,
            at: Duration::ZERO,
            since: None,
            last_event: None,
            scrub: None,
            #[cfg(not(target_os = "android"))]
            session: MediaSession::disconnected(),
            error: None,
            #[cfg(not(target_os = "android"))]
            cover: None,
            #[cfg(target_os = "ios")]
            silence: None,
        }
    }

    /// Put the player under the OS's media keys, for the window `cc` makes.
    #[cfg(not(target_os = "android"))]
    pub(crate) fn connect(&mut self, cc: &corundum_framework::eframe::CreationContext<'_>) {
        let spec = MediaSessionSpec {
            id: crate::LAUNCHER.id,
            name: crate::LAUNCHER.name,
            desktop_entry: Some(crate::LAUNCHER.id),
        };
        match media_session::start(cc, &spec) {
            Ok(session) => self.session = session,
            Err(err) => {
                tracing::warn!("corundum demo: no media session: {err}");
                self.error = Some(err.to_string());
            }
        }
        #[cfg(not(target_os = "ios"))]
        {
            self.cover = corundum_framework::platform::icon::render_png(
                include_bytes!("../../assets/app-icon.svg"),
                256,
            )
            .ok()
            .map(Into::into);
        }
        #[cfg(target_os = "ios")]
        {
            self.silence = silence::Silence::new();
        }
        self.tell_track();
    }

    fn length(&self) -> Duration {
        Duration::from_secs(TRACKS[self.track].seconds)
    }

    /// Where playback is now: moved on by the clock while playing.
    fn position(&self) -> Duration {
        let moved = self.since.map_or(Duration::ZERO, |since| since.elapsed());
        (self.at + moved).min(self.length())
    }

    /// Follow what the OS sent, and go on to the next track at the end of one; then ask for
    /// a frame when the track ends, and while the screen is open, to move its clock.
    pub(crate) fn update(&mut self, ctx: &egui::Context, on_screen: bool) {
        #[cfg(not(target_os = "android"))]
        while let Some(event) = self.session.poll() {
            self.last_event = Some(format!("{event:?}"));
            self.follow(event);
        }
        if self.status == Status::Playing {
            let left = self.length().saturating_sub(self.position());
            if left.is_zero() {
                self.next();
            } else {
                ctx.request_repaint_after(if on_screen {
                    left.min(Duration::from_millis(250))
                } else {
                    left
                });
            }
        }
    }

    #[cfg(not(target_os = "android"))]
    fn follow(&mut self, event: MediaEvent) {
        match event {
            MediaEvent::Play | MediaEvent::InterruptionEnded { resume: true } => self.play(),
            MediaEvent::Pause | MediaEvent::Interrupted | MediaEvent::OutputRemoved => self.pause(),
            MediaEvent::Toggle => self.toggle(),
            MediaEvent::Stop => self.stop(),
            MediaEvent::Next => self.next(),
            MediaEvent::Previous => self.previous(),
            MediaEvent::SeekTo(to) => self.seek(to),
            MediaEvent::SeekForward(by) => self.seek(self.position() + by),
            MediaEvent::SeekBackward(by) => self.seek(self.position().saturating_sub(by)),
            _ => {}
        }
    }

    fn play(&mut self) {
        if self.status != Status::Playing {
            self.status = Status::Playing;
            self.since = Some(Instant::now());
            self.tell_playback();
        }
    }

    fn pause(&mut self) {
        if self.status == Status::Playing {
            self.at = self.position();
            self.since = None;
            self.status = Status::Paused;
            self.tell_playback();
        }
    }

    fn toggle(&mut self) {
        if self.status == Status::Playing {
            self.pause();
        } else {
            self.play();
        }
    }

    fn stop(&mut self) {
        self.status = Status::Stopped;
        self.at = Duration::ZERO;
        self.since = None;
        self.tell_playback();
    }

    fn seek(&mut self, to: Duration) {
        self.at = to.min(self.length());
        if self.status == Status::Playing {
            self.since = Some(Instant::now());
        }
        self.tell_playback();
    }

    fn next(&mut self) {
        self.go_to((self.track + 1) % TRACKS.len());
    }

    fn previous(&mut self) {
        if self.position() > RESTART_AFTER {
            self.seek(Duration::ZERO);
        } else {
            self.go_to((self.track + TRACKS.len() - 1) % TRACKS.len());
        }
    }

    /// Start `track` from its start, playing on if the player played.
    fn go_to(&mut self, track: usize) {
        self.track = track;
        self.at = Duration::ZERO;
        if self.status == Status::Playing {
            self.since = Some(Instant::now());
        }
        self.tell_track();
    }

    /// Tell the OS the track, then where in it the player is.
    fn tell_track(&mut self) {
        #[cfg(not(target_os = "android"))]
        {
            let track = &TRACKS[self.track];
            let metadata = Metadata {
                title: Some(track.title.to_owned()),
                artist: Some(track.artist.to_owned()),
                album: Some(track.album.to_owned()),
                duration: Some(self.length()),
                cover: track
                    .cover
                    .then(|| self.cover.clone().map(Cover::Bytes))
                    .flatten(),
            };
            if let Err(err) = self.session.set_metadata(&metadata) {
                tracing::warn!("corundum demo: the media session took no track: {err}");
            }
        }
        self.tell_playback();
    }

    /// Tell the OS whether the player plays, and where: on a change and a seek only.
    fn tell_playback(&mut self) {
        #[cfg(target_os = "ios")]
        if let Some(silence) = &self.silence {
            silence.play(self.status == Status::Playing);
        }
        #[cfg(not(target_os = "android"))]
        {
            let at = self.position();
            let playback = match self.status {
                Status::Stopped => Playback::Stopped,
                Status::Paused => Playback::Paused(at),
                Status::Playing => Playback::Playing(at),
            };
            if let Err(err) = self.session.set_playback(playback) {
                tracing::warn!("corundum demo: the media session took no playback: {err}");
            }
        }
    }

    /// Whether the OS sees the player.
    fn connected(&self) -> bool {
        #[cfg(not(target_os = "android"))]
        {
            self.session.is_connected()
        }
        #[cfg(target_os = "android")]
        {
            false
        }
    }
}

pub(super) fn ui(ui: &mut egui::Ui, state: &mut State) {
    heading(ui, "demo-screen-media");
    ui.label(tr!(ui, "demo-media-intro"));

    section(ui, "demo-media-session");
    if cfg!(target_os = "android") {
        ui.label(tr!(ui, "demo-media-unsupported"));
    } else if state.connected() {
        ui.label(tr!(ui, "demo-media-connected"));
    } else {
        let error = state.error.clone().unwrap_or_default();
        ui.label(tr!(ui, "demo-media-disconnected", error = error));
    }
    let last = state
        .last_event
        .clone()
        .unwrap_or_else(|| tr!(ui, "demo-media-no-event"));
    ui.label(tr!(ui, "demo-media-last-event", event = last));
    if cfg!(target_os = "ios") {
        ui.weak(tr!(ui, "demo-media-ios-silence"));
    }

    section(ui, "demo-media-player");
    let track = &TRACKS[state.track];
    ui.strong(track.title);
    ui.label(format!("{} · {}", track.artist, track.album));
    let length = state.length();
    // The bar follows the pointer while dragged, and the player seeks once, when let go:
    // one seek for the OS, not one a frame.
    let mut seconds = state
        .scrub
        .unwrap_or_else(|| state.position().as_secs_f32());
    let slider = egui::Slider::new(&mut seconds, 0.0..=length.as_secs_f32())
        .show_value(false)
        .trailing_fill(true);
    let response = ui.add(slider);
    if response.dragged() {
        state.scrub = Some(seconds);
    }
    if response.drag_stopped() || (response.changed() && !response.dragged()) {
        state.scrub = None;
        state.seek(Duration::from_secs_f32(seconds));
    }
    ui.label(format!("{} / {}", clock(state.position()), clock(length)));
    ui.horizontal_wrapped(|ui| {
        if ui.button(tr!(ui, "demo-media-previous")).clicked() {
            state.previous();
        }
        let toggle = if state.status == Status::Playing {
            "demo-media-pause"
        } else {
            "demo-media-play"
        };
        if ui.button(tr!(ui, toggle)).clicked() {
            state.toggle();
        }
        if ui.button(tr!(ui, "demo-media-stop")).clicked() {
            state.stop();
        }
        if ui.button(tr!(ui, "demo-media-next")).clicked() {
            state.next();
        }
    });
    let status = match state.status {
        Status::Stopped => "demo-media-stopped",
        Status::Paused => "demo-media-paused",
        Status::Playing => "demo-media-playing",
    };
    ui.weak(tr!(ui, status));
}

/// `at` as minutes and seconds.
fn clock(at: Duration) -> String {
    let seconds = at.as_secs();
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

#[cfg(target_os = "ios")]
mod silence {
    //! Silence, looped, while the player plays: iOS lists in its now-playing only an app
    //! whose audio session plays sound, and the demo's player has none of its own.

    use objc2::AllocAnyThread;
    use objc2::rc::Retained;
    use objc2_avf_audio::AVAudioPlayer;
    use objc2_foundation::NSData;

    /// Samples a second, of the WAV file made below.
    const RATE: u32 = 8000;

    pub(super) struct Silence {
        player: Retained<AVAudioPlayer>,
    }

    impl Silence {
        pub(super) fn new() -> Option<Silence> {
            let data = NSData::with_bytes(&wav());
            // SAFETY: a player made from a whole WAV file in memory.
            let player =
                unsafe { AVAudioPlayer::initWithData_error(AVAudioPlayer::alloc(), &data) }
                    .inspect_err(|err| {
                        tracing::warn!(
                            "corundum demo: no silent player: {}",
                            err.localizedDescription()
                        )
                    })
                    .ok()?;
            // SAFETY: the player's loop count; -1 loops until paused.
            unsafe { player.setNumberOfLoops(-1) };
            Some(Silence { player })
        }

        pub(super) fn play(&self, playing: bool) {
            // SAFETY: the player made above.
            unsafe {
                if playing {
                    self.player.play();
                } else {
                    self.player.pause();
                }
            }
        }
    }

    /// One second of silence as a WAV file: 8 kHz, mono, 16-bit.
    fn wav() -> Vec<u8> {
        let samples = RATE as usize;
        let data_len = (samples * 2) as u32;
        let mut wav = Vec::with_capacity(44 + samples * 2);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_len).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&1u16.to_le_bytes()); // mono
        wav.extend_from_slice(&RATE.to_le_bytes());
        wav.extend_from_slice(&(RATE * 2).to_le_bytes()); // bytes a second
        wav.extend_from_slice(&2u16.to_le_bytes()); // bytes a frame
        wav.extend_from_slice(&16u16.to_le_bytes()); // bits a sample
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());
        wav.resize(44 + samples * 2, 0);
        wav
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previous_goes_back_to_the_start_first() {
        let mut state = State::new();
        state.go_to(1);
        state.seek(Duration::from_secs(60));
        state.previous();
        assert_eq!((state.track, state.at), (1, Duration::ZERO));
        state.previous();
        assert_eq!(state.track, 0);
        state.previous();
        assert_eq!(state.track, TRACKS.len() - 1);
    }

    #[test]
    fn a_seek_stays_within_the_track() {
        let mut state = State::new();
        state.seek(Duration::from_secs(10_000));
        assert_eq!(state.position(), state.length());
    }

    #[test]
    fn pause_keeps_the_position_and_stop_goes_back() {
        let mut state = State::new();
        state.seek(Duration::from_secs(20));
        state.play();
        state.pause();
        assert_eq!(state.status, Status::Paused);
        assert!(state.position() >= Duration::from_secs(20));
        state.stop();
        assert_eq!(
            (state.status, state.position()),
            (Status::Stopped, Duration::ZERO)
        );
    }
}
