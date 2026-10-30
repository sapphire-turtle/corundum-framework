//! Where an app keeps its settings, data and cache, on the desktops and on phones alike.
//!
//! On the desktops these are the `dirs` crate's: the XDG directories on Linux, Application
//! Support and Caches on macOS, the known folders on Windows. On Windows all three are in
//! Local (`%LOCALAPPDATA%`), not Roaming (`%APPDATA%`): Roaming follows the user to the
//! other PCs of a domain, and an app's search index, its logs or a device's own key must
//! not, as the apps sync themselves. A phone's app is sandboxed and `dirs` cannot see its
//! sandbox, so:
//!
//! - **iOS**: Application Support (config and data) and Caches, from `NSFileManager`, as
//!   macOS has them.
//! - **Android**: the context's `getFilesDir` (config and data) and `getCacheDir`, read
//!   through JNI. A process android-activity did not start has no context to ask, and
//!   these are `None` there.
//!
//! Config and data are one directory on Apple's platforms, on Android and on Windows, as
//! `dirs` has it on macOS, and on Windows the cache is that directory too: an app that
//! keeps more than one must not give a file in each the same name.
//!
//! These are the roots, shared by every app on a desktop. [`AppDirs`] is an app's own
//! directory under each: `<root>/<app name>` on the desktops, as the sapphire apps lay
//! theirs out, and the root itself on a phone, whose sandbox is the app's alone.
//!
//! An app that kept its directory in Roaming before calls
//! [`AppDirs::migrate_from_roaming`] once at start, before it reads anything there.

use std::io;
use std::path::{Path, PathBuf};

/// The root the apps keep their settings in, or `None` where the platform has none.
pub fn config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    return android::files_dir();
    #[cfg(target_os = "ios")]
    return apple::directory(apple::Kind::ApplicationSupport);
    // The local one: Local rather than Roaming on Windows, the same as `config_dir` on
    // macOS and Linux.
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    return ::dirs::config_local_dir();
}

/// The root the apps keep their data in, or `None` where the platform has none.
pub fn data_dir() -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    return android::files_dir();
    #[cfg(target_os = "ios")]
    return apple::directory(apple::Kind::ApplicationSupport);
    // As in `config_dir`: Local on Windows, the same as `data_dir` elsewhere.
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    return ::dirs::data_local_dir();
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

/// An app's own directories under [`config_dir`], [`data_dir`] and [`cache_dir`]:
/// `<root>/<app name>` on the desktops, and the roots themselves on iOS and Android, where
/// they are already the app's. Nothing is created; the app makes them as it writes.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AppDirs {
    config: PathBuf,
    data: PathBuf,
    cache: PathBuf,
}

