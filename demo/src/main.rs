//! The demo on a desktop.

#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(not(target_os = "android"))]
fn main() -> corundum_framework::eframe::Result<()> {
    // Velopack's hooks start the executable and expect it to exit at once: first of all,
    // but the logging their failures go to.
    #[cfg(feature = "velopack")]
    {
        corundum_demo::init_logging();
        let start = corundum_framework::package::start(&corundum_demo::velopack::PACKAGE);
        corundum_demo::velopack::log_start(&start);
    }
    if command_line() {
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

/// The command line the package puts on `PATH` (`corundum-demo`): `--help` and `--version`
/// answer in the terminal and return true. Any other start opens the window, arguments and
/// all, as the app menu's entry passes the files it was given.
#[cfg(not(target_os = "android"))]
fn command_line() -> bool {
    // `args_os`: a file name the entry passes need not be UTF-8.
    let first = std::env::args_os().nth(1);
    match first.as_ref().and_then(|arg| arg.to_str()) {
        Some("--help" | "-h") => {
            println!(
                "corundum-framework demo {}\n\n\
                 Usage: corundum-demo [--help | --version]\n\n\
                 Without an option, opens the demo's window.",
                env!("CARGO_PKG_VERSION")
            );
            true
        }
        Some("--version" | "-V") => {
            println!("corundum-framework demo {}", env!("CARGO_PKG_VERSION"));
            true
        }
        _ => false,
    }
}

// On Android the demo starts from GameActivity (`android_main` in the lib), not from here.
#[cfg(target_os = "android")]
fn main() {}
