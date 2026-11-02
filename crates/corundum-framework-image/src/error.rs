/// Why an image could not be decoded (or, with the `webp` feature, encoded).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A format this build does not decode (HEIC, HEIF, AVIF, ORF, a RAW file without the
    /// `raw` feature, or one `image` was built without).
    #[error("unsupported image format")]
    Unsupported,
    /// The pixels could not be encoded.
    #[error("could not encode the image: {0}")]
    Encode(String),
    /// The bytes are not a valid image of their format.
    #[error("could not decode the image: {0}")]
    Decode(String),
    /// The file or the image's pixels exceed the [`Limits`](crate::Limits).
    #[error("the image is too large")]
    TooLarge,
    /// The file could not be read.
    #[error("could not read the image: {0}")]
    Io(#[from] std::io::Error),
}
