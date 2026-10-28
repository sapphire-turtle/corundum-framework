//! macOS and iOS: MediaPlayer's now-playing info (`MPNowPlayingInfoCenter`) and its remote
//! commands (`MPRemoteCommandCenter`), which the media keys, the Control Centre, the lock
//! screen and AirPods drive. On iOS the audio session too: the playback category, made
//! active when playback starts, and the interruptions and route changes it reports.
//!
//! The commands come on the main thread, from blocks the session gives each command; they
//! go to the app as events. The now-playing info is a dictionary set whole, so the session
//! keeps what it was told and sets it all again on each change.

use std::io;
use std::ptr::NonNull;
use std::time::Duration;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_foundation::{NSData, NSMutableDictionary, NSNumber, NSString};
use objc2_media_player::{
    MPChangePlaybackPositionCommandEvent, MPMediaItemArtwork, MPMediaItemPropertyAlbumTitle,
    MPMediaItemPropertyArtist, MPMediaItemPropertyArtwork, MPMediaItemPropertyPlaybackDuration,
    MPMediaItemPropertyTitle, MPNowPlayingInfoCenter, MPNowPlayingInfoPropertyElapsedPlaybackTime,
    MPNowPlayingInfoPropertyPlaybackRate, MPNowPlayingPlaybackState, MPRemoteCommand,
    MPRemoteCommandCenter, MPRemoteCommandEvent, MPRemoteCommandHandlerStatus,
};
use raw_window_handle::HasWindowHandle;

use super::{Cover, EventSender, MediaEvent, MediaSessionSpec, Metadata, Playback};

pub(super) struct Backend {
    /// Each command turned on, with the target its block was added as, to take it off.
    targets: Vec<(Retained<MPRemoteCommand>, Retained<AnyObject>)>,
    metadata: Metadata,
    artwork: Option<Retained<MPMediaItemArtwork>>,
    playback: Playback,
    #[cfg(target_os = "ios")]
    audio: ios::AudioSession,
}

impl Backend {
    pub(super) fn new(
        _spec: &MediaSessionSpec<'_>,
        _window: Option<&dyn HasWindowHandle>,
        sender: EventSender,
    ) -> io::Result<Option<Backend>> {
        #[cfg(target_os = "ios")]
        let audio = ios::AudioSession::new(sender.clone())?;
        // SAFETY: MediaPlayer's shared objects, on the main thread the session is made on.
        let center = unsafe { MPRemoteCommandCenter::sharedCommandCenter() };
        let simple = |command: Retained<MPRemoteCommand>, event: MediaEvent| {
            target(command, sender.clone(), move |_| Some(event))
        };
        // SAFETY: as above.
        let (play, pause, toggle, stop, next, previous, seek) = unsafe {
            (
                center.playCommand(),
                center.pauseCommand(),
                center.togglePlayPauseCommand(),
                center.stopCommand(),
                center.nextTrackCommand(),
                center.previousTrackCommand(),
                Retained::into_super(center.changePlaybackPositionCommand()),
            )
        };
        let targets = vec![
            simple(play, MediaEvent::Play),
            simple(pause, MediaEvent::Pause),
            simple(toggle, MediaEvent::Toggle),
            simple(stop, MediaEvent::Stop),
            simple(next, MediaEvent::Next),
            simple(previous, MediaEvent::Previous),
            target(seek, sender.clone(), |event| {
                let event = event.downcast_ref::<MPChangePlaybackPositionCommandEvent>()?;
                // SAFETY: a getter of the event the command was sent with.
                let seconds = unsafe { event.positionTime() };
                Duration::try_from_secs_f64(seconds)
                    .ok()
                    .map(MediaEvent::SeekTo)
            }),
        ];
        Ok(Some(Backend {
            targets,
            metadata: Metadata::default(),
            artwork: None,
            playback: Playback::Stopped,
            #[cfg(target_os = "ios")]
            audio,
        }))
    }

