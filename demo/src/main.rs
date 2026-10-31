//! The demo on a desktop.

#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn main() -> corundum_framework::eframe::Result<()> {
    use corundum_demo::CommandLine;

    let command = corundum_demo::command_line(std::env::args_os().skip(1));
    // The daemon is the service manager's process, not the window's: it takes no
    // single-instance lock, and it skips the package's start, which starts again the
    // services an update stopped. The daemon is one of them, and starting it from itself
    // (`launchctl kickstart -k` replaces what runs) would kill it.
    if command == CommandLine::Daemon {
        corundum_demo::init_logging();
        corundum_demo::migrate_dirs();
        corundum_demo::service::run_daemon();
    }
    // Velopack's hooks start the executable and expect it to exit at once: first of all,
    // but the logging their failures go to and the move of the data directory the start
    // reads.
    #[cfg(feature = "velopack")]
    {
        corundum_demo::init_logging();
        corundum_demo::migrate_dirs();
        let start = corundum_framework::package::start(&corundum_demo::velopack::PACKAGE);
        corundum_demo::velopack::log_start(&start);
    }
    if command == CommandLine::Done {
        return Ok(());
    }
    // A downloaded update is applied at start, which a second start must not do under the
    // first: the lock is taken here, before it, rather than by the window.
    #[cfg(feature = "velopack")]
    {
        use corundum_framework::platform::instance;
        // Both outcomes go to velopack.log too, which a check on the machines reads: a second
        // start logs its `start` line as well before it learns that it is second.
        let Some(instance) = instance::acquire_or_activate(corundum_demo::APP_NAME) else {
            tracing::info!("corundum demo: running already; brought it to the front");
            corundum_demo::velopack::log("instance: running already; brought it to the front");
            return Ok(());
        };
        corundum_demo::velopack::log("instance: the only one running");
        corundum_demo::velopack::start_updates();
        corundum_demo::run_as(instance)
    }
    #[cfg(not(feature = "velopack"))]
    corundum_demo::run()
}

// On Android the demo starts from GameActivity (`android_main` in the lib), and on iOS from
// the Xcode app in ios/ (`corundum_demo_main` in the lib), not from here.
#[cfg(any(target_os = "android", target_os = "ios"))]
fn main() {}
