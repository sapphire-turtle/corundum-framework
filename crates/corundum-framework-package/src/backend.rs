//! The calls into Velopack, behind a trait so the updater's states are tested with a fake.

use std::convert::Infallible;
use std::ffi::OsString;
use std::sync::mpsc;

use semver::Version;
use velopack::sources::{GithubSource, HttpSource};
use velopack::{UpdateCheck, UpdateInfo, UpdateManager};

use crate::error::Error;
use crate::updater::{After, Source};

/// What an apply installs.
// Built once per apply and moved straight in: boxing the info would buy nothing.
#[allow(clippy::large_enum_variant)]
pub(crate) enum Applied {
    /// The update this run downloaded.
    Downloaded(UpdateInfo),
    /// The newest package downloaded by an earlier run and never applied.
    Pending,
}

/// What the updater needs from Velopack.
pub(crate) trait Backend: Send + Sync {
    /// The version of the running package.
    fn current(&self) -> Version;
    /// A newer version in the feed, or `None`.
    fn check(&self) -> Result<Option<UpdateInfo>, Error>;
    /// Fetches `info`'s package, telling `progress` how far it is, 0 to 100.
    fn download(&self, info: &UpdateInfo, progress: &dyn Fn(u8)) -> Result<(), Error>;
    /// The version of a package downloaded earlier and newer than the running one.
    fn pending(&self) -> Option<Version>;
    /// Hands `applied` to Velopack's updater and exits; returns only on an error.
    fn apply(&self, applied: Applied, after: &After) -> Result<Infallible, Error>;
}

/// The real Velopack, over its `UpdateManager`.
pub(crate) struct Velopack {
    manager: UpdateManager,
}

impl Velopack {
    /// `Error::NotPackaged` when the running executable is not in a package.
    pub(crate) fn new(source: &Source) -> Result<Velopack, Error> {
        let manager = match source {
            Source::Github { repo, prerelease } => {
                UpdateManager::new(GithubSource::new(repo, None, *prerelease), None, None)
            }
            Source::Http(url) => UpdateManager::new(HttpSource::new(url), None, None),
        }
        .map_err(from_velopack)?;
        Ok(Velopack { manager })
    }
}

impl Backend for Velopack {
    fn current(&self) -> Version {
        self.manager.get_current_version()
    }

    fn check(&self) -> Result<Option<UpdateInfo>, Error> {
        match self.manager.check_for_updates().map_err(from_velopack)? {
            UpdateCheck::UpdateAvailable(info) => Ok(Some(*info)),
            UpdateCheck::RemoteIsEmpty | UpdateCheck::NoUpdateAvailable => Ok(None),
        }
    }

    fn download(&self, info: &UpdateInfo, progress: &dyn Fn(u8)) -> Result<(), Error> {
        let (tx, rx) = mpsc::channel::<i16>();
        // Velopack reports on the sender while it downloads, so the download runs on a
        // thread of its own and this one forwards; the channel closes when it returns.
        std::thread::scope(|scope| {
            let download = scope.spawn(move || self.manager.download_updates(info, Some(tx)));
            for percent in rx {
                progress(percent.clamp(0, 100) as u8);
            }
            download
                .join()
                .unwrap_or_else(|_| Err(velopack::Error::Other("the download panicked".into())))
        })
        .map_err(from_velopack)
    }

    fn pending(&self) -> Option<Version> {
        let asset = self.manager.get_update_pending_restart()?;
        Version::parse(&asset.Version).ok()
    }

    fn apply(&self, applied: Applied, after: &After) -> Result<Infallible, Error> {
        let asset = match applied {
            Applied::Downloaded(info) => info.TargetFullRelease,
            Applied::Pending => self
                .manager
                .get_update_pending_restart()
                .ok_or_else(|| Error::Velopack("nothing downloaded".into()))?,
        };
        let (restart, args): (bool, &[OsString]) = match after {
            After::RestartApp => (true, &[]),
            After::RestartWith(args) => (true, args),
            After::Exit => (false, &[]),
        };
        self.manager
            .wait_exit_then_apply_updates(&asset, false, restart, args)
            .map_err(from_velopack)?;
        // Velopack's updater waits for this process to exit before it replaces the files.
        std::process::exit(0)
    }
}

/// Velopack's error as the crate's: not installed is not packaged, the network is the
/// network, anything else is Velopack's.
fn from_velopack(e: velopack::Error) -> Error {
    match e {
        velopack::Error::NotInstalled(_) => Error::NotPackaged,
        velopack::Error::Network(e) => Error::Network(e.to_string()),
        e => Error::Velopack(e.to_string()),
    }
}
