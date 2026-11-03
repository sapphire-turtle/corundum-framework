//! The demo on a desktop.

#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn main() -> corundum_framework::eframe::Result<()> {
    use corundum_demo::CommandLine;

    let command = corundum_demo::command_line(std::env::args_os().skip(1));
    // Without the package's shim (a `cargo install`ed demo), `--help`, `--version` and the
    // daemon's log reach the terminal through its console; the daemon's task, with no parent
    // console, opens none. Not the window: attached, it would take the terminal's Ctrl+C and
    // close with it.
    if command.in_terminal() {
        corundum_framework::platform::console::attach_parent();
    }
    let answered = command.answer();
    // The daemon is the service manager's process, not the window's: it takes no
    // single-instance lock, and it skips the package's start, which starts again the
    // services an update stopped. The daemon is one of them, and starting it from itself
    // (`launchctl kickstart -k` replaces what runs) would kill it.
    if command == CommandLine::Daemon {
        corundum_demo::init_logging();
        corundum_demo::migrate_dirs();
        corundum_demo::service::run_daemon();
    }
    // The installer's hooks start the executable and expect it to exit at once: first of all,
    // but the logging their failures go to and the move of the data directory the start
    // reads.
    #[cfg(feature = "package")]
    {
        corundum_demo::init_logging();
        corundum_demo::migrate_dirs();
        let start = corundum_framework::package::start(&corundum_demo::package::PACKAGE);
        corundum_demo::package::log_start(&start);
    }
    if answered {
        return Ok(());
    }
    // A downloaded update is applied at start, which a second start must not do under the
    // first: the lock is taken here, before it, rather than by the window.
    #[cfg(feature = "package")]
    {
        use corundum_framework::platform::instance;
        // Both outcomes go to package.log too, which a check on the machines reads: a second
        // start logs its `start` line as well before it learns that it is second.
        let Some(instance) = instance::acquire_or_activate(corundum_demo::APP_NAME) else {
            tracing::info!("corundum demo: running already; brought it to the front");
            corundum_demo::package::log("instance: running already; brought it to the front");
            return Ok(());
        };
        corundum_demo::package::log("instance: the only one running");
        corundum_demo::package::start_updates();
        corundum_demo::run_as(instance)
    }
    #[cfg(not(feature = "package"))]
    corundum_demo::run()
}

// On Android the demo starts from GameActivity (`android_main` in the lib), and on iOS from
// the Xcode app in ios/ (`corundum_demo_main` in the lib), not from here.
#[cfg(any(target_os = "android", target_os = "ios"))]
fn main() {}
