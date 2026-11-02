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

```rust,no_run
use corundum_framework_image::{decode, Limits, Source};

let image = decode(Source::Path("photo.jpg".as_ref()), &Limits::default())?;
println!("{}x{}, {} bytes", image.width, image.height, image.rgba.len());
# Ok::<(), corundum_framework_image::Error>(())
```

Licensed under MIT or Apache-2.0, at your option.
