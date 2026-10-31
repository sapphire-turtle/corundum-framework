//! The demo as a Velopack package (corundum-framework#51), on corundum-framework-package:
//! [`PACKAGE`] is what `main` hands to `package::start`, the updater [`start_updates`]
//! makes for a GUI start feeds the About page's update row and the status bar's badge, and
//! the command line goes on `PATH` as `corundum-demo`.
//!
//! The demo's daemon (`corundum-demo daemon`, [`crate::service`]) is the package's service:
//! an update stops it if it runs, and the next start starts it again. What each step did is
//! written to [`log_path`], which the Settings › Velopack page shows.
//!
//! Only with the `velopack` feature, which the release workflow (`demo-release.yml`) turns
//! on; `cargo run` builds the demo without it.

use std::io::Write as _;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use corundum_framework::gui::egui;
use corundum_framework::gui::settings::SettingsSection;
use corundum_framework::gui::tr;
use corundum_framework::package::cli_path::CliSpec;
use corundum_framework::package::{PackageSpec, Services, Source, Start, Updater};

/// The repository the demo's packages are released in, as `demo-v<version>` pre-releases.
const REPO: &str = "https://github.com/sapphire-turtle/corundum-framework";

/// How often a running demo looks for an update.
const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

/// The demo's package: its hooks write to the log, and its daemon is its one service.
pub static PACKAGE: PackageSpec = PackageSpec {
    app_name: crate::APP_NAME,
    on_installed: Some(on_installed),
    on_uninstalling: Some(on_uninstalling),
    before_apply: Some(before_apply),
    cli: Some(CliSpec {
        command: "corundum-demo",
    }),
    services: Services::new(&[crate::service::service]),
};

/// The updater [`start_updates`] made: unset outside a package and on a command-line start.
static UPDATER: OnceLock<Updater> = OnceLock::new();

fn on_installed() {
    log("hook installed");
}

fn on_uninstalling() {
    log("hook uninstalling");
}

/// Note the update in the log. The demo has nothing of its own to stop: the package stops
/// the daemon, its service, right after this.
fn before_apply() -> Result<(), String> {
    log("before_apply: the package stops the running services next");
    Ok(())
}

/// Write what `package::start` found to the log.
pub fn log_start(start: &Start) {
    log(&format!("start: {start:?}"));
}

/// For a GUI start in a package: apply an update downloaded at an earlier run (which
/// restarts the demo), else look for one now and every day. Outside a package nothing is
/// made, and the About page keeps its binary row. Only the demo's instance calls this
/// (`main` holds its lock first), so a second start never applies under the first.
pub fn start_updates() {
    let source = Source::Github {
        repo: REPO.to_owned(),
        prerelease: true,
    };
    let updater = match Updater::new(&PACKAGE, source) {
        Ok(updater) => updater,
        Err(e) => {
            log(&format!("updater: {e}"));
            return;
        }
    };
    if let Err(e) = updater.apply_pending_at_start() {
        log(&format!("apply pending: {e}"));
    }
    updater.run_in_background(CHECK_EVERY);
    let _ = UPDATER.set(updater);
}

/// The updater [`start_updates`] made, if it made one.
pub(crate) fn updater() -> Option<Updater> {
    UPDATER.get().cloned()
}

/// The file what the package did is written to: the demo's data directory, which neither
/// an update nor an uninstall touches.
pub fn log_path() -> Option<PathBuf> {
    corundum_framework::platform::dirs::AppDirs::new(crate::APP_NAME)
        .map(|dirs| dirs.data().join("velopack.log"))
}

/// Append `line` to the log, with the time and the process id. Failing to write is ignored:
/// the log is for reading, never what the demo depends on.
pub fn log(line: &str) {
    let Some(path) = log_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(file, "{secs} pid={} {line}", std::process::id());
    }
}

/// Settings › Velopack: the log of what the package did.
pub(crate) struct Page;

impl SettingsSection for Page {
    fn label(&self, ctx: &egui::Context) -> String {
        tr!(ctx, "demo-settings-velopack")
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.heading(tr!(ui, "demo-settings-velopack"));
        ui.separator();
        log_ui(ui);
    }
}

/// The log's last lines.
fn log_ui(ui: &mut egui::Ui) {
    let Some(path) = log_path() else { return };
    ui.label(format!("Log: {}", path.display()));
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    let tail = lines[lines.len().saturating_sub(40)..].join("\n");
    egui::ScrollArea::vertical()
        .max_height(240.0)
        .stick_to_bottom(true)
        .show(ui, |ui| {
            ui.add(egui::Label::new(egui::RichText::new(tail).monospace()).wrap());
        });
}
