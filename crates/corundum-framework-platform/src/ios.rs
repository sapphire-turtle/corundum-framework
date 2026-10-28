//! What the system's screens on iOS (the photo picker, the QR code scanner) need from the
//! app's window: a view controller to be presented over.

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_ui_kit::{UIApplication, UIViewController, UIWindowScene};

/// The view controller on top of the app's window: its root view controller, or what that
/// presents already. `None` before the window is shown.
///
/// The window is the key window of the app's window scene; an app without scenes (no
/// `UIApplicationSceneManifest`) has its window in the application's own list.
pub(crate) fn presenting_controller(mtm: MainThreadMarker) -> Option<Retained<UIViewController>> {
    let app = UIApplication::sharedApplication(mtm);
    let scene_window = app
        .connectedScenes()
        .allObjects()
        .into_iter()
        .filter_map(|scene| scene.downcast::<UIWindowScene>().ok())
        .find_map(|scene| scene.keyWindow().or_else(|| scene.windows().firstObject()));
    #[allow(deprecated)]
    let window = scene_window.or_else(|| app.windows().firstObject())?;
    let mut top = window.rootViewController()?;
    while let Some(presented) = top.presentedViewController() {
        top = presented;
    }
    Some(top)
}
