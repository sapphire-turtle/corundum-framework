use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use corundum_framework_platform::dirs::AppDirs;
use corundum_framework_platform::package;
use semver::Version;

use crate::backend;
use crate::services::Services;

#[cfg(windows)]
use crate::cli_path;
use crate::cli_path::CliSpec;

/// Whether [`start`] saw [`backend::RESTART`]: this start follows an apply.
static RESTARTED: AtomicBool = AtomicBool::new(false);

/// The file, in the app's data directory, holding the version the app last ran with.
const KEPT_VERSION: &str = "package-version";

/// What a package needs from the app.
#[derive(Clone, Debug)]
pub struct PackageSpec {
    /// The app's directory name, where the last version run is kept: `sapphire-journal`.
    /// Its update feed is the release's `<app_name>-latest.json`.
    pub app_name: &'static str,
    /// The running app's version: `env!("CARGO_PKG_VERSION")`.
    pub version: &'static str,
    /// The minisign public key the updates are signed for (`cargo packager signer
    /// generate`'s `.pub`, as it is): an update whose signature does not match is not
    /// installed.
    pub update_pubkey: &'static str,
    /// Run once per install: in the install hook on Windows (at every install, updates
    /// included), at the first start elsewhere. It must be safe to run again.
    pub on_installed: Option<fn()>,
    /// Run before uninstalling, on Windows only (no other OS runs anything): 30 seconds,
    /// no window.
    pub on_uninstalling: Option<fn()>,
    /// Run before any update is applied: stop the app's background processes. An error
    /// stops the update.
    pub before_apply: Option<fn() -> Result<(), String>>,
    /// The command line to put on `PATH`, if the app has one.
    pub cli: Option<CliSpec>,
    /// The app's background services: the running ones are stopped before an update is
    /// applied and started again at the next start, and on Windows each is uninstalled
    /// with the app. An app without services writes [`Services::NONE`].
    pub services: Services,
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

/// What this start is, after the Windows install hooks. The first thing `main` runs but the
/// app's logging: started by the installer for a hook (`--corundum-package installed` or
/// `uninstalling`), it runs the hook and exits, and what goes wrong in a hook (the command
/// line not put on `PATH`) is only logged, so initialise logging before calling `start`.
pub fn start(spec: &PackageSpec) -> Start {
    RESTARTED.store(
        std::env::var_os(backend::RESTART).is_some(),
        Ordering::Relaxed,
    );
    #[cfg(windows)]
    if let Some(hook) = hook_arg(std::env::args_os()) {
        run_hook(&hook, spec);
        std::process::exit(0);
    }

    // Whatever this start is, as the first process to start after an update starts the
    // services it stopped: the GUI, the command line, or one installed by the scripts.
    #[cfg(feature = "service")]
    if let Some(dirs) = AppDirs::new(spec.app_name) {
        crate::services::restart_stopped(
            spec,
            dirs.data(),
            &corundum_framework_service::Environment::detect(),
            &corundum_framework_service::SystemManager,
        );
    }

    if package::current().is_none() {
        return Start::NotPackaged;
    }
    let now = match Version::parse(spec.version) {
        Ok(version) => version,
        Err(e) => {
            tracing::warn!(
                "in a package, but the version {:?} is not one: {e}",
                spec.version
            );
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
    let start = classify(kept.as_deref(), &now);
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

/// Whether an apply started this process again, after installing an update or failing to:
/// `Updater::apply_pending_at_start` then applies nothing, as an update that failed would
/// be tried again at every start.
pub(crate) fn restarted() -> bool {
    RESTARTED.load(Ordering::Relaxed)
}

/// The install hook the installer asks for: `--corundum-package <hook>` as the first
/// arguments (corundum's NSIS template runs `installed` after every install and
/// `uninstalling` before an uninstall).
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn hook_arg(mut args: impl Iterator<Item = std::ffi::OsString>) -> Option<String> {
    args.next();
    if args.next()? != "--corundum-package" {
        return None;
    }
    args.next()?.into_string().ok()
}

/// Runs the install hook `hook`. A hook has no window: a failure is logged, never shown or
/// panicked on.
#[cfg(windows)]
fn run_hook(hook: &str, spec: &PackageSpec) {
    match hook {
        "installed" => {
            if let Some(cli) = spec.cli
                && let Err(e) = cli_path::install(&cli)
            {
                tracing::warn!("could not put {} on PATH: {e}", cli.command);
            }
            if let Some(on_installed) = spec.on_installed {
                on_installed();
            }
        }
        "uninstalling" => {
            // First, so the services' programs are no longer running when the files go:
            // each service's uninstall ends its task (`schtasks /end`) before deleting it.
            #[cfg(feature = "service")]
            crate::services::uninstall_all(
                spec,
                &corundum_framework_service::Environment::detect(),
                &corundum_framework_service::SystemManager,
            );
            if let Some(on_uninstalling) = spec.on_uninstalling {
                on_uninstalling();
            }
            if let Some(cli) = spec.cli
                && let Err(e) = cli_path::uninstall(&cli)
            {
                tracing::warn!("could not take {} off PATH: {e}", cli.command);
            }
        }
        other => tracing::warn!("unknown install hook {other:?}"),
    }
}

/// Keeps `version` as the one the app last ran with.
fn keep(path: &Path, version: &Version) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, version.to_string())
}

/// Which start this is: a first one when nothing readable is kept, an update when the kept
/// version differs from the package's either way (a downgrade is a change too), and the
/// same otherwise.
pub(crate) fn classify(kept: Option<&str>, now: &Version) -> Start {
    match kept.and_then(|kept| Version::parse(kept.trim()).ok()) {
        Some(from) => {
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
    fn a_start_is_told_from_the_kept_version_and_the_package() {
        let first = |now: &str| Start::FirstStart { version: v(now) };
        let same = |now: &str| Start::Same { version: v(now) };
        let updated = |from: &str, to: &str| Start::Updated {
            from: v(from),
            to: v(to),
        };
        let cases: &[(Option<&str>, &str, Start)] = &[
            (None, "1.0.0", first("1.0.0")),
            (Some("1.0.0"), "1.0.0", same("1.0.0")),
            (Some("1.0.0"), "1.1.0", updated("1.0.0", "1.1.0")),
            (Some("1.1.0"), "1.0.0", updated("1.1.0", "1.0.0")),
            (Some(""), "1.0.0", first("1.0.0")),
            (Some("garbage"), "1.0.0", first("1.0.0")),
            (Some(" 1.0.0\n"), "1.0.0", same("1.0.0")),
        ];
        for (kept, now, want) in cases {
            assert_eq!(classify(*kept, &v(now)), *want, "kept {kept:?}, now {now}");
        }
    }

    #[test]
    fn the_install_hook_is_the_first_arguments() {
        let args = |a: &[&str]| a.iter().map(std::ffi::OsString::from).collect::<Vec<_>>();
        assert_eq!(
            hook_arg(args(&["app.exe", "--corundum-package", "installed"]).into_iter()),
            Some("installed".to_owned())
        );
        assert_eq!(hook_arg(args(&["app.exe", "serve"]).into_iter()), None);
        assert_eq!(hook_arg(args(&["app.exe"]).into_iter()), None);
        assert_eq!(
            hook_arg(args(&["app.exe", "--corundum-package"]).into_iter()),
            None
        );
    }
}
