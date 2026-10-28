//! Windows: the window's SystemMediaTransportControls, which the media keys and the
//! volume flyout's player drive.
//!
//! A desktop app has no controls of its own, as a store app does: they are asked for the
//! window, through `ISystemMediaTransportControlsInterop`. The buttons and seeks come on a
//! thread of the OS's pool and go to the app as events.

use std::io;
use std::time::Duration;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Foundation::{TimeSpan, TypedEventHandler};
use windows::Media::{
    MediaPlaybackStatus, MediaPlaybackType, PlaybackPositionChangeRequestedEventArgs,
    SystemMediaTransportControls, SystemMediaTransportControlsButton,
    SystemMediaTransportControlsButtonPressedEventArgs, SystemMediaTransportControlsDisplayUpdater,
    SystemMediaTransportControlsTimelineProperties,
};
use windows::Storage::Streams::{
    DataWriter, InMemoryRandomAccessStream, RandomAccessStreamReference,
};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::WinRT::ISystemMediaTransportControlsInterop;
use windows::core::{HSTRING, Ref};

use super::{Cover, EventSender, MediaEvent, MediaSessionSpec, Metadata, Playback};

pub(super) struct Backend {
    controls: SystemMediaTransportControls,
    display: SystemMediaTransportControlsDisplayUpdater,
    timeline: SystemMediaTransportControlsTimelineProperties,
    button_token: i64,
    position_token: i64,
}

impl Backend {
    pub(super) fn new(
        _spec: &MediaSessionSpec<'_>,
        window: Option<&dyn HasWindowHandle>,
        sender: EventSender,
    ) -> io::Result<Option<Backend>> {
        let handle = window
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no window"))?
            .window_handle()
            .map_err(|err| io::Error::other(err.to_string()))?;
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "not a Win32 window",
            ));
        };
        let hwnd = HWND(handle.hwnd.get() as *mut std::ffi::c_void);
        Ok(Some(
            Backend::for_window(hwnd, sender).map_err(io::Error::other)?,
        ))
    }

    fn for_window(hwnd: HWND, sender: EventSender) -> windows::core::Result<Backend> {
        let interop = windows::core::factory::<
            SystemMediaTransportControls,
            ISystemMediaTransportControlsInterop,
        >()?;
        // SAFETY: `hwnd` is the live window the session was made for.
        let controls: SystemMediaTransportControls = unsafe { interop.GetForWindow(hwnd) }?;
        controls.SetIsEnabled(true)?;
        controls.SetIsPlayEnabled(true)?;
        controls.SetIsPauseEnabled(true)?;
        controls.SetIsStopEnabled(true)?;
        controls.SetIsNextEnabled(true)?;
        controls.SetIsPreviousEnabled(true)?;
        controls.SetPlaybackStatus(MediaPlaybackStatus::Closed)?;
        let display = controls.DisplayUpdater()?;
        display.SetType(MediaPlaybackType::Music)?;

        let buttons = sender.clone();
        let button_token = controls.ButtonPressed(&TypedEventHandler::new(
            move |_, args: Ref<'_, SystemMediaTransportControlsButtonPressedEventArgs>| {
                if let Some(event) = button_event(args.ok()?.Button()?) {
                    buttons.send(event);
                }
                Ok(())
            },
        ))?;
        let position_token = controls.PlaybackPositionChangeRequested(&TypedEventHandler::new(
            move |_, args: Ref<'_, PlaybackPositionChangeRequestedEventArgs>| {
                let to = Duration::from(args.ok()?.RequestedPlaybackPosition()?);
                sender.send(MediaEvent::SeekTo(to));
                Ok(())
            },
        ))?;
        Ok(Backend {
            controls,
            display,
            timeline: SystemMediaTransportControlsTimelineProperties::new()?,
            button_token,
            position_token,
        })
    }

    pub(super) fn set_metadata(&mut self, metadata: &Metadata) -> io::Result<()> {
        self.show_metadata(metadata).map_err(io::Error::other)
    }

    fn show_metadata(&mut self, metadata: &Metadata) -> windows::core::Result<()> {
        let music = self.display.MusicProperties()?;
        let text = |s: &Option<String>| HSTRING::from(s.as_deref().unwrap_or_default());
        music.SetTitle(&text(&metadata.title))?;
        music.SetArtist(&text(&metadata.artist))?;
        music.SetAlbumTitle(&text(&metadata.album))?;
        let cover = match &metadata.cover {
            Some(Cover::Bytes(bytes)) => Some(image_stream(bytes)?),
            Some(Cover::Path(path)) => match std::fs::read(path) {
                Ok(bytes) => Some(image_stream(&bytes)?),
                Err(err) => {
                    tracing::warn!("media session: cannot read the cover {path:?}: {err}");
                    None
                }
            },
            None => None,
        };
        // A track without a cover takes the last track's away.
        match cover {
            Some(cover) => self.display.SetThumbnail(&cover)?,
            None => self.display.SetThumbnail(None)?,
        }
        self.display.Update()?;

        let length = TimeSpan::from(metadata.duration.unwrap_or_default());
        self.timeline.SetStartTime(TimeSpan::default())?;
        self.timeline.SetMinSeekTime(TimeSpan::default())?;
        self.timeline.SetEndTime(length)?;
        self.timeline.SetMaxSeekTime(length)?;
        self.timeline.SetPosition(TimeSpan::default())?;
        self.controls.UpdateTimelineProperties(&self.timeline)
    }

    pub(super) fn set_playback(&mut self, playback: Playback) -> io::Result<()> {
        self.show_playback(playback).map_err(io::Error::other)
    }

    fn show_playback(&mut self, playback: Playback) -> windows::core::Result<()> {
        self.controls.SetPlaybackStatus(match playback {
            Playback::Stopped => MediaPlaybackStatus::Stopped,
            Playback::Paused(_) => MediaPlaybackStatus::Paused,
            Playback::Playing(_) => MediaPlaybackStatus::Playing,
        })?;
        self.timeline
            .SetPosition(TimeSpan::from(playback.position().unwrap_or_default()))?;
        self.controls.UpdateTimelineProperties(&self.timeline)
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        let _ = self.controls.RemoveButtonPressed(self.button_token);
        let _ = self
            .controls
            .RemovePlaybackPositionChangeRequested(self.position_token);
        let _ = self.display.ClearAll();
        let _ = self.display.Update();
        let _ = self.controls.SetIsEnabled(false);
    }
}

/// The event a button of the controls asks for; the ones the session does not turn on
/// (fast-forward, rewind, the channel buttons) ask for none.
fn button_event(button: SystemMediaTransportControlsButton) -> Option<MediaEvent> {
    Some(match button {
        SystemMediaTransportControlsButton::Play => MediaEvent::Play,
        SystemMediaTransportControlsButton::Pause => MediaEvent::Pause,
        SystemMediaTransportControlsButton::Stop => MediaEvent::Stop,
        SystemMediaTransportControlsButton::Next => MediaEvent::Next,
        SystemMediaTransportControlsButton::Previous => MediaEvent::Previous,
        _ => return None,
    })
}

/// An image's bytes as the stream a thumbnail is made from. Written to memory, so the wait
/// for the write is short.
fn image_stream(bytes: &[u8]) -> windows::core::Result<RandomAccessStreamReference> {
    let stream = InMemoryRandomAccessStream::new()?;
    let writer = DataWriter::CreateDataWriter(&stream)?;
    writer.WriteBytes(bytes)?;
    writer.StoreAsync()?.join()?;
    writer.DetachStream()?;
    stream.Seek(0)?;
    RandomAccessStreamReference::CreateFromStream(&stream)
}
