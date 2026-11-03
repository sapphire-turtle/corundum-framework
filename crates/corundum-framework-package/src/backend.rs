//! The calls into cargo-packager's updater, behind a trait so the updater's states are
//! tested with a fake.
//!
//! cargo-packager-updater reads the feed, downloads the update and checks its minisign
//! signature against the app's public key. What it downloaded is kept in the app's cache
//! directory until it is applied, so an update downloaded by one run can be applied at the
//! next start ([`Backend::pending`]). Applying is this crate's on Windows, where the
//! updater's own install ends the process at once: the installer runs only once the app
//! has exited, so that a GUI closes its window and saves first.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use cargo_packager_updater::{Config, Update, UpdateFormat, UpdaterBuilder};
use corundum_framework_platform::dirs::AppDirs;
use corundum_framework_platform::package;
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::start::PackageSpec;
use crate::updater::{After, Source};

/// Set for a process this crate starts again after applying an update: its start applies
/// nothing pending, as an update that failed would be tried at every start.
pub(crate) const RESTART: &str = "CORUNDUM_PACKAGE_RESTART";

/// An update the feed offers: its version, and what downloading it needs.
#[derive(Clone, Debug)]
pub(crate) struct Found {
    pub(crate) version: Version,
    /// `None` in tests.
    pub(crate) update: Option<Box<Update>>,
}

impl Found {
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn version(version: Version) -> Found {
        Found {
            version,
            update: None,
        }
    }
}

/// What an apply installs.
pub(crate) enum Applied {
    /// The update this run downloaded.
    Downloaded(Found),
    /// The update downloaded by an earlier run and never applied.
    Pending,
}

/// What the updater needs from the package's updates.
pub(crate) trait Backend: Send + Sync {
    /// The version of the running package.
    fn current(&self) -> Version;
    /// A newer version in the feed, or `None`.
    fn check(&self) -> Result<Option<Found>, Error>;
    /// Fetches and verifies `found`'s package, telling `progress` how far it is, 0 to 100,
    /// and keeps it until it is applied.
    fn download(&self, found: &Found, progress: &dyn Fn(u8)) -> Result<(), Error>;
    /// The version of a package downloaded earlier and newer than the running one.
    fn pending(&self) -> Option<Version>;
    /// Installs `applied` once this process ends (Windows) or at once (macOS, Linux), and
    /// starts the app again for a restart. Exits for [`After::Exit`]; for a restart returns
    /// `Ok`, and the caller ends the process.
    fn apply(&self, applied: Applied, after: &After) -> Result<(), Error>;
}

/// The feed's file on a release: one per app, so that one release can carry several.
pub(crate) fn feed_name(app_name: &str) -> String {
    format!("{app_name}-latest.json")
}

/// The update downloaded and kept, as written beside its file.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Kept {
    version: String,
    /// `app`, `appimage` or `nsis`.
    format: String,
    /// The file's name in the update directory.
    file: String,
    /// What the update replaces: the `.app`, the AppImage, the install directory.
    extract_path: PathBuf,
}

const KEPT: &str = "pending.json";

/// The real updates, through cargo-packager's updater.
pub(crate) struct Packager {
    spec: &'static PackageSpec,
    source: Source,
    current: Version,
    exe: PathBuf,
    /// Where the downloaded update is kept: the cache directory's `update`.
    dir: PathBuf,
}

impl Packager {
    /// `Error::NotPackaged` when the running executable is not in a package.
    pub(crate) fn new(spec: &'static PackageSpec, source: Source) -> Result<Packager, Error> {
        let exe = package::current_exe().ok_or(Error::NotPackaged)?;
        let current = Version::parse(spec.version)
            .map_err(|e| Error::Update(format!("the app's version {:?}: {e}", spec.version)))?;
        let dir = AppDirs::new(spec.app_name)
            .ok_or_else(|| Error::Update("no cache directory".into()))?
            .cache()
            .join("update");
        Ok(Packager {
            spec,
            source,
            current,
            exe,
            dir,
        })
    }

    /// The feed's URL for this check: on GitHub, the latest release's, or with pre-releases
    /// the newest release that has the app's feed.
    fn feed_url(&self) -> Result<url::Url, Error> {
        let feed = feed_name(self.spec.app_name);
        let url = match &self.source {
            Source::Github {
                repo,
                prerelease: false,
            } => format!(
                "{}/releases/latest/download/{feed}",
                repo.trim_end_matches('/')
            ),
            Source::Github {
                repo,
                prerelease: true,
            } => newest_feed(repo, &feed)?,
            Source::Http(url) => url.clone(),
        };
        url::Url::parse(&url).map_err(|e| Error::Update(format!("the feed {url:?}: {e}")))
    }

