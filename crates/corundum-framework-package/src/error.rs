/// What can go wrong with a package's updates.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The app is not running from a package, so there is nothing to update.
    #[error("the app is not installed from a package")]
    NotPackaged,
    /// The update feed could not be reached.
    #[error("network error: {0}")]
    Network(String),
    /// Velopack itself failed.
    #[error("Velopack error: {0}")]
    Velopack(String),
    /// The app's `before_apply` refused the update.
    #[error("could not prepare for the update: {0}")]
    Prepare(String),
    /// Another program has the command's name: a file that is not a link, left alone.
    #[error("another {command} is installed at {}", path.display())]
    Taken {
        command: String,
        path: std::path::PathBuf,
    },
    /// The command line could not be put on or taken off `PATH`.
    #[error("could not change the command line on PATH: {0}")]
    CliPath(String),
    /// A file could not be read or written.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
