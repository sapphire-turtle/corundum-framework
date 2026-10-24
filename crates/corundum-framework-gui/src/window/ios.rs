//! The window on an iPhone: drawn over the whole screen, with egui told the safe area.
//!
//! eframe 0.37 sizes egui's screen and its wgpu surface from two different sizes on iOS.
//! egui-winit takes the screen from winit's `outer_size`, the whole screen; eframe makes
//! and resizes the surface at winit's `inner_size`, which on iOS is the safe area alone.
//! The Metal layer covers the whole view, so the smaller drawable is stretched over it: the
//! picture grows taller and its bottom, where the Shell's bar is, falls off the screen
//! (#20).
//!
//! eframe has no way to choose the surface's size, so the window makes `inner_size` the
//! whole screen instead: winit's window class answers `safeAreaInsets` with none, and the
//! real insets, read past that answer, go to egui through [`IosWindow`]'s
//! `raw_input_hook`. Remove this once eframe sizes the surface from the same size as the
//! screen.

use std::ffi::CString;

use objc2::encode::Encode;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Imp, Sel};
use objc2::{ClassType, Message, msg_send, sel};
use objc2_ui_kit::{UIEdgeInsets, UIView, UIWindow};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// Make the window's surface the whole screen, and wrap `app` to give egui the safe area.
/// The app is returned as is when the window's view cannot be found.
pub(super) fn full_screen(
    cc: &eframe::CreationContext<'_>,
    app: Box<dyn eframe::App>,
) -> Box<dyn eframe::App> {
    let Some(view) = ui_view(cc) else {
        tracing::warn!("no UIView behind the window: the safe area is left to eframe");
        return app;
    };
    let Some(window) = view.window() else {
        tracing::warn!("the view has no UIWindow: the safe area is left to eframe");
        return app;
    };
    hide_safe_area(&window);
    // winit's view reports a resize from its layout, and eframe then resizes the surface
    // at `inner_size`, now the whole screen.
    view.setNeedsLayout();
    Box::new(IosWindow { app, window })
}

fn ui_view(cc: &eframe::CreationContext<'_>) -> Option<Retained<UIView>> {
    let RawWindowHandle::UiKit(handle) = cc.window_handle().ok()?.as_raw() else {
        return None;
    };
    // SAFETY: a UiKit handle's `ui_view` is the window's UIView, alive while the window is.
    let view: &UIView = unsafe { handle.ui_view.cast().as_ref() };
    Some(view.retain())
}

/// Give `window`'s class a `safeAreaInsets` with no insets, over UIWindow's. winit reads
/// it for `inner_size` (and egui-winit for egui's insets, which [`IosWindow`] puts back).
fn hide_safe_area(window: &UIWindow) {
    extern "C-unwind" fn none(_this: &AnyObject, _cmd: Sel) -> UIEdgeInsets {
        UIEdgeInsets {
            top: 0.0,
            left: 0.0,
            bottom: 0.0,
            right: 0.0,
        }
    }
    let class = window.class();
    // winit's own class: overriding it leaves the system's windows (the keyboard's) alone.
    if class == UIWindow::class() {
        tracing::warn!("the window is a plain UIWindow: the safe area is left to eframe");
        return;
    }
    let types = CString::new(format!("{}@:", UIEdgeInsets::ENCODING)).expect("no NUL");
    // SAFETY: `none` has the signature `types` describes, that of `-[UIView safeAreaInsets]`.
    // `class_addMethod` adds it only if the class has none of its own (a second window).
    unsafe {
        let imp: Imp =
            std::mem::transmute(none as extern "C-unwind" fn(&AnyObject, Sel) -> UIEdgeInsets);
        objc2::ffi::class_addMethod(
            (class as *const objc2::runtime::AnyClass).cast_mut(),
            sel!(safeAreaInsets),
            imp,
            types.as_ptr(),
        );
    }
}

/// `window`'s safe area as UIKit has it, past the override [`hide_safe_area`] put on its
/// class.
fn safe_area(window: &UIWindow) -> egui::SafeAreaInsets {
    // SAFETY: UIWindow's own `safeAreaInsets`, sent to a UIWindow on the main thread (the
    // raw input hook runs on it).
    let insets: UIEdgeInsets =
        unsafe { msg_send![super(window, UIWindow::class()), safeAreaInsets] };
    egui::SafeAreaInsets(egui::epaint::MarginF32 {
        left: insets.left as f32,
        right: insets.right as f32,
        top: insets.top as f32,
        bottom: insets.bottom as f32,
    })
}

/// The app, told the real safe area every frame.
struct IosWindow {
    app: Box<dyn eframe::App>,
    window: Retained<UIWindow>,
}

impl eframe::App for IosWindow {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        raw_input.safe_area_insets = Some(safe_area(&self.window));
        self.app.raw_input_hook(ctx, raw_input);
    }

    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.app.logic(ctx, frame);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.app.ui(ui, frame);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.app.save(storage);
    }

    fn on_exit(&mut self) {
        self.app.on_exit();
    }

    fn auto_save_interval(&self) -> std::time::Duration {
        self.app.auto_save_interval()
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        self.app.clear_color(visuals)
    }

    fn persist_egui_memory(&self) -> bool {
        self.app.persist_egui_memory()
    }
}
