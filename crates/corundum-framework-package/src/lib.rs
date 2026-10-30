//! A desktop app as a Velopack package.
//!
//! [`start`] is the first thing an app's `main` runs: Velopack's own start, the Windows
//! install and uninstall hooks, and which [`Start`] this is (the first, the first after an
//! update, or neither), told from the version the app last ran with. [`Updater`] finds and
//! downloads updates by itself and applies one only when asked, after the app's
//! `before_apply`.
//!
//! Velopack packages the desktops only; on iOS and Android the crate is empty.

#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod backend;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod cli_path;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod console;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod error;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod services;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod start;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod updater;

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use cli_path::CliSpec;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use error::Error;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use semver::Version;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use start::{PackageSpec, Start, start};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use updater::{After, Source, Updater};