    fn read_kept(&self) -> Option<Kept> {
        let text = fs::read_to_string(self.dir.join(KEPT)).ok()?;
        serde_json::from_str(&text).ok()
    }
}

/// The newest release of `repo` (`https://github.com/owner/name`), pre-releases included,
/// that has the asset `feed`: its download URL.
fn newest_feed(repo: &str, feed: &str) -> Result<String, Error> {
    #[derive(Deserialize)]
    struct Release {
        draft: bool,
        assets: Vec<Asset>,
    }
    #[derive(Deserialize)]
    struct Asset {
        name: String,
        browser_download_url: String,
    }
    let path = repo
        .trim_end_matches('/')
        .strip_prefix("https://github.com/")
        .ok_or_else(|| Error::Update(format!("{repo:?} is not a GitHub repository")))?;
    let releases: Vec<Release> = cargo_packager_updater::reqwest::blocking::Client::new()
        .get(format!(
            "https://api.github.com/repos/{path}/releases?per_page=30"
        ))
        .header("User-Agent", "corundum-framework-package")
        .header("Accept", "application/vnd.github+json")
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json())
        .map_err(|e| Error::Network(e.to_string()))?;
    // GitHub lists the newest first.
    releases
        .into_iter()
        .filter(|r| !r.draft)
        .find_map(|r| {
            r.assets
                .into_iter()
                .find(|a| a.name == feed)
                .map(|a| a.browser_download_url)
        })
        .ok_or_else(|| Error::Update(format!("no release of {repo} has {feed}")))
}

impl Backend for Packager {
    fn current(&self) -> Version {
        self.current.clone()
    }

    fn check(&self) -> Result<Option<Found>, Error> {
        let config = Config {
            endpoints: vec![self.feed_url()?],
            pubkey: self.spec.update_pubkey.to_owned(),
            windows: None,
        };
        let update = UpdaterBuilder::new(self.current.clone(), config)
            .executable_path(&self.exe)
            .build()
            .map_err(from_updater)?
            .check()
            .map_err(from_updater)?;
        let Some(update) = update else {
            return Ok(None);
        };
        let version = Version::parse(&update.version)
            .map_err(|e| Error::Update(format!("the feed's version {:?}: {e}", update.version)))?;
        Ok(Some(Found {
            version,
            update: Some(Box::new(update)),
        }))
    }

    fn download(&self, found: &Found, progress: &dyn Fn(u8)) -> Result<(), Error> {
        let update = found
            .update
            .as_deref()
            .ok_or_else(|| Error::Update("nothing to download".into()))?;
        let received = std::cell::Cell::new(0usize);
        // Verified against the app's public key before it is returned.
        let bytes = update
            .download_extended(
                |chunk, total| {
                    received.set(received.get() + chunk);
                    if let Some(total) = total.filter(|t| *t > 0) {
                        let percent = (received.get() as u64 * 100 / total).min(100);
                        progress(percent as u8);
                    }
                },
                || progress(100),
            )
            .map_err(from_updater)?;
        let format = format_name(update.format);
        let file = format!("{}.{format}", found.version);
        // One update kept at a time: an older one goes.
        let _ = fs::remove_dir_all(&self.dir);
        fs::create_dir_all(&self.dir)?;
        fs::write(self.dir.join(&file), bytes)?;
        let kept = Kept {
            version: found.version.to_string(),
            format: format.to_owned(),
            file,
            extract_path: update.extract_path.clone(),
        };
        fs::write(
            self.dir.join(KEPT),
            serde_json::to_string(&kept).map_err(|e| Error::Update(e.to_string()))?,
        )?;
        Ok(())
    }

    fn pending(&self) -> Option<Version> {
        let kept = self.read_kept()?;
        self.dir.join(&kept.file).exists().then_some(())?;
        Version::parse(&kept.version).ok()
    }

