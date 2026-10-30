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
//! [`AppDirs::migrate_from_roaming`] once at start, before it reads anything there, and
//! does not start on an error from it.
//!
//! On Windows the cache used to be the app's directory itself (`%LOCALAPPDATA%\<app>`).
//! An existing user's Local directory may still hold old cache entries at its top level,
//! beside the settings and data moved in from Roaming; they are not moved, and the app may
//! clear them.

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
    /// directory is removed once it is empty. A rename never replaces what is there.
    ///
    /// Roaming on another volume (redirected AppData) cannot be renamed into Local, so
    /// there the directory, or each entry, is copied next to its place
    /// (`<app>.corundum-migrating`, `<app>\.corundum-migrating-<name>`; a Roaming entry
    /// named with that prefix is never moved), renamed into place, and only then removed
    /// from Roaming. Nothing is ever half there: a copy that fails is removed and its
    /// source stays in Roaming. Two cases leave an entry in Roaming, reported in
    /// [`Migrated::Merged`]'s `left` (after a whole move, at the next start), and do not
    /// stop the start:
    ///
    /// - A symlink or junction in it that cannot be made again in Local (Windows lets only
    ///   an administrator, or developer mode, make one): the entry is not copied at all.
    /// - A source that cannot be removed once copied (a file in it still open in an old
    ///   process): Local holds the copy taken at that moment, and the Roaming entry stays,
    ///   reported again at later starts as one Local has already.
    ///
    /// One start moves at a time: the others find the lock
    /// (`%LOCALAPPDATA%\<app>.migrate.lock`, removed once Roaming is gone) held, and return
    /// an error of kind [`io::ErrorKind::WouldBlock`], touching nothing.
    ///
    /// A Local directory that is a Velopack install (`Update.exe`, or `current` with
    /// `packages`: one made under a `packId` equal to the app's name) is never merged into,
    /// as an uninstall would delete it whole; this returns an error naming it. An app's
    /// `packId` must differ from its data directory's name (corundum-framework#57).
    ///
    /// **When this returns an error, the app must not read or write its settings or data in
    /// this run**: the move may have stopped halfway, some entries in Local and the rest
    /// still in Roaming, and an app that ran on Local alone would start over without them,
    /// then never move them. It reports the error and exits; the next start goes on from
    /// where this one stopped, once whatever stopped it (a file in use, another start
    /// moving) is gone.
    ///
    /// Outside Windows there is no Roaming, and nothing is moved.
    pub fn migrate_from_roaming(app_name: &str) -> io::Result<Migrated> {
        #[cfg(windows)]
        return match (::dirs::config_dir(), ::dirs::data_local_dir()) {
            (Some(roaming), Some(local)) => {
                migrate_from_roaming_with(&roaming, &local, app_name, &rename_no_replace)
            }
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
    /// The Roaming directory was moved to Local whole.
    Moved,
    /// The Roaming directory was merged into Local entry by entry: there was a Local
    /// directory already, or a link deep in Roaming kept it from being copied whole. `left`
    /// are the Roaming entries left where they were (Local had them already, they have
    /// the reserved prefix, or they could not be moved whole); when it is empty the
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

/// A rename that fails rather than replace its destination, as [`rename_no_replace`].
type Rename<'a> = dyn Fn(&Path, &Path) -> io::Result<()> + 'a;

/// The prefix of the temporary copies a move across volumes makes, which nothing else
/// names an entry with: `<app>.corundum-migrating` beside the app's directory and
/// `.corundum-migrating-<name>` in it. A Roaming entry named so is never moved.
const TEMPORARY_PREFIX: &str = ".corundum-migrating";

/// [`AppDirs::migrate_from_roaming`] between the roots given and through `rename`, so it
/// can be tried on any OS, Roaming on another volume included.
#[cfg_attr(not(windows), allow(dead_code))]
fn migrate_from_roaming_with(
    roaming_root: &Path,
    local_root: &Path,
    app_name: &str,
    rename: &Rename,
) -> io::Result<Migrated> {
    let roaming = roaming_root.join(app_name);
    let local = local_root.join(app_name);
    // Most starts: nothing to move, and no lock file made for it.
    if !is_there(&roaming)? {
        return Ok(Migrated::Nothing);
    }
    std::fs::create_dir_all(local_root)?;
    let lock_path = local_root.join(format!("{app_name}.migrate.lock"));
    let lock = std::fs::File::options()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)?;
    // One start at a time: two copying across volumes would share the temporary copies,
    // and one could swap in what the other had half made.
    match lock.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "another start is moving the app's data",
            ));
        }
        Err(std::fs::TryLockError::Error(err)) => return Err(err),
    }
    // Decided again under the lock: another start may have finished meanwhile.
    let roaming_there = is_there(&roaming)?;
    // What a copy cut off by a crash left is no use, and no other start is making it now.
    let root_temporary = local_root.join(format!("{app_name}{TEMPORARY_PREFIX}"));
    remove(&root_temporary)?;
    let migrated = match migration(roaming_there, is_there(&local)?) {
        Migration::Move => match move_entry(&roaming, &local, &root_temporary, rename)? {
            Outcome::Moved | Outcome::SourceKept => {
                tracing::info!("moved {} to {}", roaming.display(), local.display());
                Migrated::Moved
            }
            // A link deep in it could not be made: the entries are moved one by one, and
            // the one holding the link is left.
            Outcome::LinkLeft => {
                std::fs::create_dir(&local)?;
                merge(&roaming, &local, rename)?
            }
        },
        Migration::Merge => {
            if is_velopack_install(&local)? {
                return Err(io::Error::other(format!(
                    "{} is a Velopack install, not merged into: an app's packId must differ \
                     from its data directory's name; {} is left as it is",
                    local.display(),
                    roaming.display()
                )));
            }
            merge(&roaming, &local, rename)?
        }
        Migration::Nothing => Migrated::Nothing,
    };
    // With Roaming gone no start needs the lock again. One that opened it before this
    // removes it finds Roaming gone too, and does nothing.
    if !is_there(&roaming).unwrap_or(true) {
        let _ = std::fs::remove_file(&lock_path);
    }
    Ok(migrated)
}

