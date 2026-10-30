//! The Velopack spike's stand-in for an app server or a bridge (see `velopack.rs`): a process
//! the package starts and leaves running, whose executable an update has to replace.

#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    use corundum_demo::velopack;

    velopack::log(&format!(
        "daemon start exe={:?}",
        std::env::current_exe().unwrap_or_default()
    ));
    let heartbeat = velopack::log_path()
        .map(|path| path.with_file_name("velopack-daemon-heartbeat"))
        .unwrap_or_else(|| std::env::temp_dir().join("corundum-demo-daemon-heartbeat"));
    velopack::daemon(&heartbeat)
}
