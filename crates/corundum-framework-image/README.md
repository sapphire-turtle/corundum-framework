# corundum-framework-image

Decode an image to pixels, with no GUI in the way. Give it a file or bytes; it gives back
RGBA8 with the EXIF orientation applied.

- `decode(Source, &Limits)`: the format is read from the bytes (the extension only breaks
  a tie), the size is checked from the header before any pixel is allocated, and a decoder
  that panics on a bad file becomes an `Error::Decode`.
- `Limits`: at most 512 MiB of file and 200 million pixels by default.
- JPEG, PNG, GIF, TIFF and WebP decode out of the box. HEIC, HEIF and AVIF are
  `Error::Unsupported`.

| Feature | What it adds |
| ------- | ------------ |
| (none)  | JPEG, PNG, GIF, TIFF and WebP decoding |
| `webp`  | `encode_webp(&Image, quality)`: lossy WebP, through libwebp built from its bundled C sources (BSD-3-Clause) |
| `raw`   | Camera RAW files decode to their embedded preview, through [rawler](https://crates.io/crates/rawler) |

Both are off by default.

### The `raw` feature

rawler is licensed under **LGPL-2.1**. This crate stays MIT or Apache-2.0, but an app
that turns `raw` on links rawler in and takes on its terms (among them, letting users
relink against a changed rawler, and shipping its licence and notices).

A RAW file is told by its content where rawler can, else by its extension. It decodes to
the embedded preview rawler picks (the large one, where a file has several), not a
developed raw image; the RAW's own orientation is then applied. `Limits` bound the RAW
file's size and the preview's pixels, but the preview is measured after rawler decodes it.

| Family | Status |
| ------ | ------ |
| DNG | Tested: a DNG with a JPEG preview, written by rawler's DNG writer in the tests |
| CR2, CR3, NEF, ARW, RW2, RAF | Read through rawler's preview support; not tested here |
| ORF | `Error::Unsupported`: rawler has no preview for it |

Without the feature, files with these extensions are `Error::Unsupported`.

```rust,no_run
use corundum_framework_image::{decode, Limits, Source};

let image = decode(Source::Path("photo.jpg".as_ref()), &Limits::default())?;
println!("{}x{}, {} bytes", image.width, image.height, image.rgba.len());
# Ok::<(), corundum_framework_image::Error>(())
```

Licensed under MIT or Apache-2.0, at your option.