/// Rename each entry of `roaming` that `local` does not have into `local`, and remove
/// `roaming` if that empties it. The first entry that fails to move stops it; one that
/// cannot be moved whole (a link that cannot be made, a source that cannot be removed) is
/// left and reported.
fn merge(roaming: &Path, local: &Path, rename: &Rename) -> io::Result<Migrated> {
    // What copies cut off by a crash left: the caller holds the lock.
    for entry in std::fs::read_dir(local)? {
        let entry = entry?;
        if is_temporary(&entry.file_name()) {
            remove(&entry.path())?;
        }
    }
    let mut names = std::fs::read_dir(roaming)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<io::Result<Vec<_>>>()?;
    // Sorted, so the entries are moved and reported in the same order on every start.
    names.sort();
    let mut left = Vec::new();
    for name in names {
        let from = roaming.join(&name);
        let to = local.join(&name);
        if is_temporary(&name) || is_there(&to)? {
            left.push(from);
            continue;
        }
        let mut temporary_name = std::ffi::OsString::from(format!("{TEMPORARY_PREFIX}-"));
        temporary_name.push(&name);
        let temporary = local.join(temporary_name);
        match move_entry(&from, &to, &temporary, rename) {
            Ok(Outcome::Moved) => {
                tracing::info!("moved {} to {}", from.display(), to.display());
            }
            Ok(Outcome::SourceKept | Outcome::LinkLeft) => left.push(from),
            // Made in Local since it was looked at: Local's is the one kept.
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => left.push(from),
            Err(err) => return Err(err),
        }
    }
    if left.is_empty() {
        std::fs::remove_dir(roaming)?;
    } else {
        let names: Vec<_> = left.iter().map(|path| path.display().to_string()).collect();
        tracing::warn!(
            "not moved into {} (it has them already, or they could not be moved whole), \
             left in Roaming: {}",
            local.display(),
            names.join(", ")
        );
    }
    Ok(Migrated::Merged { left })
}

