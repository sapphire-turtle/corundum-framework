//! The JNI underneath the Android halves: the process's JavaVM and application context, from
//! `ndk_context`, and the activity a window draws in.
//!
//! android-activity fills `ndk_context` before `android_main` runs. A process it did not
//! start (a test binary run from `adb shell`, say) has none, and every call here then fails
//! with [`Error::UninitializedJavaVM`] instead of panicking.

use std::sync::atomic::{AtomicUsize, Ordering};

/// The jni this crate calls through, for [`with_env`]'s closures.
pub use jni;
use jni::errors::{Error, Result};
use jni::objects::JObject;
use jni::{Env, JavaVM};

/// Run `f` on this thread attached to the process's JavaVM.
///
/// The attachment runs `f` in a local frame of its own, so the locals made there are
/// freed on the way out although the thread stays attached. A Java exception a call throws
/// is caught and cleared, and comes back as the error: none is left pending to abort the
/// next JNI call.
pub fn with_env<T>(f: impl FnOnce(&mut Env) -> Result<T>) -> Result<T> {
    let ctx = android_context()?;
    // SAFETY: `ndk_context` holds the process's JavaVM, set by android-activity before
    // `android_main` runs and valid for the process's lifetime.
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) };
    vm.attach_current_thread(f)
}

/// The application context (`android.content.Context`), for calls that need no window.
pub fn context<'local>(env: &Env<'local>) -> Result<JObject<'local>> {
    let raw = android_context()?.context();
    // SAFETY: `ndk_context` holds the application context, a global reference valid for
    // the process's lifetime; the JObject borrows it and does not delete it.
    Ok(unsafe { JObject::from_raw(env, raw.cast()) })
}

/// The activity whose window the app draws in, set by the window: `ndk_context` holds the
/// *application* context, which has no window.
static ACTIVITY: AtomicUsize = AtomicUsize::new(0);

/// Remember the activity (`AndroidApp::activity_as_ptr`) for [`activity`].
pub fn set_activity(activity: *mut std::ffi::c_void) {
    ACTIVITY.store(activity as usize, Ordering::SeqCst);
}

/// The activity [`set_activity`] was given, or `None` before a window has set one.
pub fn activity<'local>(env: &Env<'local>) -> Option<JObject<'local>> {
    let raw = ACTIVITY.load(Ordering::SeqCst);
    // SAFETY: the activity is the global reference android-activity holds for as long as
    // the activity lives, which outlasts the window that asks.
    (raw != 0).then(|| unsafe { JObject::from_raw(env, raw as jni::sys::jobject) })
}

/// `ndk_context::android_context`, which panics when nothing has initialised it.
fn android_context() -> Result<ndk_context::AndroidContext> {
    std::panic::catch_unwind(ndk_context::android_context).map_err(|_| Error::UninitializedJavaVM)
}