    pub(super) fn set_metadata(&mut self, metadata: &Metadata) -> io::Result<()> {
        if metadata.cover != self.metadata.cover {
            self.artwork = match &metadata.cover {
                Some(Cover::Bytes(bytes)) => artwork(bytes),
                Some(Cover::Path(path)) => match std::fs::read(path) {
                    Ok(bytes) => artwork(&bytes),
                    Err(err) => {
                        tracing::warn!("media session: cannot read the cover {path:?}: {err}");
                        None
                    }
                },
                None => None,
            };
        }
        self.metadata = metadata.clone();
        self.publish();
        Ok(())
    }

    pub(super) fn set_playback(&mut self, playback: Playback) -> io::Result<()> {
        #[cfg(target_os = "ios")]
        if matches!(playback, Playback::Playing(_)) {
            self.audio.activate();
        }
        self.playback = playback;
        self.publish();
        Ok(())
    }

    /// Set the now-playing info from what the session was told. The elapsed time and the
    /// rate are what the OS moves the position on from.
    fn publish(&self) {
        let info = NSMutableDictionary::<NSString, AnyObject>::new();
        let text = |key: &NSString, value: &Option<String>| {
            if let Some(value) = value {
                info.insert(key, &*NSString::from_str(value));
            }
        };
        // SAFETY: MediaPlayer's keys, constant strings.
        unsafe {
            text(MPMediaItemPropertyTitle, &self.metadata.title);
            text(MPMediaItemPropertyArtist, &self.metadata.artist);
            text(MPMediaItemPropertyAlbumTitle, &self.metadata.album);
            if let Some(duration) = self.metadata.duration {
                info.insert(
                    MPMediaItemPropertyPlaybackDuration,
                    &*NSNumber::new_f64(duration.as_secs_f64()),
                );
            }
            if let Some(artwork) = &self.artwork {
                info.insert(MPMediaItemPropertyArtwork, &**artwork);
            }
            if let Some(at) = self.playback.position() {
                info.insert(
                    MPNowPlayingInfoPropertyElapsedPlaybackTime,
                    &*NSNumber::new_f64(at.as_secs_f64()),
                );
            }
            let rate = if matches!(self.playback, Playback::Playing(_)) {
                1.0
            } else {
                0.0
            };
            info.insert(
                MPNowPlayingInfoPropertyPlaybackRate,
                &*NSNumber::new_f64(rate),
            );
        }
        // SAFETY: the shared now-playing centre, given a dictionary of MediaPlayer's keys and
        // the kinds of values they take.
        unsafe {
            let center = MPNowPlayingInfoCenter::defaultCenter();
            center.setNowPlayingInfo(Some(&info));
            // What macOS goes by to list the player as playing; iOS takes it from the audio
            // session.
            center.setPlaybackState(match self.playback {
                Playback::Stopped => MPNowPlayingPlaybackState::Stopped,
                Playback::Paused(_) => MPNowPlayingPlaybackState::Paused,
                Playback::Playing(_) => MPNowPlayingPlaybackState::Playing,
            });
        }
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        // SAFETY: the commands and the targets they gave, and the shared centre.
        unsafe {
            for (command, target) in &self.targets {
                command.removeTarget(Some(target));
                command.setEnabled(false);
            }
            let center = MPNowPlayingInfoCenter::defaultCenter();
            center.setNowPlayingInfo(None);
            center.setPlaybackState(MPNowPlayingPlaybackState::Stopped);
        }
    }
}

