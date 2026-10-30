//! The demo as a Velopack package (corundum-framework#51), on corundum-framework-package:
//! [`PACKAGE`] is what `main` hands to `package::start`, the updater [`start_updates`]
//! makes for a GUI start feeds the About page's update row and the status bar's badge, and
//! the command line goes on `PATH` as `corundum-demo`.
//!
//! The spike's daemon stays, to show an update stopping what the app started: the
//! Settings › Velopack page starts `corundum-framework-demo-daemon` from the package, and
//! [`PACKAGE`]'s `before_apply` kills it before an update is applied. What each step did is
//! written to [`log_path`], which the page shows.
//!
//! Only with the `velopack` feature, which the release workflow (`demo-release.yml`) turns
//! on; `cargo run` builds the demo without it.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use corundum_framework::gui::egui;
use corundum_framework::gui::settings::SettingsSection;
use corundum_framework::gui::tr;
use corundum_framework::package::cli_path::CliSpec;
use corundum_framework::package::{PackageSpec, Source, Start, Updater};

/// The repository the demo's packages are released in, as `demo-v<version>` pre-releases.
const REPO: &str = "https://github.com/sapphire-turtle/corundum-framework";

/// The daemon's name, what a pid in [`pid_path`] is checked against before it is stopped.
const DAEMON_NAME: &str = "corundum-framework-demo-daemon";

/// The daemon's executable, beside the demo's in the package.
const DAEMON: &str = if cfg!(windows) {
    "corundum-framework-demo-daemon.exe"
} else {
    "corundum-framework-demo-daemon"
};

/// How long `before_apply` waits for a daemon it signalled to go.
const STOP_WAIT: Duration = Duration::from_secs(5);

/// How often a running demo looks for an update.
const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

/// The demo's package: its hooks write to the log, and an update first stops the daemon.
pub static PACKAGE: PackageSpec = PackageSpec {
    app_name: crate::APP_NAME,
    on_installed: Some(on_installed),
    on_uninstalling: Some(on_uninstalling),
    before_apply: Some(before_apply),
    cli: Some(CliSpec {
        command: "corundum-demo",
    }),
    // Its daemon becomes a registered service later; until then `before_apply` stops it.
    services: &[],
};

/// The updater [`start_updates`] made: unset outside a package and on a command-line start.
static UPDATER: OnceLock<Updater> = OnceLock::new();

/// The daemon the Velopack page started in this process, for `before_apply` to stop and
/// collect. One started by an earlier run, which outlives it, is found by [`pid_path`].
static DAEMON_CHILD: Mutex<Option<Child>> = Mutex::new(None);

fn on_installed() {
    log("hook installed");
}

fn on_uninstalling() {
    log("hook uninstalling");
}

/// Stop the daemon, so the update can replace its executable: the one this process
/// started, else one an earlier run left, named by [`pid_path`]. An error (the daemon still
/// runs) stops the update.
fn before_apply() -> Result<(), String> {
    let child = DAEMON_CHILD
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    let stopped = match child {
        Some(child) => stop_child(child),
        None => stop_left_daemon(),
    };
    if stopped.is_ok()
        && let Some(path) = pid_path()
    {
        let _ = std::fs::remove_file(path);
    }
    stopped
}

/// Stop the daemon this process started, and collect it.
fn stop_child(mut child: Child) -> Result<(), String> {
    let pid = child.id();
    match child.try_wait() {
        Ok(Some(status)) => {
            log(&format!(
                "before_apply: daemon pid={pid} had exited: {status}"
            ));
            return Ok(());
        }
        Ok(None) => {}
        Err(e) => log(&format!("before_apply: daemon pid={pid}: {e}")),
    }
    if let Err(e) = child.kill() {
        log(&format!("before_apply: daemon pid={pid} not stopped: {e}"));
        // Kept, so a later apply tries again.
        *DAEMON_CHILD.lock().unwrap_or_else(|e| e.into_inner()) = Some(child);
        return Err(format!("the daemon (pid {pid}) did not stop: {e}"));
    }
    // Killed: `wait` returns at once.
    let status = child
        .wait()
        .map(|s| s.to_string())
        .unwrap_or_else(|e| e.to_string());
    log(&format!("before_apply: daemon pid={pid} stopped: {status}"));
    Ok(())
}

