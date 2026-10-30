//! The OS's media keys and its "now playing": a player that the keyboard's play key, the
//! headphones' buttons, the lock screen and the system's media panel control, and that shows
//! there what it plays.
//!
//! [`MediaSession`] is the player as the OS sees it. The app tells it what plays
//! ([`set_metadata`](MediaSession::set_metadata)) and whether it plays and from where
//! ([`set_playback`](MediaSession::set_playback)); what the person asks of it comes back as
//! [`MediaEvent`]s, read with [`poll`](MediaSession::poll) at the next frame. The OS calls
//! in on a thread of its own, so the session is made with a function that wakes the app
//! (an egui app's `request_repaint`), and nothing the OS does holds the app's frame.
//!
//! The position is told only when it jumps: when playback starts, pauses or stops, and
//! after a seek. The OS moves it on by the clock while the state says playing, so a player
//! does not send it every frame.
//!
//! Each OS is reached its own way:
//!
//! - Linux and the BSDs: an MPRIS player on the D-Bus session bus
//!   (`org.mpris.MediaPlayer2.<id>`), through zbus, which speaks D-Bus itself rather than
//!   linking libdbus. GNOME's and KDE's media panels, `playerctl` and the media keys of
//!   most desktops drive MPRIS players.
//! - Windows: the window's SystemMediaTransportControls, the media overlay and the keys.
//!   They belong to a window, so the session needs the window's handle.
//! - macOS: MediaPlayer's now-playing info and remote commands (the Control Centre, the
//!   keys, AirPods).
//! - iOS: the same, with the audio session (`AVAudioSession`) in the playback category.
//!   The session is made active the first time playback starts, not before, so that a
//!   player opening does not stop the music another app plays. A phone call or an alarm
//!   that interrupts playback, and headphones pulled out, come as events.
//! - Android and any other OS: nothing yet. The session is made, does nothing, and says so
//!   with [`is_connected`](MediaSession::is_connected).
//!
//! What an app must add to use it:
//!
//! - iOS: to play on in the background or with the screen locked, the Info.plist's
//!   `UIBackgroundModes` must list `audio`. iOS shows a player on the lock screen and in the
//!   Control Centre only while its audio session plays sound: the now-playing info of an
//!   app that plays nothing is not shown.
//! - macOS: nothing in the Info.plist. The Control Centre lists the app that last set its
//!   now-playing info; it may not list an app that has never played sound.
//! - Windows: nothing, but the window must exist (the controls belong to it).
//! - Linux: a session bus, which every desktop session has. `desktop_entry` names the
//!   app's `.desktop` file, from which a media panel takes the icon.

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod apple;
#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
))]
mod mpris;
#[cfg(not(any(
    all(
        unix,
        not(any(target_os = "macos", target_os = "ios", target_os = "android"))
    ),
    windows,
    target_os = "macos",
    target_os = "ios"
)))]
mod none;
#[cfg(windows)]
mod windows;

#[cfg(any(target_os = "macos", target_os = "ios"))]
use apple as imp;
#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
))]
use mpris as imp;
#[cfg(not(any(
    all(
        unix,
        not(any(target_os = "macos", target_os = "ios", target_os = "android"))
    ),
    windows,
    target_os = "macos",
    target_os = "ios"
)))]
use none as imp;
#[cfg(windows)]
use windows as imp;

use std::io;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, mpsc::Receiver};
use std::time::Duration;

/// The window-handle crate the session takes its window from, for an app that names the
/// trait itself.
pub use raw_window_handle;
use raw_window_handle::HasWindowHandle;

