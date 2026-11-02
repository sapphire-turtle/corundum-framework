//! Play audio without a GUI: decoding, resampling and (in later pieces) the playback
//! engine, with no egui involved.
//!
//! - [`decode`]: [`decode::open`] turns a [`decode::Source`] (a file, or anything seekable)
//!   into a [`decode::TrackDecoder`] of interleaved `f32` blocks, with seeking and the
//!   length. symphonia reads FLAC, MP3, AAC, ALAC, Vorbis, MP4 and WAV; ogg + opus-pure
//!   read Ogg Opus. [`decode::probe`] reads only the tags and the length.
//! - [`resample`]: [`resample::Converter`] turns decoded blocks into an [`OutputConfig`]:
//!   channels first, then the rate with rubato.

pub mod decode;
mod output;
pub mod resample;

pub use output::OutputConfig;
