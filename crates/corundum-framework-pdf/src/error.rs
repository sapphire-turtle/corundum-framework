/// Why a PDF has no picture.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// The file needs a password to open.
    #[error("the PDF is password protected")]
    Encrypted,
    /// The file is not a PDF, or too damaged to read, or the renderer failed on it.
    #[error("cannot read the PDF: {0}")]
    Broken(String),
    /// The input is larger than [`MAX_INPUT`](crate::MAX_INPUT).
    #[error("the PDF is too large")]
    TooLarge,
    /// The file uses something this crate cannot draw.
    #[error("unsupported PDF: {0}")]
    Unsupported(String),
}
