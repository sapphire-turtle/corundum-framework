//! Trying Velopack on the desktops (corundum-framework#51): the demo as a Velopack package,
//! installed, updated from GitHub Releases and uninstalled, and what each step does.
//!
//! The spike's questions, and where this module answers them:
//!
//! - which of Velopack's hooks reach the app on each OS: [`startup`] writes every one it
//!   sees, with the arguments and the variables it started with, to [`log_path`];
//! - what the app can tell of an install or an update where Velopack calls no hook: the
//!   version [`startup`] keeps beside the log, compared with the package's at each start;
//! - where the package puts the executables, and whether the paths survive an update: the
//!   About group's Velopack page ([`Page`]);
//! - what an update does to a process the package started and that still runs: the page's
//!   "Start daemon" starts `corundum-framework-demo-daemon` from the package.
//!
//! Only with the `velopack` feature, which the release workflow (`demo-release.yml`) turns
//! on; `cargo run` builds the demo without it.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use corundum_framework::gui::egui;
use corundum_framework::gui::settings::SettingsSection;
use corundum_framework::gui::tr;
use velopack::locator::{self, LocationContext, VelopackLocator};
use velopack::sources::{GithubSource, NoneSource};
use velopack::{UpdateCheck, UpdateInfo, UpdateManager, VelopackApp};

/// The repository the demo's packages are released in, as `demo-v<version>` pre-releases.
const REPO: &str = "https://github.com/sapphire-turtle/corundum-framework";

/// The daemon's executable, beside the demo's in the package.
const DAEMON: &str = if cfg!(windows) {
    "corundum-framework-demo-daemon.exe"
} else {
    "corundum-framework-demo-daemon"
};

/// The file the spike's findings are written to: the demo's data directory, which neither
/// an update nor an uninstall touches.
pub fn log_path() -> Option<PathBuf> {
    corundum_framework::platform::dirs::AppDirs::new(crate::APP_NAME)
        .map(|dirs| dirs.data().join("velopack.log"))
}

/// The version the last start ran, kept beside the log.
fn seen_path() -> Option<PathBuf> {
    corundum_framework::platform::dirs::AppDirs::new(crate::APP_NAME)
        .map(|dirs| dirs.data().join("velopack-seen-version"))
}

/// Append `line` to the log, with the time and the process id. Failing to write is ignored:
/// the log is what the spike reads, never what the demo depends on.
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

/// Velopack's start: the first thing `main` runs. A hook's start (`--veloapp-*`, Windows
/// only) logs it and exits here; any other start logs what it was started with and goes on.
pub fn startup() {
    let args: Vec<String> = std::env::args().collect();
    let vars: Vec<String> = std::env::vars()
        .filter(|(key, _)| key.starts_with("VELOPACK") || key == "APPIMAGE")
        .map(|(key, value)| format!("{key}={value}"))
        .collect();
    log(&format!(
        "start exe={:?} args={args:?} vars={vars:?}",
        std::env::current_exe().unwrap_or_default()
    ));

    let mut app = VelopackApp::build()
        .on_first_run(|version| log(&format!("hook first-run {version}")))
        .on_restarted(|version| log(&format!("hook restarted {version}")));
    #[cfg(windows)]
    {
        app = app
            .on_after_install_fast_callback(|version| log(&format!("hook install {version}")))
            .on_after_update_fast_callback(|version| log(&format!("hook updated {version}")))
            .on_before_update_fast_callback(|version| log(&format!("hook obsolete {version}")))
            .on_before_uninstall_fast_callback(|version| log(&format!("hook uninstall {version}")));
    }
    app.run();

    compare_seen_version();
}

