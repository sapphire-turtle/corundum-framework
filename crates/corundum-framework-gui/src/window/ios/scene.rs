//! The window put in a scene, for an app that adopts UIKit's scene lifecycle.
//!
//! iOS 27 stops, at launch, an app built with its SDK that has no scenes (#37). winit 0.30
//! has no scenes: it runs `UIApplicationMain` with no delegate, and makes its window key and
//! visible with no scene, so the window of an app that declares a scene is never shown.
//!
//! When the app's Info.plist declares a scene manifest, [`adopt`] puts each window in a
//! scene as winit shows it: `-[UIWindow makeKeyAndVisible]` sets a window that has no scene
//! to the connected one first. A window shown before any scene connects waits, and
//! [`SceneDelegate`], the manifest's delegate, shows it once its scene connects. An app
//! without a manifest is left as it was. Remove this once winit adopts scenes
//! (rust-windowing/winit#4723).

use std::cell::RefCell;
use std::sync::OnceLock;

use objc2::rc::Retained;
use objc2::runtime::{Imp, NSObject, NSObjectProtocol, Sel};
use objc2::{ClassType, MainThreadMarker, MainThreadOnly, Message, define_class, sel};
use objc2_foundation::{NSBundle, ns_string};
use objc2_ui_kit::{
    UIApplication, UIResponder, UIScene, UISceneConnectionOptions, UISceneDelegate, UISceneSession,
    UIWindow, UIWindowScene, UIWindowSceneDelegate,
};

define_class!(
    // SAFETY: UIResponder has no subclassing requirements, and the class does not implement
    // Drop. Its name is `IOS_SCENE_DELEGATE`, the one an app puts in its Info.plist.
    #[unsafe(super(UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "CorundumSceneDelegate"]
    struct SceneDelegate;

    unsafe impl NSObjectProtocol for SceneDelegate {}

    unsafe impl UISceneDelegate for SceneDelegate {
        #[unsafe(method(scene:willConnectToSession:options:))]
        fn scene_will_connect(
            &self,
            scene: &UIScene,
            _session: &UISceneSession,
            _options: &UISceneConnectionOptions,
        ) {
            show_waiting(scene);
        }
    }

    unsafe impl UIWindowSceneDelegate for SceneDelegate {}
);

thread_local! {
    /// Windows shown before a scene connected.
    static WAITING: RefCell<Vec<Retained<UIWindow>>> = const { RefCell::new(Vec::new()) };
}

/// UIWindow's own `makeKeyAndVisible`.
static MAKE_KEY_AND_VISIBLE: OnceLock<Imp> = OnceLock::new();

/// Put winit's windows in scenes, when the app's Info.plist declares them. Call before
/// eframe starts the event loop: the scene delegate's class must exist when UIKit makes the
/// first scene.
pub(in crate::window) fn adopt() {
    let manifest = NSBundle::mainBundle()
        .objectForInfoDictionaryKey(ns_string!("UIApplicationSceneManifest"))
        .is_some();
    if !manifest {
        tracing::debug!("no UIApplicationSceneManifest in Info.plist: the window has no scene");
        return;
    }
    // Register the class now, for UIKit to find by its name.
    let _ = SceneDelegate::class();
    if MAKE_KEY_AND_VISIBLE.get().is_some() {
        return;
    }
    let Some(method) = UIWindow::class().instance_method(sel!(makeKeyAndVisible)) else {
        tracing::warn!("UIWindow has no makeKeyAndVisible: the window has no scene");
        return;
    };
    // SAFETY: `make_key_and_visible` has the signature of `-[UIWindow makeKeyAndVisible]`,
    // and calls the implementation it replaces, kept in `MAKE_KEY_AND_VISIBLE` first.
    unsafe {
        let imp: Imp =
            std::mem::transmute(make_key_and_visible as extern "C-unwind" fn(&UIWindow, Sel));
        let original = method.set_implementation(imp);
        let _ = MAKE_KEY_AND_VISIBLE.set(original);
    }
}

/// `makeKeyAndVisible`, in a scene: a window without one is set to the connected scene, or
/// waits for one to connect.
extern "C-unwind" fn make_key_and_visible(window: &UIWindow, cmd: Sel) {
    if window.windowScene().is_none() {
        let mtm = window.mtm();
        match connected_scene(mtm) {
            Some(scene) => window.setWindowScene(Some(&scene)),
            None => {
                WAITING.with_borrow_mut(|waiting| waiting.push(window.retain()));
                return;
            }
        }
    }
    let Some(&original) = MAKE_KEY_AND_VISIBLE.get() else {
        return;
    };
    // SAFETY: UIWindow's own `makeKeyAndVisible`, sent to a UIWindow with its selector.
    unsafe {
        let original: extern "C-unwind" fn(&UIWindow, Sel) = std::mem::transmute(original);
        original(window, cmd);
    }
}

/// The app's window scene, once one has connected.
fn connected_scene(mtm: MainThreadMarker) -> Option<Retained<UIWindowScene>> {
    UIApplication::sharedApplication(mtm)
        .connectedScenes()
        .allObjects()
        .into_iter()
        .find_map(|scene| scene.downcast::<UIWindowScene>().ok())
}

/// `scene` connected: show the windows waiting for it.
fn show_waiting(scene: &UIScene) {
    let Some(scene) = scene.downcast_ref::<UIWindowScene>() else {
        return;
    };
    for window in WAITING.with_borrow_mut(std::mem::take) {
        window.setWindowScene(Some(scene));
        window.makeKeyAndVisible();
    }
}