impl AppDirs {
    /// `app_name`'s directories (`sapphire-journal`, say), or `None` where the platform
    /// has no root for one of them. A phone does not use `app_name`.
    pub fn new(app_name: &str) -> Option<AppDirs> {
        Some(AppDirs {
            config: own(config_dir()?, app_name),
            data: own(data_dir()?, app_name),
            cache: own(cache_dir()?, app_name),
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

    /// Move `app_name`'s directory from Roaming (`%APPDATA%\<app>`), where the apps kept
    /// their settings and data before, to Local (`%LOCALAPPDATA%\<app>`), where
    /// [`config_dir`] and [`data_dir`] are now. An app calls this once at start, before it
    /// reads its settings or data.
    ///
    /// It moves only when there is a Roaming directory and no Local one yet; an existing
    /// Local directory is the app's, and the Roaming one is left as it is. The directory is
    /// renamed, not copied: when that fails (a file in use, Roaming on another volume) the
    /// error is returned and the Roaming directory stays whole, to be moved at a later
    /// start. Outside Windows there is no Roaming, and nothing is moved.
    pub fn migrate_from_roaming(app_name: &str) -> io::Result<Migrated> {
        #[cfg(windows)]
        return match (::dirs::config_dir(), ::dirs::data_local_dir()) {
            (Some(roaming), Some(local)) => migrate_from_roaming_with(&roaming, &local, app_name),
            _ => Ok(Migrated::Nothing),
        };
        #[cfg(not(windows))]
        {
            let _ = app_name;
            Ok(Migrated::Nothing)
        }
    }
}

/// What [`AppDirs::migrate_from_roaming`] did.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Migrated {
    /// The Roaming directory was moved to Local.
    Moved,
    /// Both were there: the Local one is kept, and the Roaming one is left as it is.
    KeptLocal,
    /// There was no Roaming directory to move, or the platform has none.
    Nothing,
}

/// What to do with an app's Roaming directory, from whether it and the Local one exist.
fn migration(roaming_exists: bool, local_exists: bool) -> Migrated {
    match (roaming_exists, local_exists) {
        (true, false) => Migrated::Moved,
        (true, true) => Migrated::KeptLocal,
        (false, _) => Migrated::Nothing,
    }
}

/// [`AppDirs::migrate_from_roaming`] between the roots given, so it can be tried on any OS.
#[cfg_attr(not(windows), allow(dead_code))]
fn migrate_from_roaming_with(
    roaming_root: &Path,
    local_root: &Path,
    app_name: &str,
) -> io::Result<Migrated> {
    let roaming = roaming_root.join(app_name);
    let local = local_root.join(app_name);
    let migrated = migration(roaming.try_exists()?, local.try_exists()?);
    match migrated {
        Migrated::Moved => {
            std::fs::create_dir_all(local_root)?;
            // One rename: the directory is either all in Roaming or all in Local.
            std::fs::rename(&roaming, &local)?;
            tracing::info!("moved {} to {}", roaming.display(), local.display());
        }
        Migrated::KeptLocal => tracing::info!(
            "both {} and {} exist: keeping the second, the first is left as it is",
            roaming.display(),
            local.display()
        ),
        Migrated::Nothing => {}
    }
    Ok(migrated)
}

/// An app's directory under `root`: its own on the desktops, and `root` on a phone.
fn own(root: PathBuf, app_name: &str) -> PathBuf {
    if cfg!(any(target_os = "android", target_os = "ios")) {
        root
    } else {
        root.join(app_name)
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
    fn the_desktops_roots_are_dirs_local_ones() {
        assert_eq!(config_dir(), ::dirs::config_local_dir());
        assert_eq!(data_dir(), ::dirs::data_local_dir());
        assert_eq!(cache_dir(), ::dirs::cache_dir());
    }

    // macOS and Linux have no roaming directories: their roots are what they were.
    #[cfg(all(not(windows), not(any(target_os = "android", target_os = "ios"))))]
    #[test]
    fn the_roots_outside_windows_are_the_plain_ones() {
        assert_eq!(config_dir(), ::dirs::config_dir());
        assert_eq!(data_dir(), ::dirs::data_dir());
    }

    #[test]
    fn only_a_roaming_directory_is_moved() {
        assert_eq!(migration(true, false), Migrated::Moved);
    }

    #[test]
    fn a_local_directory_is_kept_over_a_roaming_one() {
        assert_eq!(migration(true, true), Migrated::KeptLocal);
    }

    #[test]
    fn without_a_roaming_directory_there_is_nothing_to_move() {
        assert_eq!(migration(false, false), Migrated::Nothing);
        assert_eq!(migration(false, true), Migrated::Nothing);
    }

    #[cfg(not(windows))]
    #[test]
    fn nothing_is_moved_outside_windows() {
        assert_eq!(
            AppDirs::migrate_from_roaming("corundum-test").unwrap(),
            Migrated::Nothing
        );
    }

    /// `Roaming` and `Local` roots under a temporary directory.
    fn roots() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let roaming = temp.path().join("Roaming");
        let local = temp.path().join("Local");
        (temp, roaming, local)
    }

    #[test]
    fn the_roaming_directory_moves_to_local_with_its_files() {
        let (_temp, roaming, local) = roots();
        std::fs::create_dir_all(roaming.join("app/sub")).unwrap();
        std::fs::write(roaming.join("app/settings.toml"), "a").unwrap();
        std::fs::write(roaming.join("app/sub/data"), "b").unwrap();

        let migrated = migrate_from_roaming_with(&roaming, &local, "app").unwrap();

        assert_eq!(migrated, Migrated::Moved);
        assert!(!roaming.join("app").exists());
        assert_eq!(
            std::fs::read_to_string(local.join("app/settings.toml")).unwrap(),
            "a"
        );
        assert_eq!(
            std::fs::read_to_string(local.join("app/sub/data")).unwrap(),
            "b"
        );
    }

    #[test]
    fn an_existing_local_directory_leaves_both_as_they_are() {
        let (_temp, roaming, local) = roots();
        std::fs::create_dir_all(roaming.join("app")).unwrap();
        std::fs::write(roaming.join("app/settings.toml"), "roaming").unwrap();
        std::fs::create_dir_all(local.join("app")).unwrap();
        std::fs::write(local.join("app/settings.toml"), "local").unwrap();

        let migrated = migrate_from_roaming_with(&roaming, &local, "app").unwrap();

        assert_eq!(migrated, Migrated::KeptLocal);
        assert_eq!(
            std::fs::read_to_string(roaming.join("app/settings.toml")).unwrap(),
            "roaming"
        );
        assert_eq!(
            std::fs::read_to_string(local.join("app/settings.toml")).unwrap(),
            "local"
        );
    }

    #[test]
    fn no_roaming_directory_makes_no_local_one() {
        let (_temp, roaming, local) = roots();

        let migrated = migrate_from_roaming_with(&roaming, &local, "app").unwrap();

        assert_eq!(migrated, Migrated::Nothing);
        assert!(!local.join("app").exists());
    }

    #[test]
    fn a_failed_move_leaves_the_roaming_directory_whole() {
        let (_temp, roaming, local) = roots();
        std::fs::create_dir_all(roaming.join("app")).unwrap();
        std::fs::write(roaming.join("app/settings.toml"), "a").unwrap();
        // A file where the Local root should be: the move cannot happen.
        std::fs::write(&local, "").unwrap();

        assert!(migrate_from_roaming_with(&roaming, &local, "app").is_err());
        assert_eq!(
            std::fs::read_to_string(roaming.join("app/settings.toml")).unwrap(),
            "a"
        );
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
        let own = |root: Option<PathBuf>| {
            let root = root.unwrap();
            if cfg!(any(target_os = "android", target_os = "ios")) {
                root
            } else {
                root.join("corundum-test")
            }
        };
        assert_eq!(dirs.config(), own(config_dir()));
        assert_eq!(dirs.data(), own(data_dir()));
        assert_eq!(dirs.cache(), own(cache_dir()));
    }
}
