use std::fs;
use std::path::Path;

use corundum_framework_platform::dirs::AppDirs;
use corundum_framework_platform::package;
use semver::Version;
use velopack::VelopackApp;
use velopack::locator::{LocationContext, auto_locate_app_manifest};

use crate::cli_path::CliSpec;
use crate::error::Error;

/// Set by Velopack for the app's first start after an install. Its `run` removes it, so it
/// is read before.
const FIRSTRUN: &str = "VELOPACK_FIRSTRUN";

/// The file, in the app's data directory, holding the version the app last ran with.
const KEPT_VERSION: &str = "package-version";

/// What a package needs from the app.
#[derive(Clone, Debug)]
pub struct PackageSpec {
    /// The app's directory name, where the last version run is kept: `sapphire-journal`.
    pub app_name: &'static str,
    /// Run once per install: in the install hook on Windows, at the first start elsewhere.
    /// It must be safe to run again.
    pub on_installed: Option<fn()>,
    /// Run before uninstalling, on Windows only (no other OS runs anything): 30 seconds,
    /// no window.
    pub on_uninstalling: Option<fn()>,
    /// Run before any update is applied: stop the app's background processes. An error
    /// stops the update.
    pub before_apply: Option<fn() -> Result<(), String>>,
    /// The command line to put on `PATH`, if the app has one.
    pub cli: Option<CliSpec>,
}

/// Which start this is, against the version the app last ran with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Start {
    /// Not in a package (`cargo run`, the scripts, cargo).
    NotPackaged,
    /// The first start of this install.
    FirstStart { version: Version },
    /// The first start since the version changed.
    Updated { from: Version, to: Version },
    /// The version last run.
    Same { version: Version },
}

/// Velopack's start, then what this start is. The first thing `main` runs: started for a
/// Windows hook, it runs the hook and exits.
pub fn start(spec: &PackageSpec) -> Start {
    let firstrun = std::env::var_os(FIRSTRUN).is_some();
    // The app applies an update when it asks to, after `before_apply`, never behind its back
    // at a start.
    let app = VelopackApp::build().set_auto_apply_on_startup(false);
    #[cfg(windows)]
    let app = windows_hooks(app, spec);
    let mut app = app;
    app.run();

    if package::current().is_none() {
        return Start::NotPackaged;
    }
    let now = match package_version() {
        Ok(now) => now,
        Err(e) => {
            tracing::warn!("in a package, but its version could not be read: {e}");
            return Start::NotPackaged;
        }
    };
    let kept_path = AppDirs::new(spec.app_name).map(|dirs| dirs.data().join(KEPT_VERSION));
    // Read as bytes: a file that is not UTF-8 is as unreadable as any other garbage, and
    // makes a first start.
    let kept = kept_path
        .as_deref()
        .and_then(|path| fs::read(path).ok())
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
    let start = classify(kept.as_deref(), firstrun, &now);
    match &kept_path {
        Some(path) => {
            if let Err(e) = keep(path, &now) {
                tracing::warn!("could not keep the version in {}: {e}", path.display());
            }
        }
        None => tracing::warn!("no data directory to keep the version in"),
    }
    // Windows ran it in the install hook; the other OSes have no hook, so the first start
    // stands in for it.
    #[cfg(not(windows))]
    if matches!(start, Start::FirstStart { .. })
        && let Some(on_installed) = spec.on_installed
    {
        on_installed();
    }
    start
}

/// The app's install and uninstall hooks. Velopack runs a hook and exits when the installer
/// starts the app for one; the update hooks are left unset, so it exits for those without
/// running anything.
#[cfg(windows)]
fn windows_hooks<'a>(app: VelopackApp<'a>, spec: &PackageSpec) -> VelopackApp<'a> {
    let on_installed = spec.on_installed;
    let on_uninstalling = spec.on_uninstalling;
    app.on_after_install_fast_callback(move |_| {
        if let Some(on_installed) = on_installed {
            on_installed();
        }
    })
    .on_before_uninstall_fast_callback(move |_| {
        if let Some(on_uninstalling) = on_uninstalling {
            on_uninstalling();
        }
    })
}

/// The version of the package the app runs from, from its manifest.
fn package_version() -> Result<Version, Error> {
    let locator = auto_locate_app_manifest(LocationContext::FromCurrentExe)
        .map_err(|e| Error::Velopack(e.to_string()))?;
    Ok(locator.get_manifest_version())
}

/// Keeps `version` as the one the app last ran with.
fn keep(path: &Path, version: &Version) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, version.to_string())
}

/// Which start this is: a first one when nothing readable is kept or Velopack says so, an
/// update when the kept version differs from the package's either way (a downgrade is a
/// change too), and the same otherwise.
pub(crate) fn classify(kept: Option<&str>, firstrun: bool, now: &Version) -> Start {
    match kept.and_then(|kept| Version::parse(kept.trim()).ok()) {
        Some(from) if !firstrun => {
            if from == *now {
                Start::Same {
                    version: now.clone(),
                }
            } else {
                Start::Updated {
                    from,
                    to: now.clone(),
                }
            }
        }
        _ => Start::FirstStart {
            version: now.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    #[test]
    fn a_start_is_told_from_the_kept_version_the_variable_and_the_package() {
        let first = |now: &str| Start::FirstStart { version: v(now) };
        let same = |now: &str| Start::Same { version: v(now) };
        let updated = |from: &str, to: &str| Start::Updated {
            from: v(from),
            to: v(to),
        };
        let cases: &[(Option<&str>, bool, &str, Start)] = &[
            (None, false, "1.0.0", first("1.0.0")),
            (Some("1.0.0"), false, "1.0.0", same("1.0.0")),
            (Some("1.0.0"), false, "1.1.0", updated("1.0.0", "1.1.0")),
            (Some("1.1.0"), false, "1.0.0", updated("1.1.0", "1.0.0")),
            (Some("1.0.0"), true, "1.0.0", first("1.0.0")),
            (Some(""), false, "1.0.0", first("1.0.0")),
            (Some("garbage"), false, "1.0.0", first("1.0.0")),
            (Some(" 1.0.0\n"), false, "1.0.0", same("1.0.0")),
        ];
        for (kept, firstrun, now, want) in cases {
            assert_eq!(
                classify(*kept, *firstrun, &v(now)),
                *want,
                "kept {kept:?}, firstrun {firstrun}, now {now}"
            );
        }
    }
}
