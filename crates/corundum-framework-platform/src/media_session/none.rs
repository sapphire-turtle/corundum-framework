//! An OS with no media session here: Android (its MediaSession, which needs a foreground
//! service to outlive the activity, is still to come) and any OS without a backend. The
//! session is made unconnected and takes everything without a word.

use std::io;

use raw_window_handle::HasWindowHandle;

use super::{EventSender, MediaSessionSpec, Metadata, Playback};

pub(super) enum Backend {}

impl Backend {
    pub(super) fn new(
        _spec: &MediaSessionSpec<'_>,
        _window: Option<&dyn HasWindowHandle>,
        _sender: EventSender,
    ) -> io::Result<Option<Backend>> {
        Ok(None)
    }

    pub(super) fn set_metadata(&mut self, _metadata: &Metadata) -> io::Result<()> {
        match *self {}
    }

    pub(super) fn set_playback(&mut self, _playback: Playback) -> io::Result<()> {
        match *self {}
    }
}
