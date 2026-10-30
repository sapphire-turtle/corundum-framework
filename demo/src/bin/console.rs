//! The demo's Windows console shim, copied into the package as `corundum-demo.exe`
//! (demo-release.yml): the GUI binary has no console, so this one, on `PATH`, starts it with
//! the terminal's arguments and streams and returns its exit code.

#[cfg(windows)]
fn main() {
    corundum_framework::package::console::forward("corundum-framework-demo.exe")
}

/// Elsewhere the GUI binary answers in a terminal by itself.
#[cfg(not(windows))]
fn main() {
    eprintln!("corundum-framework-demo-console is for Windows only: run corundum-framework-demo");
    std::process::exit(2);
}