/// Whether `name` is one of the temporary copies' (or would be taken for one).
fn is_temporary(name: &std::ffi::OsStr) -> bool {
    name.as_encoded_bytes()
        .starts_with(TEMPORARY_PREFIX.as_bytes())
}

/// What [`move_entry`] did.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Outcome {
    /// Moved: in Local, gone from Roaming.
    Moved,
    /// Copied into Local, but the source could not be removed (a file in it still open):
    /// Local holds the copy taken then, and the source stays in Roaming.
    SourceKept,
    /// Not moved: a link in it could not be made in Local. It stays in Roaming whole.
    LinkLeft,
}

/// Move `from` to `to`, which is not there: one rename, or across volumes a copy to
/// `temporary` (beside `to`), renamed to `to`, and only then `from` removed.
fn move_entry(from: &Path, to: &Path, temporary: &Path, rename: &Rename) -> io::Result<Outcome> {
    match rename(from, to) {
        Ok(()) => return Ok(Outcome::Moved),
        Err(err) if crosses_volumes(&err) => {}
        Err(err) => return Err(err),
    }
    // What a copy cut off before left is no use: it is made again whole.
    remove(temporary)?;
    if let Err(err) = copy(from, temporary).and_then(|()| rename(temporary, to)) {
        if let Err(remove_err) = remove(temporary) {
            tracing::warn!("could not remove {}: {remove_err}", temporary.display());
        }
        if err
            .get_ref()
            .is_some_and(|inner| inner.is::<LinkNotCopied>())
        {
            tracing::warn!("{} is left in Roaming: {err}", from.display());
            return Ok(Outcome::LinkLeft);
        }
        return Err(err);
    }
    match remove(from) {
        Ok(()) => Ok(Outcome::Moved),
        Err(err) => {
            tracing::warn!(
                "copied {} to {}, but could not remove it: {err}",
                from.display(),
                to.display()
            );
            Ok(Outcome::SourceKept)
        }
    }
}

/// Whether `err` is a rename's from one volume to another.
fn crosses_volumes(err: &io::Error) -> bool {
    // ERROR_NOT_SAME_DEVICE, should std not map it.
    err.kind() == io::ErrorKind::CrossesDevices || (cfg!(windows) && err.raw_os_error() == Some(17))
}

/// Copy `from` to `to`, which is not there: files, directories, and symlinks as symlinks.
fn copy(from: &Path, to: &Path) -> io::Result<()> {
    let kind = std::fs::symlink_metadata(from)?.file_type();
    if kind.is_symlink() {
        copy_symlink(from, to)
    } else if kind.is_dir() {
        std::fs::create_dir(to)?;
        for entry in std::fs::read_dir(from)? {
            let name = entry?.file_name();
            copy(&from.join(&name), &to.join(&name))?;
        }
        Ok(())
    } else {
        std::fs::copy(from, to).map(|_| ())
    }
}

/// A symlink or junction that could not be made again in Local: Windows lets only an
/// administrator, or a machine in developer mode, make one.
#[derive(Debug)]
struct LinkNotCopied {
    path: PathBuf,
    err: io::Error,
}

impl std::fmt::Display for LinkNotCopied {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the link {} could not be made again: {}",
            self.path.display(),
            self.err
        )
    }
}

impl std::error::Error for LinkNotCopied {}

#[cfg(test)]
thread_local! {
    /// Makes [`copy_symlink`] fail, as an unprivileged Windows user's would.
    static FAIL_LINKS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Make `to` a symlink to where `from` points.
fn copy_symlink(from: &Path, to: &Path) -> io::Result<()> {
    make_symlink(from, to).map_err(|err| {
        io::Error::other(LinkNotCopied {
            path: from.to_path_buf(),
            err,
        })
    })
}

fn make_symlink(from: &Path, to: &Path) -> io::Result<()> {
    #[cfg(test)]
    if FAIL_LINKS.get() {
        return Err(io::Error::from(io::ErrorKind::PermissionDenied));
    }
    let target = std::fs::read_link(from)?;
    #[cfg(unix)]
    return std::os::unix::fs::symlink(target, to);
    #[cfg(windows)]
    {
        // Windows tells a link to a directory from one to a file; a link to nothing is
        // made to a file.
        let to_dir = std::fs::metadata(from).is_ok_and(|meta| meta.is_dir());
        if to_dir {
            std::os::windows::fs::symlink_dir(target, to)
        } else {
            std::os::windows::fs::symlink_file(target, to)
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (target, to);
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }
}

/// Remove `path`, a directory with all in it or a file or symlink, if it is there.
fn remove(path: &Path) -> io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path).or_else(|err| {
            // A symlink to a directory on Windows is removed as a directory.
            if cfg!(windows) {
                std::fs::remove_dir(path)
            } else {
                Err(err)
            }
        }),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

/// Whether there is anything at `path`, a dangling symlink included.
fn is_there(path: &Path) -> io::Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err),
    }
}

