//! The child of the console shim's test: writes its arguments, one per line, to the file named
//! by `CONSOLE_CHILD_OUT` and exits with code 3.

fn main() {
    let out = std::env::var_os("CONSOLE_CHILD_OUT").expect("CONSOLE_CHILD_OUT is set");
    let mut text = String::new();
    for arg in std::env::args().skip(1) {
        text.push_str(&arg);
        text.push('\n');
    }
    std::fs::write(out, text).expect("write the arguments");
    std::process::exit(3);
}
