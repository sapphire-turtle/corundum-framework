//! Android's half of [`platform`](super): the clipboard, through JNI.

use jni::objects::{JObject, JString};
use jni::{Env, JavaVM, jni_sig, jni_str};

/// The primary clip's text, coerced as Android's own paste would, or `None`.
pub(super) fn clipboard_text() -> Option<String> {
    match read_clipboard() {
        Ok(text) => text,
        Err(err) => {
            tracing::warn!("could not read the clipboard: {err}");
            None
        }
    }
}

/// The activity whose window the app draws in, set by the mobile host: `ndk_context`
/// holds the *application* context, which has no window.
static ACTIVITY: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Remember the activity (`AndroidApp::activity_as_ptr`) for [`window_insets`].
pub(crate) fn set_activity(activity: *mut std::ffi::c_void) {
    ACTIVITY.store(activity as usize, std::sync::atomic::Ordering::SeqCst);
}

/// Run `f` on this thread attached to the process's JavaVM.
///
/// The attachment runs `f` in a local frame of its own, so the locals made there are
/// freed on the way out although the thread stays attached for the activity's life. A
/// Java exception a call throws is caught and cleared, and comes back as the error: none
/// is left pending to abort the next JNI call.
fn with_env<T>(f: impl FnOnce(&mut Env) -> jni::errors::Result<T>) -> jni::errors::Result<T> {
    let ctx = ndk_context::android_context();
    // SAFETY: `ndk_context` holds the process's JavaVM, set by android-activity before
    // `android_main` runs and valid for the process's lifetime.
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) };
    vm.attach_current_thread(f)
}

fn read_clipboard() -> jni::errors::Result<Option<String>> {
    let context = ndk_context::android_context().context();
    with_env(|env| {
        // SAFETY: `ndk_context` holds the application context, a global reference valid
        // for the process's lifetime.
        let activity = unsafe { JObject::from_raw(env, context.cast()) };
        let name = env.new_string("clipboard")?;
        let manager = env
            .call_method(
                &activity,
                jni_str!("getSystemService"),
                jni_sig!("(Ljava/lang/String;)Ljava/lang/Object;"),
                &[(&name).into()],
            )?
            .l()?;
        if manager.is_null() {
            return Ok(None);
        }
        let clip = env
            .call_method(
                &manager,
                jni_str!("getPrimaryClip"),
                jni_sig!("()Landroid/content/ClipData;"),
                &[],
            )?
            .l()?;
        if clip.is_null() {
            return Ok(None);
        }
        let count = env
            .call_method(&clip, jni_str!("getItemCount"), jni_sig!("()I"), &[])?
            .i()?;
        if count < 1 {
            return Ok(None);
        }
        let item = env
            .call_method(
                &clip,
                jni_str!("getItemAt"),
                jni_sig!("(I)Landroid/content/ClipData$Item;"),
                &[0.into()],
            )?
            .l()?;
        let text = env
            .call_method(
                &item,
                jni_str!("coerceToText"),
                jni_sig!("(Landroid/content/Context;)Ljava/lang/CharSequence;"),
                &[(&activity).into()],
            )?
            .l()?;
        if text.is_null() {
            return Ok(None);
        }
        let text = env
            .call_method(
                &text,
                jni_str!("toString"),
                jni_sig!("()Ljava/lang/String;"),
                &[],
            )?
            .l()?;
        let text = env.cast_local::<JString>(text)?.try_to_string(env)?;
        Ok(Some(text))
    })
}

/// The system bars' and the keyboard's insets in physical pixels — left, top, right, bottom —
/// or `None` where the platform does not report them.
pub(crate) fn window_insets() -> Option<[i32; 4]> {
    match read_insets() {
        Ok(insets) => insets,
        Err(err) => {
            tracing::debug!("could not read the window insets: {err}");
            None
        }
    }
}

fn read_insets() -> jni::errors::Result<Option<[i32; 4]>> {
    let activity = ACTIVITY.load(std::sync::atomic::Ordering::SeqCst);
    if activity == 0 {
        return Ok(None);
    }
    with_env(|env| {
        // SAFETY: the activity is the global reference android-activity holds for as long
        // as the activity lives, which outlasts the window that asks.
        let activity = unsafe { JObject::from_raw(env, activity as jni::sys::jobject) };
        let window = env
            .call_method(
                &activity,
                jni_str!("getWindow"),
                jni_sig!("()Landroid/view/Window;"),
                &[],
            )?
            .l()?;
        let decor = env
            .call_method(
                &window,
                jni_str!("getDecorView"),
                jni_sig!("()Landroid/view/View;"),
                &[],
            )?
            .l()?;
        let insets = env
            .call_method(
                &decor,
                jni_str!("getRootWindowInsets"),
                jni_sig!("()Landroid/view/WindowInsets;"),
                &[],
            )?
            .l()?;
        if insets.is_null() {
            return Ok(None);
        }
        let sdk = env
            .get_static_field(
                jni_str!("android/os/Build$VERSION"),
                jni_str!("SDK_INT"),
                jni_sig!("I"),
            )?
            .i()?;
        if sdk >= 30 {
            // WindowInsets.Type.systemBars() | ime() | displayCutout()
            let mut kind = |name| {
                env.call_static_method(
                    jni_str!("android/view/WindowInsets$Type"),
                    name,
                    jni_sig!("()I"),
                    &[],
                )
                .and_then(|v| v.i())
            };
            let kinds = kind(jni_str!("systemBars"))?
                | kind(jni_str!("ime"))?
                | kind(jni_str!("displayCutout"))?;
            let rect = env
                .call_method(
                    &insets,
                    jni_str!("getInsets"),
                    jni_sig!("(I)Landroid/graphics/Insets;"),
                    &[kinds.into()],
                )?
                .l()?;
            let mut side = |name| {
                env.get_field(&rect, name, jni_sig!("I"))
                    .and_then(|v| v.i())
            };
            Ok(Some([
                side(jni_str!("left"))?,
                side(jni_str!("top"))?,
                side(jni_str!("right"))?,
                side(jni_str!("bottom"))?,
            ]))
        } else {
            let mut side = |name| {
                env.call_method(&insets, name, jni_sig!("()I"), &[])
                    .and_then(|v| v.i())
            };
            Ok(Some([
                side(jni_str!("getSystemWindowInsetLeft"))?,
                side(jni_str!("getSystemWindowInsetTop"))?,
                side(jni_str!("getSystemWindowInsetRight"))?,
                side(jni_str!("getSystemWindowInsetBottom"))?,
            ]))
        }
    })
}
