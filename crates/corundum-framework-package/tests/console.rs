#![cfg(not(any(target_os = "ios", target_os = "android")))]

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;

use corundum_framework_package::console::forward_to;

/// Builds the example child with cargo (into the target dir this test runs from) and returns it.
fn build_child() -> PathBuf {
    let mut cargo = Command::new(env!("CARGO"));
    cargo.args([
        "build",
        "-p",
        "corundum-framework-package",
        "--example",
        "console_child",
    ]);
    // Reuse the target dir of this build so the child lands beside the test binary.
    if let Some(dir) = std::env::var_os("CARGO_TARGET_DIR") {
        cargo.env("CARGO_TARGET_DIR", dir);
    }
    assert!(cargo.status().expect("run cargo").success());
    let exe = std::env::current_exe().unwrap();
    let profile_dir = exe.parent().unwrap().parent().unwrap();
    profile_dir
        .join("examples")
        .join(format!("console_child{}", std::env::consts::EXE_SUFFIX))
}

#[test]
fn the_child_s_arguments_and_exit_code_come_through() {
    let child = build_child();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("args.txt");
    // SAFETY: no other test in this binary reads or writes the environment.
    unsafe { std::env::set_var("CONSOLE_CHILD_OUT", &out) };
    let code = forward_to(&child, [OsString::from("a"), OsString::from("b c")]);
    assert_eq!(code, 3);
    assert_eq!(std::fs::read_to_string(out).unwrap(), "a\nb c\n");
}

#[test]
fn a_missing_target_exits_1() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(forward_to(&dir.path().join("nope"), []), 1);
}
