//! The demo on a desktop.

#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(not(target_os = "android"))]
fn main() -> corundum_framework::eframe::Result<()> {
    // Velopack's hooks start the executable and expect it to exit at once: first of all.
    #[cfg(feature = "velopack")]
    {
        let start = corundum_framework::package::start(&corundum_demo::velopack::PACKAGE);
        corundum_demo::velopack::log_start(&start);
    }
    if command_line() {
        return Ok(());
    }
    #[cfg(feature = "velopack")]
    corundum_demo::velopack::start_updates();
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
