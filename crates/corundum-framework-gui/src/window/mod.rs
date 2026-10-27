//! A desktop window: eframe on wgpu, with the renderer set up by [`gpu`](crate::gpu) and a
//! CJK font behind egui's own.
//!
//! [`run`] is the whole of opening an app's desktop window; [`open`] is its lower half, for
//! an entry that builds its own [`eframe::NativeOptions`]. With the `android` feature,
//! `run_android` opens the window on an Android phone. The sapphire apps
//! and asteria open their windows through here, so a renderer fix reaches all of them.
//!
//! On iOS, an app built with the iOS 27 SDK must declare a scene in its Info.plist, with
//! [`IOS_SCENE_DELEGATE`] as the scene's delegate: iOS 27 stops at launch an app that does
//! not, and winit 0.30 does not put its window in a scene by itself (#37).
//!
//! ```yaml
//! UIApplicationSceneManifest:
//!   UIApplicationSupportsMultipleScenes: false
//!   UISceneConfigurations:
//!     UIWindowSceneSessionRoleApplication:
//!       - UISceneConfigurationName: Default
//!         UISceneDelegateClassName: CorundumSceneDelegate
//! ```
//!
//! ```no_run
//! use corundum_framework_gui::{eframe, egui, window};
//!
//! struct Example;
//!
//! impl eframe::App for Example {
//!     fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
//!         ui.label("Hello");
//!     }
//! }
//!
//! fn main() -> eframe::Result<()> {
//!     window::run(
//!         window::WindowOptions::new("Example").inner_size([800.0, 500.0]),
//!         |_cc| Box::new(Example),
//!     )
//! }
//! ```

use corundum_framework_platform::launcher::{self, LauncherSpec};

use crate::gpu;

#[cfg(all(target_os = "android", feature = "android"))]
mod android;
#[cfg(target_os = "ios")]
mod ios;
// Used by the iOS window; kept platform-free so its tests run on a desktop.
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
mod marked_text;
// Used by the Android window; kept platform-free so its tests run on a desktop.
#[cfg_attr(not(all(target_os = "android", feature = "android")), allow(dead_code))]
mod text_input;

#[cfg(all(target_os = "android", feature = "android"))]
pub use android::{AndroidApp, run_android};

/// The class of the scene delegate an iOS app names in its Info.plist's scene manifest
/// (`UISceneDelegateClassName`). It shows the window once the app's scene connects.
pub const IOS_SCENE_DELEGATE: &str = "CorundumSceneDelegate";

/// Sets the egui context's fonts, once, before the app is built.
pub type InstallFonts = Box<dyn FnOnce(&egui::Context)>;

/// A desktop window: its title, its size, its fonts and the app's entry in the OS's app menu.
pub struct WindowOptions {
    title: String,
    inner_size: [f32; 2],
    min_inner_size: [f32; 2],
    fonts: Option<InstallFonts>,
    launcher: Option<LauncherSpec>,
    #[cfg(feature = "serde")]
    store: Option<crate::store::Store>,
}

impl WindowOptions {
    /// A window titled `title`, 900 × 600 and no smaller than 600 × 400.
    pub fn new(title: impl Into<String>) -> WindowOptions {
        WindowOptions {
            title: title.into(),
            inner_size: [900.0, 600.0],
            min_inner_size: [600.0, 400.0],
            fonts: None,
            launcher: None,
            #[cfg(feature = "serde")]
            store: None,
        }
    }

    /// The window's size when it opens.
    pub fn inner_size(mut self, size: [f32; 2]) -> WindowOptions {
        self.inner_size = size;
        self
    }

    /// The smallest the window can be made.
    pub fn min_inner_size(mut self, size: [f32; 2]) -> WindowOptions {
        self.min_inner_size = size;
        self
    }

    /// Set the fonts with `install` instead of the default: egui's own, with a CJK font
    /// the OS has behind them ([`fonts::install_system_cjk_fallback`](crate::fonts::install_system_cjk_fallback)).
    pub fn fonts(mut self, install: impl FnOnce(&egui::Context) + 'static) -> WindowOptions {
        self.fonts = Some(Box::new(install));
        self
    }

    /// Put the app in the OS's app menu as `spec` as the window opens
    /// ([`launcher::register`]), and give the window `spec`'s id and icon, so the OS matches
    /// the running window to the entry. With [`remember`](Self::remember), an entry the
    /// person took out ([`LauncherRow`](crate::settings::LauncherRow)) is not written back.
    pub fn launcher(mut self, spec: LauncherSpec) -> WindowOptions {
        self.launcher = Some(spec);
        self
    }

    /// The spec the app's entry in the app menu is made from, if one is set.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn launcher_spec(&self) -> Option<&LauncherSpec> {
        self.launcher.as_ref()
    }

    /// Whether `run` registers the app's entry: unless the store says the person took it
    /// out. Without a store it always does.
    fn wants_launcher(&self) -> bool {
        #[cfg(feature = "serde")]
        if let Some(store) = &self.store {
            return crate::store::wants_launcher(&store.load().unwrap_or_default());
        }
        true
    }

    /// Keep corundum's file in `store`: the look saved there is installed before the app's first
    /// frame, and the [`Shell`](crate::Shell) takes the rail's order saved there; from then
    /// on [`look::apply`](crate::look::apply) and the shell's menu save their changes there.
    #[cfg(feature = "serde")]
    pub fn remember(mut self, store: crate::store::Store) -> WindowOptions {
        self.store = Some(store);
        self
    }

    /// `create`, after attaching corundum's store to the context when there is one.
    #[cfg_attr(not(feature = "serde"), allow(unused_mut))]
    pub(crate) fn wrap_create<'f>(
        &mut self,
        create: impl FnOnce(&eframe::CreationContext<'_>) -> Box<dyn eframe::App> + 'f,
    ) -> impl FnOnce(&eframe::CreationContext<'_>) -> Box<dyn eframe::App> + 'f {
        #[cfg(feature = "serde")]
        let store = self.store.take();
        move |cc| {
            #[cfg(feature = "serde")]
            if let Some(store) = store {
                crate::store::attach(&cc.egui_ctx, store);
            }
            create(cc)
        }
    }
}