/// Turn `command` on, sending the event `event` makes of what it was sent with; a command
/// it makes none of is answered as failed.
fn target(
    command: Retained<MPRemoteCommand>,
    sender: EventSender,
    event: impl Fn(&MPRemoteCommandEvent) -> Option<MediaEvent> + 'static,
) -> (Retained<MPRemoteCommand>, Retained<AnyObject>) {
    let block = RcBlock::new(
        move |sent: NonNull<MPRemoteCommandEvent>| -> MPRemoteCommandHandlerStatus {
            // SAFETY: MediaPlayer calls the block with the event, alive for the call.
            match event(unsafe { sent.as_ref() }) {
                Some(event) => {
                    sender.send(event);
                    MPRemoteCommandHandlerStatus::Success
                }
                None => MPRemoteCommandHandlerStatus::CommandFailed,
            }
        },
    );
    // SAFETY: the command keeps a copy of the block until the target is removed.
    let target = unsafe {
        command.setEnabled(true);
        command.addTargetWithHandler(&block)
    };
    (command, target)
}

/// The cover's bytes as MediaPlayer's artwork: an image of the OS's, handed over at any
/// size asked, at which the OS scales it. `None` where the bytes are no image it reads.
fn artwork(bytes: &[u8]) -> Option<Retained<MPMediaItemArtwork>> {
    let data = NSData::with_bytes(bytes);
    let artwork = image_artwork(&data);
    if artwork.is_none() {
        tracing::warn!("media session: the cover is no image the OS reads");
    }
    artwork
}

#[cfg(target_os = "macos")]
fn image_artwork(data: &NSData) -> Option<Retained<MPMediaItemArtwork>> {
    use objc2::AllocAnyThread;
    use objc2_app_kit::NSImage;
    use objc2_core_foundation::CGSize;

    let image = NSImage::initWithData(NSImage::alloc(), data)?;
    let size = image.size();
    let handler = RcBlock::new(move |_: CGSize| NonNull::from(&*image));
    // SAFETY: the handler returns the image it holds, alive as long as the block.
    Some(unsafe {
        MPMediaItemArtwork::initWithBoundsSize_requestHandler(
            MPMediaItemArtwork::alloc(),
            size,
            &handler,
        )
    })
}

#[cfg(target_os = "ios")]
fn image_artwork(data: &NSData) -> Option<Retained<MPMediaItemArtwork>> {
    use objc2::{AllocAnyThread, msg_send};
    use objc2_core_foundation::CGSize;
    use objc2_ui_kit::UIImage;

    let image = UIImage::imageWithData(data)?;
    // SAFETY: the image's size, read once here.
    let size = unsafe { image.size() };
    let handler = RcBlock::new(move |_: CGSize| -> NonNull<UIImage> { NonNull::from(&*image) });
    // objc2-media-player binds this initialiser for macOS's NSImage only; on iOS it takes a
    // UIImage. SAFETY: the same initialiser, with the block's return a UIImage alive as long
    // as the block.
    unsafe {
        msg_send![
            MPMediaItemArtwork::alloc(),
            initWithBoundsSize: size,
            requestHandler: &*handler
        ]
    }
}

#[cfg(target_os = "ios")]
mod ios {
    //! The audio session: what makes an iOS app the one the lock screen and the Control
    //! Centre show, and what tells it of the calls and the headphones.