/// What an app can tell by itself where no hook is called (macOS, Linux): the package's
/// version against the one the last start ran.
fn compare_seen_version() {
    let Ok(here) = locator::auto_locate_app_manifest(LocationContext::FromCurrentExe) else {
        log("seen: not in a Velopack package");
        return;
    };
    let now = here.get_manifest_version_full_string();
    let Some(path) = seen_path() else { return };
    match std::fs::read_to_string(&path) {
        Ok(before) if before.trim() == now => log(&format!("seen: same version {now}")),
        Ok(before) => log(&format!("seen: version changed {} -> {now}", before.trim())),
        Err(_) => log(&format!("seen: first start of a package, {now}")),
    }
    let _ = std::fs::write(&path, &now);
}

/// Settings › Velopack: where the package is, its updates, the daemon, and the log.
pub(crate) struct Page {
    work: Arc<Mutex<Work>>,
    daemon: Option<std::process::Child>,
}

/// What the update buttons did, from the worker thread.
#[derive(Default)]
struct Work {
    busy: bool,
    message: String,
    /// The update found by the last check, downloaded once `downloaded` is set.
    found: Option<UpdateInfo>,
    downloaded: bool,
}

impl Page {
    pub(crate) fn new() -> Page {
        Page {
            work: Arc::default(),
            daemon: None,
        }
    }

    /// The source: the repository's releases, pre-releases included (the demo's are).
    fn manager() -> Result<UpdateManager, String> {
        UpdateManager::new(GithubSource::new(REPO, None, true), None, None)
            .map_err(|e| e.to_string())
    }

    /// Run `job` on a thread, its outcome written to `work` and the window repainted.
    fn spawn(&self, ctx: &egui::Context, job: impl FnOnce(&mut Work) + Send + 'static) {
        let work = Arc::clone(&self.work);
        let ctx = ctx.clone();
        work.lock().unwrap().busy = true;
        std::thread::spawn(move || {
            let mut done = Work::default();
            {
                let current = work.lock().unwrap();
                done.found = current.found.clone();
                done.downloaded = current.downloaded;
            }
            job(&mut done);
            *work.lock().unwrap() = done;
            ctx.request_repaint();
        });
    }

    fn package_ui(ui: &mut egui::Ui) {
        ui.label(format!(
            "current_exe: {}",
            std::env::current_exe().unwrap_or_default().display()
        ));
        match locator::auto_locate_app_manifest(LocationContext::FromCurrentExe) {
            Ok(here) => locator_ui(ui, &here),
            Err(e) => {
                ui.label(format!("Not in a Velopack package: {e}"));
            }
        }
        match UpdateManager::new(NoneSource {}, None, None) {
            Ok(manager) => {
                ui.label(format!(
                    "UpdateManager: app id {}, version {}, portable {}",
                    manager.get_app_id(),
                    manager.get_current_version_as_string(),
                    manager.get_is_portable()
                ));
                if let Some(pending) = manager.get_update_pending_restart() {
                    ui.label(format!("Pending restart: {}", pending.Version));
                }
            }
            Err(e) => {
                ui.label(format!("UpdateManager: {e}"));
            }
        }
    }

