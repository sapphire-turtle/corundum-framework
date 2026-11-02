/// Why an image could not be decoded.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A format this build does not decode (HEIC, HEIF, AVIF, or one `image` was built without).
    #[error("unsupported image format")]
    Unsupported,
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