    use std::io;
    use std::ptr::NonNull;

    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, ProtocolObject};
    use objc2_avf_audio::{
        AVAudioSession, AVAudioSessionCategoryPlayback, AVAudioSessionInterruptionNotification,
        AVAudioSessionInterruptionOptionKey, AVAudioSessionInterruptionTypeKey,
        AVAudioSessionRouteChangeNotification, AVAudioSessionRouteChangeReasonKey,
    };
    use objc2_foundation::{
        NSDictionary, NSNotification, NSNotificationCenter, NSNumber, NSObjectProtocol, NSString,
    };

    use crate::media_session::{EventSender, MediaEvent};

    /// `AVAudioSessionInterruptionTypeBegan`.
    const INTERRUPTION_BEGAN: usize = 1;
    /// `AVAudioSessionInterruptionOptionShouldResume`.
    const SHOULD_RESUME: usize = 1;
    /// `AVAudioSessionRouteChangeReasonOldDeviceUnavailable`: the headphones went.
    const OLD_DEVICE_UNAVAILABLE: usize = 2;

    pub(super) struct AudioSession {
        session: Retained<AVAudioSession>,
        active: bool,
        observers: Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
    }

    impl AudioSession {
        /// The shared session in the playback category, which plays with the ringer
        /// switch on silent and, with `UIBackgroundModes` `audio`, in the background. Not
        /// made active yet: that would stop another app's music as the player opens.
        pub(super) fn new(sender: EventSender) -> io::Result<AudioSession> {
            // SAFETY: the shared session and its category's constant.
            let session = unsafe { AVAudioSession::sharedInstance() };
            if let Some(category) = unsafe { AVAudioSessionCategoryPlayback } {
                unsafe { session.setCategory_error(category) }
                    .map_err(|err| io::Error::other(err.localizedDescription().to_string()))?;
            }
            let observers = [
                // SAFETY: AVFoundation's notification names.
                unsafe { AVAudioSessionInterruptionNotification }.map(|name| {
                    observe(name, sender.clone(), |info| {
                        // SAFETY: the keys of the interruption's userInfo.
                        let kind = number(info, unsafe { AVAudioSessionInterruptionTypeKey })?;
                        Some(if kind == INTERRUPTION_BEGAN {
                            MediaEvent::Interrupted
                        } else {
                            let options =
                                number(info, unsafe { AVAudioSessionInterruptionOptionKey })
                                    .unwrap_or(0);
                            MediaEvent::InterruptionEnded {
                                resume: options & SHOULD_RESUME != 0,
                            }
                        })
                    })
                }),
                unsafe { AVAudioSessionRouteChangeNotification }.map(|name| {
                    observe(name, sender.clone(), |info| {
                        // SAFETY: the key of the route change's userInfo.
                        let reason = number(info, unsafe { AVAudioSessionRouteChangeReasonKey })?;
                        (reason == OLD_DEVICE_UNAVAILABLE).then_some(MediaEvent::OutputRemoved)
                    })
                }),
            ]
            .into_iter()
            .flatten()
            .collect();
            Ok(AudioSession {
                session,
                active: false,
                observers,
            })
        }

        /// Make the session active, once, as playback first starts.
        pub(super) fn activate(&mut self) {
            if self.active {
                return;
            }
            // SAFETY: the shared session.
            match unsafe { self.session.setActive_error(true) } {
                Ok(()) => self.active = true,
                Err(err) => tracing::warn!(
                    "media session: the audio session would not start: {}",
                    err.localizedDescription()
                ),
            }
        }
    }

    impl Drop for AudioSession {
        fn drop(&mut self) {
            let center = NSNotificationCenter::defaultCenter();
            for observer in &self.observers {
                // SAFETY: an observer the centre gave.
                unsafe { center.removeObserver(AsRef::<AnyObject>::as_ref(&**observer)) };
            }
        }
    }

    /// Send the event `event` makes of the userInfo of each `name` notification. Posted on
    /// the thread the session changed on, which does not matter: the block only sends.
    fn observe(
        name: &NSString,
        sender: EventSender,
        event: impl Fn(&NSDictionary) -> Option<MediaEvent> + 'static,
    ) -> Retained<ProtocolObject<dyn NSObjectProtocol>> {
        let block = RcBlock::new(move |notification: NonNull<NSNotification>| {
            // SAFETY: the notification, alive for the call.
            let info = unsafe { notification.as_ref() }.userInfo();
            if let Some(event) = info.as_deref().and_then(&event) {
                sender.send(event);
            }
        });
        // SAFETY: no object to match and no queue: the block runs where it is posted.
        unsafe {
            NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
                Some(name),
                None,
                None,
                &block,
            )
        }
    }

    /// The unsigned number under `key` in a notification's userInfo.
    fn number(info: &NSDictionary, key: Option<&NSString>) -> Option<usize> {
        let key: &AnyObject = key?;
        let value = info.objectForKey(key)?;
        Some(value.downcast::<NSNumber>().ok()?.as_usize())
    }
}