/// Whether `dir` is a Velopack install: `Update.exe`, or `current` with `packages`.
fn is_velopack_install(dir: &Path) -> io::Result<bool> {
    Ok(is_there(&dir.join("Update.exe"))?
        || (is_there(&dir.join("current"))? && is_there(&dir.join("packages"))?))
}

/// Rename `from` to `to`, failing with [`io::ErrorKind::AlreadyExists`] rather than
/// replace what is at `to`. On Windows this is `MoveFileExW` without
/// `MOVEFILE_REPLACE_EXISTING` (nor `MOVEFILE_COPY_ALLOWED`: across volumes it fails).
#[cfg_attr(not(windows), allow(dead_code))]
fn rename_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;

        use windows::Win32::Storage::FileSystem::{MOVE_FILE_FLAGS, MoveFileExW};
        use windows::core::PCWSTR;

        let wide = |path: &Path| {
            path.as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<u16>>()
        };
        let (from, to) = (wide(from), wide(to));
        // SAFETY: both are NUL-terminated UTF-16 strings that outlive the call.
        unsafe {
            MoveFileExW(
                PCWSTR(from.as_ptr()),
                PCWSTR(to.as_ptr()),
                MOVE_FILE_FLAGS(0),
            )
        }
        .map_err(|err| {
            // An HRESULT from a Win32 error code (facility 7) carries the code in its
            // low word, which std maps to its kinds.
            let code = err.code().0 as u32;
            if code & 0xFFFF_0000 == 0x8007_0000 {
                io::Error::from_raw_os_error((code & 0xFFFF) as i32)
            } else {
                io::Error::other(err)
            }
        })
    }
    #[cfg(not(windows))]
    {
        // Checked just before: the rename of a directory onto an empty one would replace
        // it, and of a file onto a file always does.
        if is_there(to)? {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{} is there already", to.display()),
            ));
        }
        std::fs::rename(from, to)
    }
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

    /// The migration between `roaming` and `local`, through the real rename.
    fn migrate(roaming: &Path, local: &Path) -> io::Result<Migrated> {
        migrate_from_roaming_with(roaming, local, "app", &rename_no_replace)
    }

    /// The migration with Roaming on another volume: renaming anything out of `roaming`
    /// fails as across volumes; a rename within Local is a real one.
    fn migrate_across_volumes(roaming: &Path, local: &Path) -> io::Result<Migrated> {
        let rename = |from: &Path, to: &Path| {
            if from.starts_with(roaming) {
                Err(io::Error::from(io::ErrorKind::CrossesDevices))
            } else {
                rename_no_replace(from, to)
            }
        };
        migrate_from_roaming_with(roaming, local, "app", &rename)
    }

    fn read(path: impl AsRef<Path>) -> String {
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn a_failure_mid_merge_leaves_what_moved_in_local_and_the_rest_in_roaming() {
        let (_temp, roaming, local) = roots();
        std::fs::create_dir_all(roaming.join("app")).unwrap();
        for name in ["a", "b", "c"] {
            std::fs::write(roaming.join("app").join(name), name).unwrap();
        }
        std::fs::create_dir_all(local.join("app/cache")).unwrap();
        // `b`'s destination is blocked: its rename fails, after `a`'s and before `c`'s.
        let rename = |from: &Path, to: &Path| {
            if from.ends_with("b") {
                Err(io::Error::from(io::ErrorKind::PermissionDenied))
            } else {
                rename_no_replace(from, to)
            }
        };

        let err = migrate_from_roaming_with(&roaming, &local, "app", &rename).unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(read(local.join("app/a")), "a");
        assert!(!roaming.join("app/a").exists());
        assert_eq!(read(roaming.join("app/b")), "b");
        assert_eq!(read(roaming.join("app/c")), "c");
        assert!(!local.join("app/b").exists());
        assert!(!local.join("app/c").exists());
    }

    #[test]
    fn a_failed_merge_moves_the_rest_at_the_next_start() {
        let (_temp, roaming, local) = roots();
        std::fs::create_dir_all(roaming.join("app")).unwrap();
        std::fs::write(roaming.join("app/a"), "a").unwrap();
        std::fs::create_dir_all(local.join("app/cache")).unwrap();
        let failing = |_: &Path, _: &Path| Err(io::Error::from(io::ErrorKind::PermissionDenied));
        assert!(migrate_from_roaming_with(&roaming, &local, "app", &failing).is_err());

        let migrated = migrate(&roaming, &local).unwrap();

        assert_eq!(migrated, Migrated::Merged { left: vec![] });
        assert_eq!(read(local.join("app/a")), "a");
        assert!(!roaming.join("app").exists());
    }

    #[test]
    fn across_volumes_the_roaming_directory_is_copied_then_swapped_in() {
        let (_temp, roaming, local) = roots();
        std::fs::create_dir_all(roaming.join("app/data")).unwrap();
        std::fs::write(roaming.join("app/config.toml"), "a").unwrap();
        std::fs::write(roaming.join("app/data/index"), "b").unwrap();
        // What an earlier copy that was cut off left.
        std::fs::create_dir_all(local.join("app.corundum-migrating/stale")).unwrap();

        let migrated = migrate_across_volumes(&roaming, &local).unwrap();

        assert_eq!(migrated, Migrated::Moved);
        assert_eq!(read(local.join("app/config.toml")), "a");
        assert_eq!(read(local.join("app/data/index")), "b");
        assert!(!local.join("app/stale").exists());
        assert!(!local.join("app.corundum-migrating").exists());
        assert!(!roaming.join("app").exists());
        // Done, the lock is not left behind.
        assert!(!local.join("app.migrate.lock").exists());
    }

    #[test]
    fn across_volumes_each_entry_is_copied_then_swapped_into_local() {
        let (_temp, roaming, local) = roots();
        std::fs::create_dir_all(roaming.join("app/data")).unwrap();
        std::fs::write(roaming.join("app/config.toml"), "a").unwrap();
        std::fs::write(roaming.join("app/data/index"), "b").unwrap();
        std::fs::create_dir_all(local.join("app/cache")).unwrap();

        let migrated = migrate_across_volumes(&roaming, &local).unwrap();

        assert_eq!(migrated, Migrated::Merged { left: vec![] });
        assert_eq!(read(local.join("app/config.toml")), "a");
        assert_eq!(read(local.join("app/data/index")), "b");
        let names: Vec<_> = std::fs::read_dir(local.join("app"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert!(
            names
                .iter()
                .all(|name| !name.to_string_lossy().contains("corundum-migrating")),
            "{names:?}"
        );
        assert!(!roaming.join("app").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_copy_that_fails_leaves_roaming_whole_and_no_copy_behind() {
        use std::os::unix::fs::PermissionsExt;

        let (_temp, roaming, local) = roots();
        std::fs::create_dir_all(roaming.join("app")).unwrap();
        std::fs::write(roaming.join("app/config.toml"), "a").unwrap();
        std::fs::write(roaming.join("app/unreadable"), "b").unwrap();
        let unreadable = roaming.join("app/unreadable");
        std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000)).unwrap();
        if std::fs::read(&unreadable).is_ok() {
            // Root reads it anyway: there is no failure to try.
            return;
        }

        assert!(migrate_across_volumes(&roaming, &local).is_err());

        assert_eq!(read(roaming.join("app/config.toml")), "a");
        assert!(unreadable.exists());
        assert!(!local.join("app").exists());
        assert!(!local.join("app.corundum-migrating").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_is_copied_as_a_symlink() {
        let (_temp, roaming, local) = roots();
        std::fs::create_dir_all(roaming.join("app")).unwrap();
        std::os::unix::fs::symlink("elsewhere", roaming.join("app/link")).unwrap();

        migrate_across_volumes(&roaming, &local).unwrap();

        assert_eq!(
            std::fs::read_link(local.join("app/link")).unwrap(),
            Path::new("elsewhere")
        );
    }

    #[test]
    fn a_start_finding_another_moving_touches_nothing() {
        let (_temp, roaming, local) = roots();
        std::fs::create_dir_all(roaming.join("app")).unwrap();
        std::fs::write(roaming.join("app/config.toml"), "a").unwrap();
        std::fs::create_dir_all(&local).unwrap();
        let other = std::fs::File::create(local.join("app.migrate.lock")).unwrap();
        other.lock().unwrap();

        let err = migrate_across_volumes(&roaming, &local).unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::WouldBlock);
        assert_eq!(read(roaming.join("app/config.toml")), "a");
        assert!(!local.join("app").exists());
    }

    #[test]
    fn an_entry_with_the_reserved_prefix_is_left_in_roaming() {
        let (_temp, roaming, local) = roots();
        std::fs::create_dir_all(roaming.join("app")).unwrap();
        std::fs::write(roaming.join("app/.corundum-migrating-x"), "x").unwrap();
        std::fs::write(roaming.join("app/config.toml"), "a").unwrap();
        std::fs::create_dir_all(local.join("app/cache")).unwrap();

        let migrated = migrate(&roaming, &local).unwrap();

        assert_eq!(
            migrated,
            Migrated::Merged {
                left: vec![roaming.join("app/.corundum-migrating-x")]
            }
        );
        assert_eq!(read(roaming.join("app/.corundum-migrating-x")), "x");
        assert_eq!(read(local.join("app/config.toml")), "a");
    }

    #[test]
    fn a_merge_clears_what_a_crashed_copy_left() {
        let (_temp, roaming, local) = roots();
        std::fs::create_dir_all(roaming.join("app")).unwrap();
        std::fs::write(roaming.join("app/config.toml"), "a").unwrap();
        std::fs::create_dir_all(local.join("app/.corundum-migrating-data/half")).unwrap();
        std::fs::create_dir_all(local.join("app.corundum-migrating/half")).unwrap();

        migrate_across_volumes(&roaming, &local).unwrap();

        assert!(!local.join("app/.corundum-migrating-data").exists());
        assert!(!local.join("app.corundum-migrating").exists());
        assert_eq!(read(local.join("app/config.toml")), "a");
    }

    #[cfg(unix)]
    #[test]
    fn a_link_that_cannot_be_made_leaves_its_entry_in_roaming() {
        let (_temp, roaming, local) = roots();
        std::fs::create_dir_all(roaming.join("app")).unwrap();
        std::os::unix::fs::symlink("elsewhere", roaming.join("app/link")).unwrap();
        std::fs::write(roaming.join("app/config.toml"), "a").unwrap();
        std::fs::create_dir_all(local.join("app/cache")).unwrap();
        FAIL_LINKS.set(true);

        let migrated = migrate_across_volumes(&roaming, &local).unwrap();

        assert_eq!(
            migrated,
            Migrated::Merged {
                left: vec![roaming.join("app/link")]
            }
        );
        assert!(roaming.join("app/link").is_symlink());
        assert!(!local.join("app/link").is_symlink());
        assert_eq!(read(local.join("app/config.toml")), "a");
    }

    #[cfg(unix)]
    #[test]
    fn a_link_deep_in_a_whole_move_leaves_only_its_entry() {
        let (_temp, roaming, local) = roots();
        std::fs::create_dir_all(roaming.join("app/linked")).unwrap();
        std::os::unix::fs::symlink("elsewhere", roaming.join("app/linked/link")).unwrap();
        std::fs::write(roaming.join("app/config.toml"), "a").unwrap();
        FAIL_LINKS.set(true);

        let migrated = migrate_across_volumes(&roaming, &local).unwrap();

        assert_eq!(
            migrated,
            Migrated::Merged {
                left: vec![roaming.join("app/linked")]
            }
        );
        assert!(roaming.join("app/linked/link").is_symlink());
        assert!(!local.join("app/linked").exists());
        assert_eq!(read(local.join("app/config.toml")), "a");
        assert!(!local.join("app.corundum-migrating").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_source_that_cannot_be_removed_is_copied_and_left() {
        use std::os::unix::fs::PermissionsExt;

        let (_temp, roaming, local) = roots();
        std::fs::create_dir_all(roaming.join("app/data")).unwrap();
        std::fs::write(roaming.join("app/data/index"), "b").unwrap();
        std::fs::create_dir_all(local.join("app/cache")).unwrap();
        // Nothing can be taken out of `data`, as a file open in an old process holds it on
        // Windows.
        let data = roaming.join("app/data");
        std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o555)).unwrap();
        if std::fs::File::create(data.join("probe")).is_ok() {
            // Root writes there anyway: there is no failure to try.
            std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o755)).unwrap();
            return;
        }

        let migrated = migrate_across_volumes(&roaming, &local);
        std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert_eq!(
            migrated.unwrap(),
            Migrated::Merged {
                left: vec![roaming.join("app/data")]
            }
        );
        assert_eq!(read(local.join("app/data/index")), "b");
    }

    #[test]
    fn a_velopack_install_is_never_merged_into() {
        for install in [&["Update.exe"][..], &["current/", "packages/"][..]] {
            let (_temp, roaming, local) = roots();
            std::fs::create_dir_all(roaming.join("app")).unwrap();
            std::fs::write(roaming.join("app/config.toml"), "a").unwrap();
            std::fs::create_dir_all(local.join("app")).unwrap();
            for entry in install {
                match entry.strip_suffix('/') {
                    Some(dir) => std::fs::create_dir(local.join("app").join(dir)).unwrap(),
                    None => std::fs::write(local.join("app").join(entry), "").unwrap(),
                }
            }

            let err = migrate(&roaming, &local).unwrap_err();

            assert!(err.to_string().contains("Velopack"), "{err}");
            assert_eq!(read(roaming.join("app/config.toml")), "a");
            assert!(!local.join("app/config.toml").exists());
        }
    }

    #[test]
    fn a_rename_never_replaces_what_is_there() {
        let temp = tempfile::tempdir().unwrap();
        let (from, to) = (temp.path().join("from"), temp.path().join("to"));
        std::fs::create_dir(&from).unwrap();
        std::fs::create_dir(&to).unwrap();

        let err = rename_no_replace(&from, &to).unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert!(from.is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn a_dangling_symlink_in_local_counts_as_there() {
        let (_temp, roaming, local) = roots();
        std::fs::create_dir_all(roaming.join("app")).unwrap();
        std::fs::write(roaming.join("app/config.toml"), "a").unwrap();
        std::fs::create_dir_all(local.join("app")).unwrap();
        std::os::unix::fs::symlink("nowhere", local.join("app/config.toml")).unwrap();

        let migrated = migrate(&roaming, &local).unwrap();

        assert_eq!(
            migrated,
            Migrated::Merged {
                left: vec![roaming.join("app/config.toml")]
            }
        );
        assert_eq!(read(roaming.join("app/config.toml")), "a");
    }

    #[test]
    fn the_roaming_directory_moves_to_local_with_its_files() {
        let (_temp, roaming, local) = roots();
        std::fs::create_dir_all(roaming.join("app/sub")).unwrap();
        std::fs::write(roaming.join("app/settings.toml"), "a").unwrap();
        std::fs::write(roaming.join("app/sub/data"), "b").unwrap();

        let migrated = migrate(&roaming, &local).unwrap();

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

        let migrated = migrate(&roaming, &local).unwrap();

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

        let migrated = migrate(&roaming, &local).unwrap();

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

        let migrated = migrate(&roaming, &local).unwrap();

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

        assert!(migrate(&roaming, &local).is_err());
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