    fn updates_ui(&mut self, ui: &mut egui::Ui) {
        let (busy, found, downloaded, message) = {
            let work = self.work.lock().unwrap();
            (
                work.busy,
                work.found.clone(),
                work.downloaded,
                work.message.clone(),
            )
        };
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!busy, |ui| {
                if ui.button("Check for updates").clicked() {
                    self.spawn(ui.ctx(), |work| {
                        work.found = None;
                        work.downloaded = false;
                        work.message = match Self::manager()
                            .and_then(|m| m.check_for_updates().map_err(|e| e.to_string()))
                        {
                            Ok(UpdateCheck::UpdateAvailable(info)) => {
                                let text =
                                    format!("Update available: {}", info.TargetFullRelease.Version);
                                work.found = Some(*info);
                                text
                            }
                            Ok(UpdateCheck::NoUpdateAvailable) => "Up to date".to_owned(),
                            Ok(UpdateCheck::RemoteIsEmpty) => "No release found".to_owned(),
                            Err(e) => format!("Check failed: {e}"),
                        };
                        log(&format!("check: {}", work.message));
                    });
                }
                if let Some(info) = found.clone()
                    && !downloaded
                    && ui.button("Download").clicked()
                {
                    self.spawn(ui.ctx(), move |work| {
                        work.message = match Self::manager().and_then(|m| {
                            m.download_updates(&info, None).map_err(|e| e.to_string())
                        }) {
                            Ok(()) => {
                                work.downloaded = true;
                                format!("Downloaded {}", info.TargetFullRelease.Version)
                            }
                            Err(e) => format!("Download failed: {e}"),
                        };
                        log(&format!("download: {}", work.message));
                    });
                }
                if let Some(info) = found.as_ref()
                    && downloaded
                    && ui.button("Apply and restart").clicked()
                {
                    log(&format!("apply: {}", info.TargetFullRelease.Version));
                    // Exits the process: the updater waits for it, applies and restarts it.
                    if let Err(e) = Self::manager()
                        .and_then(|m| m.apply_updates_and_restart(info).map_err(|e| e.to_string()))
                    {
                        log(&format!("apply failed: {e}"));
                        self.work.lock().unwrap().message = format!("Apply failed: {e}");
                    }
                }
            });
            if busy {
                ui.spinner();
            }
        });
        if !message.is_empty() {
            ui.label(message);
        }
    }

    fn daemon_ui(&mut self, ui: &mut egui::Ui) {
        if let Some(child) = &mut self.daemon
            && let Ok(Some(status)) = child.try_wait()
        {
            log(&format!("daemon exited: {status}"));
            self.daemon = None;
        }
        match &self.daemon {
            Some(child) => {
                ui.label(format!("Daemon running, pid {}", child.id()));
            }
            None => {
                if ui.button("Start daemon").clicked() {
                    self.start_daemon();
                }
            }
        }
    }

    /// Start the daemon beside the demo's executable, where the package put both.
    fn start_daemon(&mut self) {
        let exe = std::env::current_exe().unwrap_or_default();
        let daemon = exe.with_file_name(DAEMON);
        match std::process::Command::new(&daemon).spawn() {
            Ok(child) => {
                log(&format!(
                    "daemon started: {} pid={}",
                    daemon.display(),
                    child.id()
                ));
                self.daemon = Some(child);
            }
            Err(e) => log(&format!(
                "daemon failed to start: {}: {e}",
                daemon.display()
            )),
        }
    }
}

/// The paths the package's locator gives.
fn locator_ui(ui: &mut egui::Ui, here: &VelopackLocator) {
    // The root is the AppImage itself on Linux, where the locator names it so.
    #[cfg(not(target_os = "linux"))]
    let root = ("root", here.get_root_dir());
    #[cfg(target_os = "linux")]
    let root = ("appimage", here.get_appimage_path());
    let rows: [(&str, PathBuf); 5] = [
        root,
        ("current bin", here.get_current_bin_dir()),
        ("main exe", here.get_main_exe_path()),
        ("updater", here.get_update_path()),
        ("packages", here.get_packages_dir()),
    ];
    ui.label(format!(
        "Package {} {} (channel {})",
        here.get_manifest_id(),
        here.get_manifest_version_full_string(),
        here.get_manifest_channel()
    ));
    for (name, path) in rows {
        ui.label(format!("{name}: {}", path.display()));
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

impl SettingsSection for Page {
    fn label(&self, ctx: &egui::Context) -> String {
        tr!(ctx, "demo-settings-velopack")
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.heading(tr!(ui, "demo-settings-velopack"));
        ui.separator();
        Self::package_ui(ui);
        ui.add_space(8.0);
        self.updates_ui(ui);
        ui.add_space(8.0);
        self.daemon_ui(ui);
        ui.add_space(8.0);
        log_ui(ui);
    }
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
        std::thread::sleep(std::time::Duration::from_secs(5));
    }
}