    fn apply(&self, applied: Applied, after: &After) -> Result<(), Error> {
        // Both are the update `download` kept; this run's must be the one it downloaded.
        let kept = self
            .read_kept()
            .ok_or_else(|| Error::Update("nothing downloaded".into()))?;
        if let Applied::Downloaded(found) = &applied
            && kept.version != found.version.to_string()
        {
            return Err(Error::Update(format!(
                "the kept update is {}, not the downloaded {}",
                kept.version, found.version
            )));
        }
        let file = self.dir.join(&kept.file);
        let args: &[OsString] = match after {
            After::RestartApp | After::Exit => &[],
            After::RestartWith(args) => args,
        };
        let restart = !matches!(after, After::Exit);
        install(&kept, &file, &self.exe, restart, args, self.spec)?;
        if !restart {
            std::process::exit(0);
        }
        Ok(())
    }
}

/// The format's name, as the feed and the kept file have it.
fn format_name(format: UpdateFormat) -> &'static str {
    match format {
        UpdateFormat::App => "app",
        UpdateFormat::AppImage => "appimage",
        UpdateFormat::Nsis => "nsis",
        UpdateFormat::Wix => "wix",
        // A format a newer updater added: kept under a name, refused at the apply.
        #[allow(unreachable_patterns)]
        _ => "unknown",
    }
}

/// Windows: once this process has exited, runs the installer silently and starts the app
/// again, from a PowerShell of its own that outlives the app (the console shim's job lets
/// it leave). The installer replaces the files in place and runs the install hook.
#[cfg(windows)]
fn install(
    kept: &Kept,
    file: &Path,
    exe: &Path,
    restart: bool,
    args: &[OsString],
    _spec: &PackageSpec,
) -> Result<(), Error> {
    use std::os::windows::process::CommandExt;

    if kept.format != "nsis" {
        return Err(Error::Update(format!(
            "cannot install a {} update",
            kept.format
        )));
    }
    // The installer wants an `.exe` name.
    let setup = file.with_extension("exe");
    fs::rename(file, &setup)?;
    let script = windows_script(std::process::id(), &setup, exe, restart, args);
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    let system_root = std::env::var("SYSTEMROOT").unwrap_or_else(|_| r"C:\Windows".into());
    std::process::Command::new(format!(
        r"{system_root}\System32\WindowsPowerShell\v1.0\powershell.exe"
    ))
    .args([
        "-NoProfile",
        "-NonInteractive",
        "-WindowStyle",
        "Hidden",
        "-Command",
    ])
    .arg(script)
    .env(RESTART, "1")
    .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
    .spawn()?;
    Ok(())
}

/// The PowerShell that waits for `pid` to exit, runs `setup` silently and, for a restart,
/// starts `exe` with `args`.
#[cfg_attr(not(windows), allow(dead_code))]
fn windows_script(pid: u32, setup: &Path, exe: &Path, restart: bool, args: &[OsString]) -> String {
    fn quoted(s: &str) -> String {
        format!("'{}'", s.replace('\'', "''"))
    }
    let mut script = format!(
        "Wait-Process -Id {pid} -ErrorAction SilentlyContinue; \
         Start-Process -Wait -FilePath {} -ArgumentList '/S'",
        quoted(&setup.to_string_lossy())
    );
    if restart {
        script.push_str(&format!(
            "; Start-Process -FilePath {}",
            quoted(&exe.to_string_lossy())
        ));
        if !args.is_empty() {
            let list: Vec<String> = args.iter().map(|a| quoted(&a.to_string_lossy())).collect();
            script.push_str(&format!(" -ArgumentList @({})", list.join(", ")));
        }
    }
    script
}

/// macOS and Linux: the updater replaces the `.app` or the AppImage in place (the running
/// process keeps its open files), then the app is started again from the same path.
#[cfg(not(windows))]
fn install(
    kept: &Kept,
    file: &Path,
    _exe: &Path,
    restart: bool,
    args: &[OsString],
    spec: &PackageSpec,
) -> Result<(), Error> {
    let format = match kept.format.as_str() {
        "app" => UpdateFormat::App,
        "appimage" => UpdateFormat::AppImage,
        other => return Err(Error::Update(format!("cannot install a {other} update"))),
    };
    let bytes = fs::read(file)?;
    // What the updater's install reads of it: the format and the path it replaces. The
    // signature was checked when it was downloaded.
    let update = Update {
        config: Config {
            endpoints: Vec::new(),
            pubkey: spec.update_pubkey.to_owned(),
            windows: None,
        },
        body: None,
        current_version: spec.version.to_owned(),
        version: kept.version.clone(),
        date: None,
        target: String::new(),
        extract_path: kept.extract_path.clone(),
        download_url: url::Url::parse("file:///").expect("a valid URL"),
        signature: String::new(),
        timeout: None,
        headers: Default::default(),
        format,
    };
    update.install(bytes).map_err(from_updater)?;
    let _ = fs::remove_file(file);
    if restart {
        restart_from(&kept.extract_path, args)?;
    }
    Ok(())
}