/// Who the player is, as the OS names it.
#[derive(Clone, Copy, Debug)]
pub struct MediaSessionSpec<'a> {
    /// The app's identifier: on Linux the end of the player's D-Bus name
    /// (`org.mpris.MediaPlayer2.<id>`), with any character D-Bus does not allow made `_`.
    pub id: &'a str,
    /// The name the OS shows for the player.
    pub name: &'a str,
    /// The app's `.desktop` file, without `.desktop`, from which a Linux media panel takes
    /// the icon; `LauncherSpec::id` for an app
    /// registered with `launcher`. Not used elsewhere.
    pub desktop_entry: Option<&'a str>,
}

/// What plays, as the OS shows it. A field left `None` is shown empty.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Metadata {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    /// The track's length: the OS draws its progress bar and lets the person seek within it.
    pub duration: Option<Duration>,
    pub cover: Option<Cover>,
}

/// A cover image: a file, or its bytes (PNG or JPEG) for an image the app holds in memory
/// or fetched from a server.
#[derive(Clone, Debug, PartialEq)]
pub enum Cover {
    Path(PathBuf),
    /// Shared, so that an app resending its metadata does not copy the image.
    Bytes(Arc<[u8]>),
}

/// Whether the player plays, and the position it is at as it is told.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Playback {
    /// Nothing is playing; the OS shows no position.
    Stopped,
    /// Paused at the position.
    Paused(Duration),
    /// Playing from the position, which the OS moves on by the clock from now.
    Playing(Duration),
}

impl Playback {
    /// The position, where there is one.
    pub fn position(self) -> Option<Duration> {
        match self {
            Playback::Stopped => None,
            Playback::Paused(at) | Playback::Playing(at) => Some(at),
        }
    }
}

/// What the person asked of the player through the OS, or what the OS did to its sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MediaEvent {
    Play,
    Pause,
    /// Play if paused, pause if playing: a keyboard's play key.
    Toggle,
    Stop,
    Next,
    Previous,
    /// Go to this position in the track (a progress bar dragged).
    SeekTo(Duration),
    /// Go on by this much from where playback is (MPRIS's `Seek`).
    SeekForward(Duration),
    /// Go back by this much from where playback is.
    SeekBackward(Duration),
    /// iOS: something else took the sound (a call, an alarm); the OS has stopped the app's.
    /// The player should show itself paused.
    Interrupted,
    /// iOS: the interruption is over. `resume` is the OS's word that playback should go on
    /// by itself, as it does after a call; without it the player stays paused.
    InterruptionEnded {
        resume: bool,
    },
    /// iOS: the output the sound went to is gone (headphones pulled out). Players pause, so
    /// that the sound does not move to the speaker.
    OutputRemoved,
}

