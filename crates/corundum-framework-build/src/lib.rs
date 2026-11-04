//! Build-time helpers for corundum apps, called from an app's `build.rs`: the app icon
//! embedded in a Windows executable ([`embed_icon`]), and rendered from its SVG for the
//! phones' projects ([`ios_app_icon`], [`android_app_icons`]), so a repository keeps only
//! the SVGs.
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

/// The iOS app icon's size: one 1024-pixel image, which iOS scales for every place.
const IOS_ICON_SIZE: u32 = 1024;

/// Android's screen densities, as each mipmap folder's name and its scale against mdpi in
/// halves (mdpi 1, hdpi 1.5, xhdpi 2, xxhdpi 3, xxxhdpi 4).
const ANDROID_DENSITIES: [(&str, u32); 5] = [
    ("mdpi", 2),
    ("hdpi", 3),
    ("xhdpi", 4),
    ("xxhdpi", 6),
    ("xxxhdpi", 8),
];

/// Renders the iOS app icon from the SVG at `svg_path` into the asset catalog's app icon
/// set at `appiconset_dir`, as `AppIcon.png`, when the app is built for iOS.
///
/// It is meant for `build.rs`, which an iOS app's Xcode project runs through `cargo build`
/// before Xcode compiles its asset catalog. The SVG is drawn full-bleed (iOS rounds the
/// corners itself), and any transparency is flattened onto black, as the App Store takes no
/// alpha channel in an app icon, so the SVG draws its own background. The set's
/// `Contents.json` names the image:
///
/// ```json
/// { "images": [{ "filename": "AppIcon.png", "idiom": "universal", "platform": "ios",
///                "size": "1024x1024" }],
///   "info": { "author": "xcode", "version": 1 } }
/// ```
///
/// It always asks Cargo to rerun when the SVG changes, and for iOS when the PNG does (so one
/// deleted comes back on the next build). It writes the PNG only when it differs from the
/// one there, so Xcode does not see a change on every build, and on failure prints a
/// `cargo:warning=` and returns, as [`embed_icon`] does.
pub fn ios_app_icon(svg_path: impl AsRef<Path>, appiconset_dir: impl AsRef<Path>) {
    let svg_path = svg_path.as_ref();
    println!("cargo:rerun-if-changed={}", svg_path.display());
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() != "ios" {
        return;
    }
    if let Err(what) = write_ios_icon(svg_path, appiconset_dir.as_ref()) {
        println!("cargo:warning=could not render the iOS app icon: {what}");
    }
}

/// Renders the Android launcher icons into the app's resources at `res_dir` (its
/// `src/main/res`), for every density, when the app is built for Android: `legacy_svg` as
/// `mipmap-*/ic_launcher.png` (48 dp, the icon before Android 8, drawn whole), and
/// `foreground_svg` as `mipmap-*/ic_launcher_foreground.png` (108 dp, the foreground of the
/// adaptive icon from Android 8 on, its mark within the middle 66 dp).
///
/// It is meant for `build.rs`, which `cargo ndk` runs when it builds the app's library,
/// before Gradle packs the resources. The adaptive icon's XML and its background stay in
/// the app's resources; the PNGs are generated, and a repository ignores them.
///
/// It always asks Cargo to rerun when an SVG changes, and for Android when a PNG does. It
/// writes a PNG only when it differs from the one there, and on failure prints a
/// `cargo:warning=` and returns, as [`embed_icon`] does.
pub fn android_app_icons(
    legacy_svg: impl AsRef<Path>,
    foreground_svg: impl AsRef<Path>,
    res_dir: impl AsRef<Path>,
) {
    let (legacy_svg, foreground_svg) = (legacy_svg.as_ref(), foreground_svg.as_ref());
    println!("cargo:rerun-if-changed={}", legacy_svg.display());
    println!("cargo:rerun-if-changed={}", foreground_svg.display());
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() != "android" {
        return;
    }
    if let Err(what) = write_android_icons(legacy_svg, foreground_svg, res_dir.as_ref()) {
        println!("cargo:warning=could not render the Android app icons: {what}");
    }
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("reading {}: {e}", path.display()))
}

/// Writes `bytes` to `path`, making its folder, unless the file already holds them. It asks
/// Cargo to rerun when the file changes too, so one deleted (by a clean of the ignored files)
/// is written again on the next build.
fn write_if_changed(path: &Path, bytes: &[u8]) -> Result<(), String> {
    println!("cargo:rerun-if-changed={}", path.display());
    if std::fs::read(path).is_ok_and(|old| old == bytes) {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("making {}: {e}", dir.display()))?;
    }
    std::fs::write(path, bytes).map_err(|e| format!("writing {}: {e}", path.display()))
}

