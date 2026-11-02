//! Play audio without a GUI: decoding, resampling, the playback engine and its device,
//! with no egui involved.
//!
//! - [`decode`]: [`decode::open`] turns a [`decode::Source`] (a file, or anything seekable)
//!   into a [`decode::TrackDecoder`] of interleaved `f32` blocks, with seeking and the
//!   length. symphonia reads FLAC, MP3, AAC, ALAC, Vorbis, MP4 and WAV; ogg + opus-pure
//!   read Ogg Opus. [`decode::probe`] reads only the tags and the length.
//! - [`Engine`]: the playback state machine over a queue of any [`QueueItem`], an
//!   [`Opener`] and an [`OutputFactory`].
//! - [`feed`]: the feeds that decode a track on a thread of its own and hand it to the engine.
//! - [`FileOpener`]: an [`Opener`] for files on disk.
//! - [`Player`]: the engine on a thread, driven by [`Command`]s and read as a [`Status`].
//! - `CpalOutputs` (feature `cpal`): the default sound device, as an [`OutputFactory`].
//! - [`resample`]: [`resample::Converter`] turns decoded blocks into an [`OutputConfig`]:
//!   channels first, then the rate with rubato.

#[cfg(feature = "cpal")]
mod cpal;
pub mod decode;
mod engine;
#[cfg(any(test, feature = "test-support"))]
pub mod fakes;
pub mod feed;
mod file;
mod output;
mod player;
pub mod resample;

#[cfg(feature = "cpal")]
pub use crate::cpal::CpalOutputs;
pub use engine::{
    AudioOutput, Command, Engine, Feed, FeedEvent, MAX_SKIPS, Opener, OutputFactory, PlayerError,
    PlayerState, QueueItem, RESTART_AFTER_MS, Status, TrackError, Wait,
};
pub use file::FileOpener;
pub use output::OutputConfig;
pub use player::Player;