/// What the backends call when the OS asks something: the event goes on the channel and the
/// app is woken to read it.
#[derive(Clone)]
pub(crate) struct EventSender {
    tx: mpsc::Sender<MediaEvent>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl EventSender {
    // Not called where the OS has no media session here (`none`).
    #[allow(dead_code)]
    pub(crate) fn send(&self, event: MediaEvent) {
        // A closed channel is a session dropped while the OS was calling in.
        if self.tx.send(event).is_ok() {
            (self.wake)();
        }
    }
}

/// The player as the OS sees it: what plays, whether it plays, and the person's requests
/// from the keys and the system's media panel. Dropping it takes the player away from the
/// OS.
///
/// Made and used on the app's UI thread (on macOS and iOS the main thread, where
/// MediaPlayer's objects live): it is not `Send`, on any OS, so code written on one
/// compiles on the others.
pub struct MediaSession {
    backend: Option<imp::Backend>,
    events: Receiver<MediaEvent>,
    /// The last playback told, so that one told again is not sent.
    playback: Option<Playback>,
    _ui_thread: PhantomData<*const ()>,
}

impl MediaSession {
    /// The player `spec` describes, for `window`, waking the app with `wake` whenever an
    /// event comes in. Windows needs the window and fails without one; the other OSes do
    /// not use it. Where the OS has no media session here (Android, for now), the session is
    /// made and does nothing ([`is_connected`](Self::is_connected) is false).
    ///
    /// On Linux this waits for the session bus to answer, as the player's name is claimed
    /// there: a few milliseconds, once.
    pub fn new(
        spec: &MediaSessionSpec<'_>,
        window: Option<&dyn HasWindowHandle>,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> io::Result<MediaSession> {
        let (tx, events) = mpsc::channel();
        let sender = EventSender {
            tx,
            wake: Arc::new(wake),
        };
        Ok(MediaSession {
            backend: imp::Backend::new(spec, window, sender)?,
            events,
            playback: None,
            _ui_thread: PhantomData,
        })
    }

    /// A session that is not connected to the OS: for an app that could not make one, or
    /// in tests, so that the player's code is the same.
    pub fn disconnected() -> MediaSession {
        MediaSession {
            backend: None,
            events: mpsc::channel().1,
            playback: None,
            _ui_thread: PhantomData,
        }
    }

    /// Whether the OS sees this player: false on an OS with no media session here, and for
    /// [`disconnected`](Self::disconnected).
    pub fn is_connected(&self) -> bool {
        self.backend.is_some()
    }

    /// Show `metadata` as what plays. A new track starts from its start: tell the playback
    /// after it, with its position.
    pub fn set_metadata(&mut self, metadata: &Metadata) -> io::Result<()> {
        match &mut self.backend {
            Some(backend) => backend.set_metadata(metadata),
            None => Ok(()),
        }
    }

    /// Tell the OS the player now plays, is paused or stopped, and where. Call it when the
    /// state changes and after a seek, not every frame: the OS moves the position on by
    /// itself while playing. The same playback told twice in a row is sent once.
    pub fn set_playback(&mut self, playback: Playback) -> io::Result<()> {
        if self.playback == Some(playback) {
            return Ok(());
        }
        self.playback = Some(playback);
        match &mut self.backend {
            Some(backend) => backend.set_playback(playback),
            None => Ok(()),
        }
    }

    /// The next event the OS sent, without waiting: `None` when there is none.
    pub fn poll(&self) -> Option<MediaEvent> {
        self.events.try_recv().ok()
    }

    /// Every event the OS has sent since the last read, without waiting.
    pub fn events(&self) -> impl Iterator<Item = MediaEvent> + '_ {
        self.events.try_iter()
    }
}

impl std::fmt::Debug for MediaSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaSession")
            .field("connected", &self.is_connected())
            .field("playback", &self.playback)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_disconnected_session_takes_everything_and_sends_nothing() {
        let mut session = MediaSession::disconnected();
        assert!(!session.is_connected());
        session
            .set_metadata(&Metadata {
                title: Some("A".into()),
                ..Metadata::default()
            })
            .unwrap();
        session
            .set_playback(Playback::Playing(Duration::ZERO))
            .unwrap();
        assert_eq!(session.poll(), None);
    }

    #[test]
    fn events_are_read_in_order_and_wake_the_app() {
        let (tx, events) = mpsc::channel();
        let woken = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let sender = EventSender {
            tx,
            wake: Arc::new({
                let woken = woken.clone();
                move || {
                    woken.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }
            }),
        };
        let session = MediaSession {
            backend: None,
            events,
            playback: None,
            _ui_thread: PhantomData,
        };
        sender.send(MediaEvent::Play);
        sender.send(MediaEvent::SeekTo(Duration::from_secs(3)));
        assert_eq!(woken.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert_eq!(
            session.events().collect::<Vec<_>>(),
            [MediaEvent::Play, MediaEvent::SeekTo(Duration::from_secs(3))]
        );
        assert_eq!(session.poll(), None);
    }

    #[test]
    fn the_position_is_there_unless_stopped() {
        let at = Duration::from_secs(7);
        assert_eq!(Playback::Playing(at).position(), Some(at));
        assert_eq!(Playback::Paused(at).position(), Some(at));
        assert_eq!(Playback::Stopped.position(), None);
    }
}
