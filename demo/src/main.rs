//! The demo on a desktop.

#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(not(target_os = "android"))]
fn main() -> corundum_framework::eframe::Result<()> {
    // Velopack's hooks start the executable and expect it to exit at once: first of all.
    #[cfg(feature = "velopack")]
    corundum_demo::velopack::startup();
    corundum_demo::run()
}

// On Android the demo starts from GameActivity (`android_main` in the lib), not from here.
#[cfg(target_os = "android")]
fn main() {}
