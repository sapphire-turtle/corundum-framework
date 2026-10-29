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
    // The activity's native method for results is registered now, not at the first picker:
    // a result of the app's own, forwarded before then, would find no native method and
    // throw.
    #[cfg(feature = "media-picker")]
    if let Err(err) = with_env(|env| match self::activity(env) {
        Some(activity) => forwards_results(env, &activity).map(drop),
        None => Ok(()),
    }) {
        tracing::warn!("could not register the activity's results: {err}");
    }
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

/// `Build.VERSION.SDK_INT`: the Android version's API level.
#[cfg(feature = "media-picker")]
pub(crate) fn sdk_int(env: &mut Env<'_>) -> Result<i32> {
    use jni::{jni_sig, jni_str};

    env.get_static_field(
        jni_str!("android/os/Build$VERSION"),
        jni_str!("SDK_INT"),
        jni_sig!("I"),
    )?
    .i()
}

#[cfg(feature = "media-picker")]
pub(crate) use results::{forwards_results, start_activity_for_result};

#[cfg(feature = "media-picker")]
mod results {
    //! Activity results, for the system's pickers started with `startActivityForResult`.
    //!
    //! A result comes back only to the activity's `onActivityResult`, which is Java: the app's
    //! activity forwards it to a native method, `corundumActivityResult`, which it declares
    //! and this module registers (`RegisterNatives`) on the activity's class, since the class
    //! is the app's and its name is not known here. An activity that does not declare the
    //! method cannot start a picker: [`start_activity_for_result`] says so, and nothing is
    //! started.

    use std::sync::Mutex;

    use jni::errors::{Error, LogErrorAndDefault, Result};
    use jni::objects::JObject;
    use jni::sys::jint;
    use jni::{Env, EnvUnowned, NativeMethod, jni_sig, jni_str};

    /// What to do with a request's result, on the activity's main thread: `resultCode` and
    /// the result's intent, which is null when there is none.
    pub(crate) type Handler = Box<dyn for<'a> FnOnce(&mut Env<'a>, i32, &JObject<'a>) + Send>;

    /// The requests started and not yet answered.
    static WAITING: Mutex<Vec<(i32, Handler)>> = Mutex::new(Vec::new());

    /// Whether the activity's class declares `corundumActivityResult`, once registered.
    static REGISTERED: Mutex<Option<bool>> = Mutex::new(None);

    /// Start `intent` for a result with `request_code`, and give the result to `handler`.
    /// `Ok(false)`, with nothing started, when there is no activity or its class does not
    /// forward results. A request started again with the same code replaces the first,
    /// whose handler is dropped unanswered.
    pub(crate) fn start_activity_for_result(
        env: &mut Env<'_>,
        intent: &JObject<'_>,
        request_code: i32,
        handler: Handler,
    ) -> Result<bool> {
        let Some(activity) = super::activity(env) else {
            return Ok(false);
        };
        if !forwards_results(env, &activity)? {
            return Ok(false);
        }
        let replaced = {
            let mut waiting = WAITING.lock().unwrap_or_else(|p| p.into_inner());
            let replaced = take(&mut waiting, request_code);
            waiting.push((request_code, handler));
            replaced
        };
        // Dropped outside the lock: dropping a handler answers its request.
        drop(replaced);
        let started = env.call_method(
            &activity,
            jni_str!("startActivityForResult"),
            jni_sig!("(Landroid/content/Intent;I)V"),
            &[intent.into(), request_code.into()],
        );
        if let Err(err) = started {
            let handler = take(
                &mut WAITING.lock().unwrap_or_else(|p| p.into_inner()),
                request_code,
            );
            drop(handler);
            return Err(err);
        }
        Ok(true)
    }

    /// The handler waiting for `request_code`, taken out.
    fn take(waiting: &mut Vec<(i32, Handler)>, request_code: i32) -> Option<Handler> {
        let at = waiting.iter().position(|(code, _)| *code == request_code)?;
        Some(waiting.remove(at).1)
    }

    /// Can the activity's class take results ([`start_activity_for_result`])? Registers
    /// the native method the first time.
    pub(crate) fn forwards_results(env: &mut Env<'_>, activity: &JObject<'_>) -> Result<bool> {
        let mut registered = REGISTERED.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(registered) = *registered {
            return Ok(registered);
        }
        let class = env.get_object_class(activity)?;
        // SAFETY: `activity_result` has the signature given, as an instance method: the env,
        // `this`, two ints and an object, returning nothing.
        let methods = [unsafe {
            NativeMethod::from_raw_parts(
                jni_str!("corundumActivityResult"),
                jni_str!("(IILandroid/content/Intent;)V"),
                activity_result as *mut std::ffi::c_void,
            )
        }];
        // SAFETY: the method is an instance method, and `activity_result` takes `this`.
        let answer = match unsafe { env.register_native_methods(&class, &methods) } {
            Ok(()) => true,
            Err(Error::NoSuchMethod(_)) => {
                tracing::warn!(
                    "the activity does not declare corundumActivityResult: no system pickers"
                );
                false
            }
            Err(err) => return Err(err),
        };
        *registered = Some(answer);
        Ok(answer)
    }

    /// `corundumActivityResult(int requestCode, int resultCode, Intent data)`, called by the
    /// app's `onActivityResult` on the activity's main thread.
    extern "system" fn activity_result<'local>(
        mut env: EnvUnowned<'local>,
        _this: JObject<'local>,
        request_code: jint,
        result_code: jint,
        data: JObject<'local>,
    ) {
        env.with_env(|env| -> Result<()> {
            let handler = take(
                &mut WAITING.lock().unwrap_or_else(|p| p.into_inner()),
                request_code,
            );
            // Another request code is the app's own, or another library's: not ours.
            if let Some(handler) = handler {
                handler(env, result_code, &data);
            }
            Ok(())
        })
        .resolve::<LogErrorAndDefault>()
    }
}
