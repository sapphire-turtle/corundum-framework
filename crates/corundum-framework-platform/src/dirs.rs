//! Where an app keeps its settings, data and cache, on the desktops and on phones alike.
//!
//! On the desktops these are the `dirs` crate's: the XDG directories on Linux, Application
//! Support and Caches on macOS, the known folders on Windows. A phone's app is sandboxed
//! and `dirs` cannot see its sandbox, so:
//!
//! - **iOS**: Application Support (config and data) and Caches, from `NSFileManager`, as
//!   macOS has them.
//! - **Android**: the context's `getFilesDir` (config and data) and `getCacheDir`, read
//!   through JNI. A process android-activity did not start has no context to ask, and
//!   these are `None` there.
//!
//! Config and data are one directory on Apple's platforms and on Android, as `dirs` has it
//! on macOS: an app that keeps both must not give a file in each the same name.
//!
//! These are the roots, shared by every app on a desktop. [`AppDirs`] is an app's own
//! directory under each, `<root>/<app name>`, as the sapphire apps lay theirs out.

use std::path::{Path, PathBuf};

/// The root the apps keep their settings in, or `None` where the platform has none.
pub fn config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    return android::files_dir();
    #[cfg(target_os = "ios")]
    return apple::directory(apple::Kind::ApplicationSupport);
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    return ::dirs::config_dir();
}

/// The root the apps keep their data in, or `None` where the platform has none.
pub fn data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    return android::files_dir();
    #[cfg(target_os = "ios")]
    return apple::directory(apple::Kind::ApplicationSupport);
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    return ::dirs::data_dir();
}

/// The root the apps keep their cache in, which the system may clear, or `None` where the
/// platform has none.
pub fn cache_dir() -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    return android::cache_dir();
    #[cfg(target_os = "ios")]
    return apple::directory(apple::Kind::Caches);
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    return ::dirs::cache_dir();
}

/// An app's own directories: `<root>/<app name>` under [`config_dir`], [`data_dir`] and
/// [`cache_dir`]. Nothing is created; the app makes them as it writes.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AppDirs {
    config: PathBuf,
    data: PathBuf,
    cache: PathBuf,
}

impl AppDirs {
    /// `app_name`'s directories (`sapphire-journal`, say), or `None` where the platform
    /// has no root for one of them.
    pub fn new(app_name: &str) -> Option<AppDirs> {
        Some(AppDirs {
            config: config_dir()?.join(app_name),
            data: data_dir()?.join(app_name),
            cache: cache_dir()?.join(app_name),
        })
    }

    /// Where the app keeps its settings.
    pub fn config(&self) -> &Path {
        &self.config
    }

    /// Where the app keeps its data.
    pub fn data(&self) -> &Path {
        &self.data
    }

    /// Where the app keeps its cache.
    pub fn cache(&self) -> &Path {
        &self.cache
    }
}

#[cfg(target_os = "ios")]
mod apple {
    use std::path::PathBuf;

    use objc2_foundation::{NSFileManager, NSSearchPathDirectory, NSSearchPathDomainMask};

    pub(super) enum Kind {
        ApplicationSupport,
        Caches,
    }

    /// The sandbox's directory of `kind`, as `NSFileManager` gives it.
    pub(super) fn directory(kind: Kind) -> Option<PathBuf> {
        let kind = match kind {
            Kind::ApplicationSupport => NSSearchPathDirectory::ApplicationSupportDirectory,
            Kind::Caches => NSSearchPathDirectory::CachesDirectory,
        };
        let urls = NSFileManager::defaultManager()
            .URLsForDirectory_inDomains(kind, NSSearchPathDomainMask::UserDomainMask);
        let path = urls.firstObject()?.path()?;
        Some(PathBuf::from(path.to_string()))
    }
}

#[cfg(target_os = "android")]
mod android {
    use std::path::PathBuf;

    use jni::objects::JString;
    use jni::strings::JNIStr;
    use jni::{jni_sig, jni_str};

    pub(super) fn files_dir() -> Option<PathBuf> {
        context_dir(jni_str!("getFilesDir"))
    }

    pub(super) fn cache_dir() -> Option<PathBuf> {
        context_dir(jni_str!("getCacheDir"))
    }

    /// `context.<getter>().getAbsolutePath()`, or `None` when the JVM cannot be reached.
    fn context_dir(getter: &JNIStr) -> Option<PathBuf> {
        crate::android::with_env(|env| {
            let context = crate::android::context(env)?;
            let file = env
                .call_method(&context, getter, jni_sig!("()Ljava/io/File;"), &[])?
                .l()?;
            let path = env
                .call_method(
                    &file,
                    jni_str!("getAbsolutePath"),
                    jni_sig!("()Ljava/lang/String;"),
                    &[],
                )?
                .l()?;
            env.cast_local::<JString>(path)?.try_to_string(env)
        })
        .inspect_err(|err| tracing::debug!("could not read the app's directory: {err}"))
        .ok()
        .map(PathBuf::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[test]
    fn the_desktops_roots_are_dirs() {
        assert_eq!(config_dir(), ::dirs::config_dir());
        assert_eq!(data_dir(), ::dirs::data_dir());
        assert_eq!(cache_dir(), ::dirs::cache_dir());
    }

    // On a phone too: the iOS simulator gives its sandbox's directories, and an Android
    // test binary (no context) gives none.
    #[test]
    fn the_roots_are_absolute_where_there_are_any() {
        for dir in [config_dir(), data_dir(), cache_dir()]
            .into_iter()
            .flatten()
        {
            assert!(dir.is_absolute(), "{dir:?} is not absolute");
        }
    }

    #[test]
    fn an_apps_directories_are_under_the_roots() {
        let Some(dirs) = AppDirs::new("corundum-test") else {
            return;
        };
        assert_eq!(dirs.config(), config_dir().unwrap().join("corundum-test"));
        assert_eq!(dirs.data(), data_dir().unwrap().join("corundum-test"));
        assert_eq!(dirs.cache(), cache_dir().unwrap().join("corundum-test"));
    }
}