/// Renders `svg` to an opaque RGB PNG of `size` pixels, its transparency flattened onto black.
fn opaque_png(svg: &[u8], size: u32) -> Result<Vec<u8>, String> {
    let rgba = corundum_framework_platform::icon::render_rgba(svg, size)
        .map_err(|e| format!("rendering: {e}"))?;
    let rgb: Vec<u8> = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| {
            let a = u16::from(p[3]);
            [0, 1, 2].map(|i| ((u16::from(p[i]) * a + 127) / 255) as u8)
        })
        .collect();
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, size, size);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|e| format!("encoding: {e}"))?;
    writer
        .write_image_data(&rgb)
        .map_err(|e| format!("encoding: {e}"))?;
    writer.finish().map_err(|e| format!("encoding: {e}"))?;
    Ok(out)
}

fn write_ios_icon(svg_path: &Path, appiconset_dir: &Path) -> Result<(), String> {
    let png = opaque_png(&read(svg_path)?, IOS_ICON_SIZE)
        .map_err(|e| format!("{}: {e}", svg_path.display()))?;
    write_if_changed(&appiconset_dir.join("AppIcon.png"), &png)
}

fn write_android_icons(
    legacy_svg: &Path,
    foreground_svg: &Path,
    res_dir: &Path,
) -> Result<(), String> {
    let layers = [
        (legacy_svg, "ic_launcher.png", 24),
        (foreground_svg, "ic_launcher_foreground.png", 54),
    ];
    for (svg_path, name, half_dp) in layers {
        let svg = read(svg_path)?;
        for (density, halves) in ANDROID_DENSITIES {
            let png = corundum_framework_platform::icon::render_png(&svg, half_dp * halves)
                .map_err(|e| format!("rendering {}: {e}", svg_path.display()))?;
            write_if_changed(&res_dir.join(format!("mipmap-{density}")).join(name), &png)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 24-unit SVG: a blue square with a transparent margin of 6.
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="6" y="6" width="12" height="12" fill="#4F7FD9"/></svg>"##;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("corundum-build-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn png_info(bytes: &[u8]) -> (u32, png::ColorType) {
        let reader = png::Decoder::new(std::io::Cursor::new(bytes))
            .read_info()
            .unwrap();
        let info = reader.info();
        (info.width, info.color_type)
    }

    #[test]
    fn the_ios_icon_is_1024_pixels_with_no_alpha() {
        let dir = scratch("ios");
        std::fs::write(dir.join("icon.svg"), SVG).unwrap();
        write_ios_icon(&dir.join("icon.svg"), &dir.join("AppIcon.appiconset")).unwrap();
        let png = std::fs::read(dir.join("AppIcon.appiconset/AppIcon.png")).unwrap();
        assert_eq!(png_info(&png), (1024, png::ColorType::Rgb));
    }

    #[test]
    fn transparency_is_flattened_onto_black() {
        let png = opaque_png(SVG, 24).unwrap();
        let mut reader = png::Decoder::new(std::io::Cursor::new(png))
            .read_info()
            .unwrap();
        let mut rgb = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut rgb).unwrap();
        assert_eq!(&rgb[..3], &[0, 0, 0], "the margin");
        let middle = (12 * 24 + 12) * 3;
        assert_eq!(&rgb[middle..middle + 3], &[0x4F, 0x7F, 0xD9], "the square");
    }

    #[test]
    fn the_android_icons_are_in_every_density() {
        let dir = scratch("android");
        std::fs::write(dir.join("icon.svg"), SVG).unwrap();
        let res = dir.join("res");
        write_android_icons(&dir.join("icon.svg"), &dir.join("icon.svg"), &res).unwrap();
        for (density, legacy, layer) in
            [("mdpi", 48, 108), ("hdpi", 72, 162), ("xxxhdpi", 192, 432)]
        {
            let read =
                |name: &str| std::fs::read(res.join(format!("mipmap-{density}/{name}"))).unwrap();
            assert_eq!(png_info(&read("ic_launcher.png")).0, legacy, "{density}");
            assert_eq!(
                png_info(&read("ic_launcher_foreground.png")).0,
                layer,
                "{density}"
            );
        }
    }

    #[test]
    fn an_unchanged_icon_is_not_written_again() {
        let dir = scratch("unchanged");
        let path = dir.join("a.png");
        write_if_changed(&path, b"one").unwrap();
        let before = std::fs::metadata(&path).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        write_if_changed(&path, b"one").unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            before
        );
        write_if_changed(&path, b"two").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
    }

    #[test]
    fn only_windows_wants_the_icon() {
        assert!(wants_icon("windows"));
        for os in ["linux", "macos", "ios", "android", ""] {
            assert!(!wants_icon(os), "{os}");
        }
    }
}
