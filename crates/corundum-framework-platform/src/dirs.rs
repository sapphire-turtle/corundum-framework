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
//! `dirs` has it on macOS: an app that keeps both must not give a file in each the same
//! name. On Windows the cache's root is that one too; [`AppDirs`] keeps the app's cache in
//! a `cache` folder of its directory there.
//!
//! These are the roots, shared by every app on a desktop. [`AppDirs`] is an app's own
//! directory under each: `<root>/<app name>` on the desktops, as the sapphire apps lay
//! theirs out (with the cache in `<root>\<app name>\cache` on Windows), and the root
//! itself on a phone, whose sandbox is the app's alone.
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
///
/// On Windows this is the same root as [`config_dir`] and [`data_dir`] (Local): a caller
/// that uses the roots directly keeps its cache in a folder of its own, apart from its
/// settings and data, as [`AppDirs::cache`] does.
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
/// they are already the app's. On Windows, where the three roots are one, the cache is
/// `<root>\<app name>\cache`, so that clearing it never takes the settings or data with
/// it. Nothing is created; the app makes them as it writes.
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
        let layout = Layout::current();
        Some(AppDirs {
            config: own(layout, &config_dir()?, app_name),
            data: own(layout, &data_dir()?, app_name),
            cache: own_cache(layout, &cache_dir()?, app_name),
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
    /// With no Local directory yet, the Roaming one is renamed to it whole. With one (its
    /// cache, written before this ran, makes one), the two are merged: each entry at the
    /// top of the Roaming directory that Local does not have is renamed into it, one at a
    /// time; an entry Local has too is left in Roaming and logged, and the Roaming
    /// directory is removed once it is empty. Nothing is copied, so nothing is ever half
    /// there: when a rename fails (a file in use, Roaming on another volume) the error is
    /// returned, what was moved stays moved, and the rest is moved at a later start.
    /// Outside Windows there is no Roaming, and nothing is moved.
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
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Migrated {
    /// The Roaming directory was renamed to Local whole.
    Moved,
    /// There was a Local directory already, and the Roaming one was merged into it. `left`
    /// are the Roaming entries Local had too, left where they were; when it is empty the
    /// Roaming directory is gone.
    Merged { left: Vec<PathBuf> },
    /// There was no Roaming directory to move, or the platform has none.
    Nothing,
}

/// What to do with an app's Roaming directory.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Migration {
    Move,
    Merge,
    Nothing,
}

/// What to do with an app's Roaming directory, from whether it and the Local one exist.
fn migration(roaming_exists: bool, local_exists: bool) -> Migration {
    match (roaming_exists, local_exists) {
        (true, false) => Migration::Move,
        (true, true) => Migration::Merge,
        (false, _) => Migration::Nothing,
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
    match migration(roaming.try_exists()?, local.try_exists()?) {
        Migration::Move => {
            std::fs::create_dir_all(local_root)?;
            // One rename: the directory is either all in Roaming or all in Local.
            std::fs::rename(&roaming, &local)?;
            tracing::info!("moved {} to {}", roaming.display(), local.display());
            Ok(Migrated::Moved)
        }
        Migration::Merge => merge(&roaming, &local),
        Migration::Nothing => Ok(Migrated::Nothing),
    }
}

/// Rename each entry of `roaming` that `local` does not have into `local`, and remove
/// `roaming` if that empties it.
fn merge(roaming: &Path, local: &Path) -> io::Result<Migrated> {
    let mut names = std::fs::read_dir(roaming)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<io::Result<Vec<_>>>()?;
    // Sorted, so the entries are moved and reported in the same order on every start.
    names.sort();
    let mut left = Vec::new();
    for name in names {
        let from = roaming.join(&name);
        let to = local.join(&name);
        if to.try_exists()? {
            left.push(from);
        } else {
            std::fs::rename(&from, &to)?;
            tracing::info!("moved {} to {}", from.display(), to.display());
        }
    }
    if left.is_empty() {
        std::fs::remove_dir(roaming)?;
    } else {
        let names: Vec<_> = left.iter().map(|path| path.display().to_string()).collect();
        tracing::warn!(
            "{} has these too, kept; left in Roaming: {}",
            local.display(),
            names.join(", ")
        );
    }
    Ok(Migrated::Merged { left })
}

/// How a platform lays out an app's directories under the roots.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Layout {
    /// A phone: the roots are the app's sandbox already.
    Phone,
    /// Windows: one root (Local) for config, data and cache.
    Windows,
    /// macOS and Linux: a root of its own for the cache.
    Desktop,
}

impl Layout {
    fn current() -> Layout {
        if cfg!(any(target_os = "android", target_os = "ios")) {
            Layout::Phone
        } else if cfg!(windows) {
            Layout::Windows
        } else {
            Layout::Desktop
        }
    }
}

/// An app's settings or data directory under `root`: its own on the desktops, and `root`
/// on a phone.
fn own(layout: Layout, root: &Path, app_name: &str) -> PathBuf {
    match layout {
        Layout::Phone => root.to_path_buf(),
        Layout::Windows | Layout::Desktop => root.join(app_name),
    }
}

/// An app's cache directory under `root`: as [`own`], but a folder of the app's directory
/// on Windows, where the cache's root is the settings' and data's too.
fn own_cache(layout: Layout, root: &Path, app_name: &str) -> PathBuf {
    match layout {
        Layout::Windows => root.join(app_name).join("cache"),
        Layout::Phone | Layout::Desktop => own(layout, root, app_name),
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
        assert_eq!(migration(true, false), Migration::Move);
    }

    #[test]
    fn a_roaming_directory_is_merged_into_a_local_one() {
        assert_eq!(migration(true, true), Migration::Merge);
    }

    #[test]
    fn without_a_roaming_directory_there_is_nothing_to_move() {
        assert_eq!(migration(false, false), Migration::Nothing);
        assert_eq!(migration(false, true), Migration::Nothing);
    }

    #[test]
    fn a_desktop_keeps_an_app_under_each_root() {
        let root = Path::new("/root");
        assert_eq!(own(Layout::Desktop, root, "app"), root.join("app"));
        assert_eq!(own_cache(Layout::Desktop, root, "app"), root.join("app"));
    }

    #[test]
    fn windows_keeps_the_cache_in_a_folder_of_the_apps_directory() {
        let root = Path::new("/root");
        assert_eq!(own(Layout::Windows, root, "app"), root.join("app"));
        assert_eq!(
            own_cache(Layout::Windows, root, "app"),
            root.join("app").join("cache")
        );
    }

    #[test]
    fn a_phone_uses_the_roots_themselves() {
        let root = Path::new("/root");
        assert_eq!(own(Layout::Phone, root, "app"), root);
        assert_eq!(own_cache(Layout::Phone, root, "app"), root);
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
    fn a_local_directory_a_cache_made_takes_in_all_of_roaming() {
        let (_temp, roaming, local) = roots();
        std::fs::create_dir_all(roaming.join("app/data")).unwrap();
        std::fs::write(roaming.join("app/config.toml"), "a").unwrap();
        std::fs::write(roaming.join("app/data/index"), "b").unwrap();
        std::fs::create_dir_all(local.join("app/cache")).unwrap();
        std::fs::write(local.join("app/cache/sample"), "c").unwrap();

        let migrated = migrate_from_roaming_with(&roaming, &local, "app").unwrap();

        assert_eq!(migrated, Migrated::Merged { left: vec![] });
        assert!(!roaming.join("app").exists());
        assert_eq!(
            std::fs::read_to_string(local.join("app/config.toml")).unwrap(),
            "a"
        );
        assert_eq!(
            std::fs::read_to_string(local.join("app/data/index")).unwrap(),
            "b"
        );
        assert_eq!(
            std::fs::read_to_string(local.join("app/cache/sample")).unwrap(),
            "c"
        );
    }

    #[test]
    fn an_entry_local_has_too_is_left_in_roaming_and_reported() {
        let (_temp, roaming, local) = roots();
        std::fs::create_dir_all(roaming.join("app")).unwrap();
        std::fs::write(roaming.join("app/config.toml"), "roaming").unwrap();
        std::fs::write(roaming.join("app/other.toml"), "moved").unwrap();
        std::fs::create_dir_all(local.join("app")).unwrap();
        std::fs::write(local.join("app/config.toml"), "local").unwrap();

        let migrated = migrate_from_roaming_with(&roaming, &local, "app").unwrap();

        assert_eq!(
            migrated,
            Migrated::Merged {
                left: vec![roaming.join("app/config.toml")]
            }
        );
        assert_eq!(
            std::fs::read_to_string(roaming.join("app/config.toml")).unwrap(),
            "roaming"
        );
        assert_eq!(
            std::fs::read_to_string(local.join("app/config.toml")).unwrap(),
            "local"
        );
        assert!(!roaming.join("app/other.toml").exists());
        assert_eq!(
            std::fs::read_to_string(local.join("app/other.toml")).unwrap(),
            "moved"
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
        let layout = Layout::current();
        let app = "corundum-test";
        assert_eq!(dirs.config(), own(layout, &config_dir().unwrap(), app));
        assert_eq!(dirs.data(), own(layout, &data_dir().unwrap(), app));
        assert_eq!(dirs.cache(), own_cache(layout, &cache_dir().unwrap(), app));
    }

    // The cache can be cleared whole without touching the settings or the data.
    #[test]
    fn an_apps_cache_is_never_its_config_or_data() {
        let Some(dirs) = AppDirs::new("corundum-test") else {
            return;
        };
        if cfg!(any(target_os = "android", target_os = "ios")) {
            return;
        }
        assert!(!dirs.config().starts_with(dirs.cache()));
        assert!(!dirs.data().starts_with(dirs.cache()));
    }
}
