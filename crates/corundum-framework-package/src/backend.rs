//! The calls into Velopack, behind a trait so the updater's states are tested with a fake.

use std::convert::Infallible;
use std::ffi::OsString;
use std::path::Path;
use std::sync::mpsc;

use corundum_framework_platform::package;
use semver::Version;
use velopack::locator::{LocationContext, VelopackLocatorConfig, auto_locate_app_manifest};
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
        let located = locate()?.config;
        let manager = match source {
            Source::Github { repo, prerelease } => {
                // Velopack panics on a repository that is not a URL.
                url::Url::parse(repo.trim_end_matches('/')).map_err(|e| {
                    Error::Velopack(format!("the repository {repo:?} is not a URL: {e}"))
                })?;
                UpdateManager::new(GithubSource::new(repo, None, *prerelease), None, located)
            }
            Source::Http(url) => UpdateManager::new(HttpSource::new(url), None, located),
        }
        .map_err(from_velopack)?;
        Ok(Velopack { manager })
    }
}

/// The running package, as Velopack's paths and the version in its manifest.
pub(crate) struct Located {
    /// The paths for `VelopackApp::set_locator` and `UpdateManager::new`; `None` on
    /// Windows, where Velopack's own search finds the same.
    pub(crate) config: Option<VelopackLocatorConfig>,
    pub(crate) version: Version,
}

/// Finds the running package from the executable with its links resolved
/// ([`package::current_exe`]). Velopack's own search looks at the path the process was
/// started by, which on macOS may be the command line's `/usr/local/bin` link, outside the
/// `.app`: the app would not be seen as a package, and its updates would fail.
pub(crate) fn locate() -> Result<Located, Error> {
    let exe = package::current_exe().ok_or(Error::NotPackaged)?;
    locate_from(&exe)
}

/// [`locate`] for a given executable, links already resolved.
pub(crate) fn locate_from(exe: &Path) -> Result<Located, Error> {
    let locator = auto_locate_app_manifest(LocationContext::FromSpecifiedAppExecutable(
        exe.to_path_buf(),
    ))
    .map_err(from_velopack)?;
    let version = locator.get_manifest_version();
    // Windows: Velopack's search from the executable is the same, and it moves the packages
    // directory when the install directory is not writable, which a round trip through the
    // getters would only repeat.
    #[cfg(windows)]
    let config = None;
    #[cfg(not(windows))]
    let config = {
        let bin_dir = locator.get_current_bin_dir();
        // The `.app` on macOS, the AppImage file on Linux.
        #[cfg(target_os = "linux")]
        let root = locator.get_appimage_path();
        #[cfg(not(target_os = "linux"))]
        let root = locator.get_root_dir();
        // The locator does not give the manifest's path back: it is where its search found
        // it, beside the binaries, or in a bundle's `Contents/Resources` on macOS.
        let manifest = [
            bin_dir.join("sq.version"),
            root.join("Contents").join("Resources").join("sq.version"),
        ]
        .into_iter()
        .find(|path| path.exists())
        .unwrap_or_else(|| bin_dir.join("sq.version"));
        Some(VelopackLocatorConfig {
            RootAppDir: root,
            UpdateExePath: locator.get_update_path(),
            PackagesDir: locator.get_packages_dir(),
            ManifestPath: manifest,
            CurrentBinaryDir: bin_dir,
            IsPortable: locator.get_is_portable(),
        })
    };
    Ok(Located { config, version })
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

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::fs;

    use velopack::locator::VelopackLocator;

    use super::*;

    #[test]
    fn a_located_appimage_gives_velopack_paths_it_accepts() {
        let tmp = tempfile::tempdir().unwrap();
        let tmp = tmp.path().canonicalize().unwrap();
        let bin = tmp.join("mount/usr/bin");
        fs::create_dir_all(&bin).unwrap();
        fs::write(bin.join("UpdateNix"), "").unwrap();
        fs::write(
            bin.join("sq.version"),
            "<package><metadata><id>demo</id><version>1.2.3</version>\
             <mainExe>demo</mainExe></metadata></package>",
        )
        .unwrap();
        fs::write(bin.join("demo"), "").unwrap();
        let image = tmp.join("Demo.AppImage");
        fs::write(&image, "").unwrap();
        // SAFETY: no other test in this binary reads or writes `APPIMAGE`.
        unsafe { std::env::set_var("APPIMAGE", &image) };

        let located = locate_from(&bin.join("demo")).unwrap();
        assert_eq!(located.version, Version::new(1, 2, 3));
        let config = located.config.unwrap();
        assert_eq!(config.RootAppDir, image);
        assert_eq!(config.ManifestPath, bin.join("sq.version"));
        let locator = VelopackLocator::new(&config).unwrap();
        assert_eq!(locator.get_manifest_version(), Version::new(1, 2, 3));
        assert_eq!(locator.get_main_exe_path(), bin.join("demo"));
    }
}