/// Open a desktop window and run it until it closes.
///
/// Installs [`gpu::install_panic_hook`], opens the window on wgpu with
/// [`gpu::wgpu_options`], and builds the app with `create` once the window exists.
pub fn run(
    options: WindowOptions,
    create: impl FnOnce(&eframe::CreationContext<'_>) -> Box<dyn eframe::App>,
) -> eframe::Result<()> {
    let mut options = options;
    // Before the store is handed to the context: it is read here for the launcher.
    let wants_launcher = options.wants_launcher();
    let create = options.wrap_create(create);
    let WindowOptions {
        title,
        inner_size,
        min_inner_size,
        fonts,
        launcher,
        ..
    } = options;
    gpu::install_panic_hook();
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size(inner_size)
        .with_min_inner_size(min_inner_size)
        .with_title(&title);
    if let Some(spec) = launcher {
        if wants_launcher && let Err(err) = launcher::register(&spec) {
            tracing::warn!("could not put {} in the app menu: {err}", spec.id);
        }
        viewport = with_launcher_look(viewport, &spec);
    }
    let native = eframe::NativeOptions {
        viewport,
        wgpu_options: gpu::wgpu_options(),
        ..Default::default()
    };
    open(&title, native, fonts, create)
}

/// `viewport` with `spec`'s id, and its icon where it can be rendered, so the OS matches the
/// window to the app's entry in the app menu.
fn with_launcher_look(
    viewport: egui::ViewportBuilder,
    spec: &LauncherSpec,
) -> egui::ViewportBuilder {
    // Before the window exists, so the taskbar groups it with the app's entry.
    if let Err(err) = launcher::set_process_app_id(spec) {
        tracing::warn!("could not set the app id of {}: {err}", spec.id);
    }
    let viewport = viewport.with_app_id(spec.id);
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        const SIZE: u32 = 256;
        match corundum_framework_platform::icon::render_rgba(spec.icon_svg, SIZE) {
            Ok(rgba) => {
                return viewport.with_icon(std::sync::Arc::new(egui::IconData {
                    rgba,
                    width: SIZE,
                    height: SIZE,
                }));
            }
            Err(err) => tracing::warn!("could not render the icon of {}: {err}", spec.id),
        }
    }
    viewport
}

/// Run eframe with `native` as the caller built it, building the app with `create` once the
/// window exists: reports the renderer ([`gpu::on_render_state`]) and installs `fonts`, or
/// the default ones when `None`. [`run`] is this for a desktop window.
pub fn open(
    title: &str,
    native: eframe::NativeOptions,
    fonts: Option<InstallFonts>,
    create: impl FnOnce(&eframe::CreationContext<'_>) -> Box<dyn eframe::App>,
) -> eframe::Result<()> {
    // Before the event loop starts: UIKit makes the app's scene as it launches (#37).
    #[cfg(target_os = "ios")]
    ios::scene::adopt();
    eframe::run_native(
        title,
        native,
        Box::new(move |cc| {
            gpu::on_render_state(cc.wgpu_render_state.as_ref());
            match fonts {
                Some(install) => install(&cc.egui_ctx),
                None => {
                    crate::fonts::install_system_cjk_fallback(&cc.egui_ctx);
                }
            }
            let app = create(cc);
            // eframe stretches its surface over an iPhone's screen (#20): see `ios`.
            #[cfg(target_os = "ios")]
            let app = ios::full_screen(cc, app);
            Ok(app)
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_default_and_override_the_window_size() {
        let options = WindowOptions::new("Example");
        assert_eq!(options.title, "Example");
        assert_eq!(options.inner_size, [900.0, 600.0]);
        assert_eq!(options.min_inner_size, [600.0, 400.0]);
        assert!(options.fonts.is_none());
        let options = options
            .inner_size([1000.0, 640.0])
            .min_inner_size([720.0, 440.0])
            .fonts(|_| {});
        assert_eq!(options.inner_size, [1000.0, 640.0]);
        assert_eq!(options.min_inner_size, [720.0, 440.0]);
        assert!(options.fonts.is_some());
    }

    #[test]
    fn options_keep_the_launcher_spec() {
        const SPEC: LauncherSpec = LauncherSpec {
            id: "net.example.Test",
            name: "Test",
            description: "A test app",
            category: "Utility",
            icon_svg: b"<svg/>",
            bundle_prefix: "net.example",
        };
        assert!(WindowOptions::new("x").launcher_spec().is_none());
        let options = WindowOptions::new("x").launcher(SPEC);
        assert_eq!(
            options.launcher_spec().map(|s| s.id),
            Some("net.example.Test")
        );
    }
}
