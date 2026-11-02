# corundum-framework-audio

Decode and resample audio, with no GUI in the way.

- `decode::open(Box<dyn Source>, hint)`: a `TrackDecoder` of interleaved `f32` blocks, with
  seeking and the length. FLAC, MP3, AAC, ALAC, Vorbis, MP4, WAV (symphonia) and Ogg Opus
  (ogg + opus-pure).
- `decode::probe(Box<dyn Source>, hint)`: the title, artist, album and length, read without
  decoding any audio.
- `resample::Converter`: channels first, then the rate with rubato, to an `OutputConfig`.
