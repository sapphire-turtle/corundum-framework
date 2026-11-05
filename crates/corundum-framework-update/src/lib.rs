//! Newer releases of an app's binary: finding them, and installing one.
//!
//! A binary's releases are a GitHub repository's, tagged `<prefix><version>`
//! ([`ReleaseSource`]). [`latest_release`] finds the newest; the app server asks it once a
//! day and shows the answer in `status`. The `update` command (`UpdateArgs`) goes one step
//! further and replaces the running executable with the release's, but only:
//!
//! - with a newer version: never a downgrade;
//! - with the asset for this platform, `<asset>-<os>-<arch>[.exe]`, downloaded over HTTPS
//!   from the repository's own release, and matching the SHA-256 GitHub reports for it;
//! - when the source names a minisign key ([`ReleaseSource::pubkey`]), only with a file whose
//!   signature, `<file>.sig` beside it in the release, is that key's and names the file: then
//!   the key, not GitHub, is the authority on what was released;
//! - when the executable was placed by hand, not by `cargo install`, `cargo binstall` or a
//!   package manager, whose own bookkeeping a replaced file would contradict ([`InstallMethod`]).
//!
//! Downloading, verifying and replacing are behind the `self-update` feature (on by default), the
//! command line (`UpdateArgs`, `run`) behind `cli`; without `self-update` the crate only looks
//! releases up and words [`advice`].
//!
//! Restarting afterwards is the caller's: the CLI says so, and the GUI panel restarts the
//! app, its services or a bridge itself.

#![warn(missing_docs)]

mod install;
mod package;
mod release;
#[cfg(feature = "self-update")]
mod replace;
#[cfg(feature = "self-update")]
mod verify;

#[cfg(feature = "cli")]
pub use install::UpdateArgs;
#[cfg(all(feature = "cli", feature = "self-update"))]
pub use install::run;
pub use install::{
    BinstallSource, InstallMethod, StoreKind, advice, advice_command, asset_name, cargo_home,
    install_method, platform,
};
#[cfg(feature = "self-update")]
pub use install::{download_asset, install};
pub use package::{PackageState, PackageStatus};
pub use release::{Asset, Release, ReleaseSource, client, latest_release};
#[cfg(feature = "self-update")]
pub use replace::replace_file;
#[cfg(feature = "self-update")]
pub use verify::verify_signature;

/// Errors from finding, fetching or installing a release.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Talking to GitHub failed.
    #[error("{0}")]
    Http(String),
    /// The repository has no release this source matches.
    #[error("{0}")]
    NoRelease(String),
    /// The release has no file for this platform.
    #[error("{0}")]
    NoAsset(String),
    /// The release file's signature is missing, unreadable, made with another key, or made
    /// for another file.
    #[error("{0}")]
    Signature(String),
    /// The binary does not say where its releases are, or its own version is unusable.
    #[error("{0}")]
    NotConfigured(String),
    /// Reading the executable or writing its replacement failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// This crate's result.
pub type Result<T> = std::result::Result<T, Error>;