/// Starts the app installed at `installed` (the `.app` or the AppImage) again with `args`,
/// once this process has exited: a single-instance app started earlier would only hand
/// itself to this one.
#[cfg(not(windows))]
fn restart_from(installed: &Path, args: &[OsString]) -> Result<(), Error> {
    let program = if installed.extension().is_some_and(|e| e == "app") {
        // The bundle's executable: the one `Info.plist` names is the only one there.
        let macos = installed.join("Contents").join("MacOS");
        fs::read_dir(&macos)?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .find(|p| p.is_file())
            .ok_or_else(|| Error::Update(format!("nothing to start in {}", macos.display())))?
    } else {
        installed.to_path_buf()
    };
    std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(RESTART_SCRIPT)
        .arg("sh")
        .arg(std::process::id().to_string())
        .arg(program)
        .args(args)
        .env(RESTART, "1")
        .spawn()?;
    Ok(())
}

/// Waits for the process `$1` to exit, then runs the rest of the arguments.
#[cfg(not(windows))]
const RESTART_SCRIPT: &str =
    r#"while kill -0 "$1" 2>/dev/null; do sleep 0.2; done; shift; exec "$@""#;

/// The updater's error as the crate's: the network is the network, anything else is the
/// update's.
fn from_updater(e: cargo_packager_updater::Error) -> Error {
    match e {
        cargo_packager_updater::Error::Reqwest(e) => Error::Network(e.to_string()),
        e => Error::Update(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_feed_is_named_after_the_app() {
        assert_eq!(feed_name("sapphire-sync"), "sapphire-sync-latest.json");
    }

    #[test]
    fn the_windows_script_waits_installs_silently_and_restarts_with_the_arguments() {
        let script = windows_script(
            42,
            Path::new(r"C:\cache\update\1.2.0.exe"),
            Path::new(r"C:\Users\o'brien\AppData\Local\Demo\demo.exe"),
            true,
            &[OsString::from("serve"), OsString::from("--port=1")],
        );
        assert_eq!(
            script,
            "Wait-Process -Id 42 -ErrorAction SilentlyContinue; \
             Start-Process -Wait -FilePath 'C:\\cache\\update\\1.2.0.exe' -ArgumentList '/S'; \
             Start-Process -FilePath 'C:\\Users\\o''brien\\AppData\\Local\\Demo\\demo.exe' \
             -ArgumentList @('serve', '--port=1')"
        );
    }

    #[test]
    fn the_windows_script_without_a_restart_only_installs() {
        let script = windows_script(7, Path::new("s.exe"), Path::new("a.exe"), false, &[]);
        assert_eq!(
            script,
            "Wait-Process -Id 7 -ErrorAction SilentlyContinue; \
             Start-Process -Wait -FilePath 's.exe' -ArgumentList '/S'"
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_restart_waits_for_the_process_to_exit_then_runs_the_program() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("out");
        let mut sleeper = std::process::Command::new("sleep")
            .arg("1")
            .spawn()
            .unwrap();
        let pid = sleeper.id();
        // Reaped as soon as it ends: a child left a zombie still answers `kill -0`.
        let reaper = std::thread::spawn(move || sleeper.wait().unwrap());
        let started = std::time::Instant::now();
        let status = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(RESTART_SCRIPT)
            .arg("sh")
            .arg(pid.to_string())
            .arg("/bin/sh")
            .arg("-c")
            .arg(format!("echo \"$0\" > {}", out.display()))
            .arg("ran")
            .status()
            .unwrap();
        assert!(status.success());
        // Not before the process it waited for had ended.
        assert!(started.elapsed() >= std::time::Duration::from_millis(800));
        reaper.join().unwrap();
        assert_eq!(fs::read_to_string(&out).unwrap().trim(), "ran");
    }

    #[test]
    fn a_kept_update_reads_back() {
        let kept = Kept {
            version: "1.2.0".into(),
            format: "appimage".into(),
            file: "1.2.0.appimage".into(),
            extract_path: PathBuf::from("/home/u/Demo.AppImage"),
        };
        let text = serde_json::to_string(&kept).unwrap();
        assert_eq!(serde_json::from_str::<Kept>(&text).unwrap(), kept);
    }
}
