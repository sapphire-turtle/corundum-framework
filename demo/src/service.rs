//! The demo's background service: the `daemon` subcommand (`corundum-demo daemon`), which
//! the first run's page and the About row register with the OS's user-level service manager
//! (systemd, launchd, Task Scheduler). It stands in for an app server or a bridge, a process
//! that keeps running while the window is closed and whose executable an update replaces.
//!
//! In every desktop build, not only the package: a `cargo run` shows the same page and row,
//! which say that a development build registers nothing.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use corundum_framework::service::ServiceSpec;

/// The demo's daemon as a service: the demo's executable, started with `daemon`.
pub fn service() -> ServiceSpec {
    ServiceSpec {
        app_name: "corundum-demo",
        description: "Corundum demo daemon".into(),
        args: vec!["daemon".into()],
        legacy_labels: vec![],
        post_install: None,
    }
}

/// The `daemon` subcommand: note the start in the package's log, then run [`daemon`] with
/// its heartbeat in the demo's data directory.
pub fn run_daemon() -> ! {
    #[cfg(feature = "velopack")]
    crate::velopack::log(&format!(
        "daemon start exe={:?}",
        std::env::current_exe().unwrap_or_default()
    ));
    daemon(&heartbeat_path())
}

/// The file the daemon writes its heartbeat to: the demo's data directory, else the
/// temporary one.
fn heartbeat_path() -> PathBuf {
    corundum_framework::platform::dirs::AppDirs::new(crate::APP_NAME)
        .map(|dirs| dirs.data().join("daemon-heartbeat"))
        .unwrap_or_else(|| std::env::temp_dir().join("corundum-demo-daemon-heartbeat"))
}

/// The daemon's body: stay up, writing the time and its pid to `heartbeat` every few
/// seconds, until it is stopped.
pub fn daemon(heartbeat: &Path) -> ! {
    if let Some(dir) = heartbeat.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    loop {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        let _ = std::fs::write(heartbeat, format!("{secs} pid={}\n", std::process::id()));
        std::thread::sleep(Duration::from_secs(5));
    }
}
