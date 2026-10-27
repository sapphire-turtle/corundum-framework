//! Build-time helpers for corundum apps, called from an app's `build.rs`.
//!
//! ```toml
//! [build-dependencies]
//! corundum-framework-build = "0.2"
//! ```
//!
//! ```no_run
//! // In build.rs, inside `fn main`:
//! corundum_framework_build::embed_icon("assets/icon.svg");
//! ```

use std::path::Path;

/// Tells whether an app built for `target_os` carries its icon in the executable, which
/// only Windows does; the other platforms take the icon from the bundle or the desktop file.
pub(crate) fn wants_icon(target_os: &str) -> bool {
    target_os == "windows"
}

/// Embeds the SVG at `svg_path` as the icon of the Windows executable being built.
///
/// It is meant for `build.rs`. It always asks Cargo to rerun when the SVG changes. Only
/// for a Windows target does it render the SVG to an `.ico` in `OUT_DIR` and compile it
/// into the executable's resources; for other targets it does nothing more. When any step
/// of that fails (an unreadable SVG, or no resource compiler on a cross build), it prints a
/// `cargo:warning=` and returns, so it never breaks the app's build.
pub fn embed_icon(svg_path: impl AsRef<Path>) {
    let svg_path = svg_path.as_ref();
    println!("cargo:rerun-if-changed={}", svg_path.display());
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if !wants_icon(&target_os) {
        return;
    }
    if let Err(what) = embed_windows_icon(svg_path) {
        println!("cargo:warning=could not embed the app icon: {what}");
    }
}

/// Renders the SVG to `app.ico` in `OUT_DIR` and compiles it into the Windows resources,
/// returning what went wrong as text.
fn embed_windows_icon(svg_path: &Path) -> Result<(), String> {
    let svg =
        std::fs::read(svg_path).map_err(|e| format!("reading {}: {e}", svg_path.display()))?;
    let ico = corundum_framework_platform::icon::ico(&svg)
        .map_err(|e| format!("rendering {}: {e}", svg_path.display()))?;
    let out_dir = std::env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?;
    let ico_path = Path::new(&out_dir).join("app.ico");
    std::fs::write(&ico_path, ico).map_err(|e| format!("writing {}: {e}", ico_path.display()))?;
    let ico_str = ico_path
        .to_str()
        .ok_or("the icon path is not valid UTF-8")?;
    winresource::WindowsResource::new()
        .set_icon(ico_str)
        .compile()
        .map_err(|e| format!("compiling the resource: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_windows_wants_the_icon() {
        assert!(wants_icon("windows"));
        for os in ["linux", "macos", "ios", "android", ""] {
            assert!(!wants_icon(os), "{os}");
        }
    }
}
