//! What can go wrong with a store.

use std::path::PathBuf;

/// An error from the store or its users.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A file or the directory could not be read or written.
    #[error("{}: {source}", path.display())]
    Io {
        /// The file or directory.
        path: PathBuf,
        /// What the OS said.
        source: std::io::Error,
    },
    /// A record could not be written or imported as given.
    #[error("{}: {message}", path.display())]
    Parse {
        /// The file.
        path: PathBuf,
        /// What was wrong.
        message: String,
    },
    /// No key has this id or name.
    #[error("no key is called {0:?}")]
    NotFound(String),
    /// More than one key has this name: give the id.
    #[error("more than one key is called {0:?}; give its id")]
    Ambiguous(String),
    /// An unretired key already has this name.
    #[error("a key called {0:?} already exists")]
    NameTaken(String),
    /// A key already has this id.
    #[error("a key with the id {0:?} already exists")]
    IdTaken(String),
    /// Not a key id: ASCII letters, digits, `-` and `_` only.
    #[error("{0:?} is not a key id")]
    InvalidId(String),
    /// A profile id cannot be empty.
    #[error("a profile id cannot be empty")]
    InvalidProfile(String),
    /// Not one of the profiles the app offers.
    #[error("{id:?} is not a profile; choose one of: {}", known_list(known))]
    UnknownProfile {
        /// The id given.
        id: String,
        /// The ids the app offers.
        known: Vec<crate::ProfileId>,
    },
}

fn known_list(known: &[crate::ProfileId]) -> String {
    if known.is_empty() {
        "(none)".to_owned()
    } else {
        known
            .iter()
            .map(|p| p.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// The store's result.
pub type Result<T> = std::result::Result<T, Error>;