/// Stop a daemon an earlier run started and left running, if [`pid_path`] names one that
/// still runs. A pid now another program's is left alone.
fn stop_left_daemon() -> Result<(), String> {
    let Some(pid) = pid_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| text.trim().parse::<u32>().ok())
    else {
        log("before_apply: no daemon to stop");
        return Ok(());
    };
    if !is_daemon(pid) {
        log(&format!(
            "before_apply: pid {pid} is no daemon of the demo's now"
        ));
        return Ok(());
    }
    terminate(pid).map_err(|e| {
        log(&format!("before_apply: daemon pid={pid} not stopped: {e}"));
        format!("the daemon (pid {pid}) did not stop: {e}")
    })?;
    let since = std::time::Instant::now();
    while is_daemon(pid) {
        if since.elapsed() > STOP_WAIT {
            log(&format!("before_apply: daemon pid={pid} still runs"));
            return Err(format!("the daemon (pid {pid}) still runs"));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    log(&format!(
        "before_apply: daemon pid={pid}, left by an earlier run, stopped"
    ));
    Ok(())
}

/// Whether `pid` runs and is the demo's daemon (its executable, by name).
#[cfg(target_os = "linux")]
fn is_daemon(pid: u32) -> bool {
    // `exe`, not `comm`, which the kernel cuts at 15 bytes. A zombie has none.
    std::fs::read_link(format!("/proc/{pid}/exe"))
        .is_ok_and(|exe| exe.to_string_lossy().contains(DAEMON_NAME))
}

/// Whether `pid` runs and is the demo's daemon (its executable, by name).
#[cfg(all(unix, not(target_os = "linux")))]
fn is_daemon(pid: u32) -> bool {
    std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .output()
        .is_ok_and(|out| {
            out.status.success() && String::from_utf8_lossy(&out.stdout).contains(DAEMON_NAME)
        })
}

/// Whether `pid` runs and is the demo's daemon (its image, by name).
#[cfg(windows)]
fn is_daemon(pid: u32) -> bool {
    hidden("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .output()
        .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).contains(DAEMON_NAME))
}

/// Ask `pid` to end.
#[cfg(unix)]
fn terminate(pid: u32) -> Result<(), String> {
    let pid = libc::pid_t::try_from(pid).map_err(|e| e.to_string())?;
    // SAFETY: `kill` takes any pid and signal number; a wrong one only returns an error.
    if unsafe { libc::kill(pid, libc::SIGTERM) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().to_string())
    }
}

/// End `pid`.
#[cfg(windows)]
fn terminate(pid: u32) -> Result<(), String> {
    let status = hidden("taskkill")
        .args(["/PID", &pid.to_string(), "/F"])
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("taskkill: {status}"))
    }
}

/// `program` without the console window a GUI process's child would otherwise flash.
#[cfg(windows)]
fn hidden(program: &str) -> std::process::Command {
    use std::os::windows::process::CommandExt as _;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut command = std::process::Command::new(program);
    command.creation_flags(CREATE_NO_WINDOW);
    command
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

/// The file the daemon's pid is kept in while it may run, beside the log: an update started
/// by a later run stops it from there.
fn pid_path() -> Option<PathBuf> {
    log_path().map(|path| path.with_file_name("velopack-daemon.pid"))
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

/// Settings › Velopack: the daemon an update stops, and the log.
pub(crate) struct Page;

impl SettingsSection for Page {
    fn label(&self, ctx: &egui::Context) -> String {
        tr!(ctx, "demo-settings-velopack")
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.heading(tr!(ui, "demo-settings-velopack"));
        ui.separator();
        daemon_ui(ui);
        ui.add_space(8.0);
        log_ui(ui);
    }
}

/// The daemon's pid while it runs, else the button that starts it.
fn daemon_ui(ui: &mut egui::Ui) {
    let mut daemon = DAEMON_CHILD.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(child) = daemon.as_mut()
        && let Ok(Some(status)) = child.try_wait()
    {
        log(&format!("daemon exited: {status}"));
        *daemon = None;
    }
    match daemon.as_ref() {
        Some(child) => {
            ui.label(format!("Daemon running, pid {}", child.id()));
        }
        None => {
            if ui.button("Start daemon").clicked() {
                *daemon = start_daemon();
            }
        }
    }
}

/// Start the daemon beside the demo's executable, where the package put both.
fn start_daemon() -> Option<Child> {
    let exe = std::env::current_exe().unwrap_or_default();
    let daemon = exe.with_file_name(DAEMON);
    match std::process::Command::new(&daemon).spawn() {
        Ok(child) => {
            log(&format!(
                "daemon started: {} pid={}",
                daemon.display(),
                child.id()
            ));
            if let Some(path) = pid_path()
                && let Err(e) = std::fs::write(&path, child.id().to_string())
            {
                log(&format!("daemon pid not kept in {}: {e}", path.display()));
            }
            Some(child)
        }
        Err(e) => {
            log(&format!(
                "daemon failed to start: {}: {e}",
                daemon.display()
            ));
            None
        }
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

/// The daemon's body: stay up, writing a line now and then, until killed. It stands in for
/// an app server or a bridge, whose executable an update must replace while it runs.
pub fn daemon(heartbeat: &Path) -> ! {
    loop {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        let _ = std::fs::write(heartbeat, format!("{secs} pid={}\n", std::process::id()));
        std::thread::sleep(Duration::from_secs(5));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pid_another_program_has_is_not_taken_for_the_daemon() {
        // This test's own process runs, but it is not the daemon: a stale pid file naming
        // it must stop nothing.
        assert!(!is_daemon(std::process::id()));
    }

    #[test]
    fn a_pid_nothing_runs_is_not_taken_for_the_daemon() {
        assert!(!is_daemon(u32::MAX - 1));
    }
}
