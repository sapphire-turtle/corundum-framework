//! The Windows entry: a Start Menu shortcut whose `System.AppUserModel.ID` lets the
//! taskbar group the app's window with it.

use super::{LauncherSpec, Root};
#[cfg(windows)]
use super::{Registered, Status};
#[cfg(windows)]
use std::io;
use std::path::{Path, PathBuf};

/// What the app's shortcut holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LinkFields {
    /// The executable the shortcut starts.
    pub target: PathBuf,
    /// The folder the executable is started in.
    pub working_dir: PathBuf,
    /// The file the icon is read from, and the icon's index in it.
    pub icon: (PathBuf, i32),
    /// The `System.AppUserModel.ID` the taskbar groups the app's windows by.
    pub app_id: String,
    /// The tooltip the Start Menu shows.
    pub description: String,
}

/// The `System.AppUserModel.ID` of the app `spec` describes, which the shortcut holds and the
/// process sets for itself.
pub(crate) fn app_id(spec: &LauncherSpec) -> String {
    format!("{}.{}", spec.bundle_prefix, spec.id)
}

/// The shortcut that starts `exe` as the app `spec` describes: in the executable's folder,
/// with the executable's own icon.
pub(crate) fn link_fields(spec: &LauncherSpec, exe: &Path) -> LinkFields {
    LinkFields {
        target: exe.to_path_buf(),
        working_dir: exe.parent().unwrap_or(Path::new("")).to_path_buf(),
        icon: (exe.to_path_buf(), 0),
        app_id: app_id(spec),
        description: spec.description.to_owned(),
    }
}

/// The path of the app's shortcut in the user's Start Menu, or `None` where `%APPDATA%`
/// is not known.
pub(crate) fn link_path(spec: &LauncherSpec, root: &Root) -> Option<PathBuf> {
    let appdata = root.appdata.as_ref()?;
    Some(
        appdata
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join(format!("{}.lnk", spec.name)),
    )
}

/// Sets the process's explicit AppUserModelID to the shortcut's, so the taskbar groups the
/// window with it however the process was started.
#[cfg(windows)]
pub(crate) fn set_process_app_id(spec: &LauncherSpec) -> io::Result<()> {
    use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
    use windows::core::HSTRING;
    // SAFETY: the string is live for the call, which copies it.
    unsafe { SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(app_id(spec))) }
        .map_err(io::Error::from)
}

/// Writes the app's shortcut unless one already starts `exe` with the app's id; `Skipped`
/// where `%APPDATA%` is not known.
#[cfg(windows)]
pub(crate) fn register(spec: &LauncherSpec, root: &Root, exe: &Path) -> io::Result<Registered> {
    let Some(path) = link_path(spec, root) else {
        return Ok(Registered::Skipped);
    };
    let fields = link_fields(spec, exe);
    let _com = com::Com::init()?;
    if let Ok((target, app_id)) = com::read(&path)
        && target == fields.target
        && app_id == fields.app_id
    {
        return Ok(Registered::Unchanged);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    com::write(&path, &fields)?;
    Ok(Registered::Written)
}

/// Removes the app's shortcut; one that is not there is no error.
#[cfg(windows)]
pub(crate) fn unregister(spec: &LauncherSpec, root: &Root) -> io::Result<()> {
    let Some(path) = link_path(spec, root) else {
        return Ok(());
    };
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Whether a shortcut whose target is `target` was left by an install before the package
/// rooted at `package_root`, and so is to go: its target is outside the package. Velopack's
/// own shortcut sits at the same path when the package's title is the app's name, and
/// starts the executable inside the package. A target that could not be read (`None`, or
/// empty) is kept, as it might be the installer's. Paths are compared as Windows compares
/// them: in any case, with `/` or `\`, a verbatim `\\?\` prefix and a trailing separator
/// aside.
pub(crate) fn is_left_behind(target: Option<&Path>, package_root: &Path) -> bool {
    let Some(target) = target.filter(|t| !t.as_os_str().is_empty()) else {
        return false;
    };
    let (target, root) = (comparable(target), comparable(package_root));
    !(target == root || target.starts_with(&format!("{root}\\")))
}

/// `path` as [`is_left_behind`] compares it: lower case, `\` as the only separator, without
/// a verbatim prefix or a trailing separator.
fn comparable(path: &Path) -> String {
    let text = path.to_string_lossy().replace('/', "\\").to_lowercase();
    let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
    text.trim_end_matches('\\').to_owned()
}

/// Removes the app's shortcut when an install before the package rooted at `package_root`
/// left it ([`is_left_behind`]); the package's own shortcut, and one that cannot be read,
/// is kept.
#[cfg(windows)]
pub(crate) fn remove_left_behind(
    spec: &LauncherSpec,
    root: &Root,
    package_root: &Path,
) -> io::Result<()> {
    let Some(path) = link_path(spec, root) else {
        return Ok(());
    };
    if !path.exists() {
        return Ok(());
    }
    let target = match com::Com::init() {
        Ok(_com) => com::read(&path).ok().map(|(target, _)| target),
        Err(_) => None,
    };
    // Resolved only when both are there, so that both are spelled alike.
    let (target, package_root) = match (
        target.as_deref().map(Path::canonicalize),
        package_root.canonicalize(),
    ) {
        (Some(Ok(target)), Ok(package_root)) => (Some(target), package_root),
        _ => (target, package_root.to_path_buf()),
    };
    if !is_left_behind(target.as_deref(), &package_root) {
        return Ok(());
    }
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Whether the app's shortcut is there and starts `exe`; `Unsupported` where `%APPDATA%`
/// is not known.
#[cfg(windows)]
pub(crate) fn status(spec: &LauncherSpec, root: &Root, exe: &Path) -> Status {
    let Some(path) = link_path(spec, root) else {
        return Status::Unsupported;
    };
    if !path.exists() {
        return Status::Absent;
    }
    let Ok(_com) = com::Com::init() else {
        return Status::Stale;
    };
    match com::read(&path) {
        Ok((target, _)) if target == exe => Status::Registered,
        _ => Status::Stale,
    }
}

/// The shortcut read and written through the shell's COM objects.
#[cfg(windows)]
mod com {
    use super::LinkFields;
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::path::{Path, PathBuf};
    use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
    use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
    use windows::Win32::System::Com::StructuredStorage::{
        PROPVAR_CHANGE_FLAGS, PROPVARIANT, PropVariantChangeType,
    };
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
        CoUninitialize, IPersistFile, STGM_READ,
    };
    use windows::Win32::System::Variant::VT_LPWSTR;
    use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
    use windows::Win32::UI::Shell::{IShellLinkW, SLGP_RAWPATH, ShellLink};
    use windows::core::{BSTR, HSTRING, Interface, Result};

    /// COM initialised on this thread for as long as it lives; it is uninitialised on drop
    /// only if this value's own initialisation succeeded.
    pub(super) struct Com {
        /// Whether `CoInitializeEx` succeeded and so is to be balanced.
        owned: bool,
    }

    impl Com {
        /// Initialises COM as a single-threaded apartment. A thread already in another
        /// apartment can still use the shell objects, so that is no error.
        pub(super) fn init() -> Result<Com> {
            // SAFETY: no reserved pointer is passed, and every success is balanced in `drop`.
            let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            if hr == RPC_E_CHANGED_MODE {
                return Ok(Com { owned: false });
            }
            // S_OK and S_FALSE both succeed and both count towards the uninitialisations.
            hr.ok()?;
            Ok(Com { owned: true })
        }
    }

    impl Drop for Com {
        fn drop(&mut self) {
            if self.owned {
                // SAFETY: balances this value's successful `CoInitializeEx` on this thread.
                unsafe { CoUninitialize() };
            }
        }
    }

    /// A new, empty shell link object.
    fn shell_link() -> Result<IShellLinkW> {
        // SAFETY: COM is initialised on this thread by the caller's `Com`.
        unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }
    }

    /// The target and the app id of the shortcut at `path`.
    pub(super) fn read(path: &Path) -> Result<(PathBuf, String)> {
        let (target, value) = read_raw(path)?;
        let app_id = BSTR::try_from(&value)
            .map(|s| s.to_string())
            .unwrap_or_default();
        Ok((target, app_id))
    }

    /// The target of the shortcut at `path` and its app id property as stored.
    pub(super) fn read_raw(path: &Path) -> Result<(PathBuf, PROPVARIANT)> {
        let link = shell_link()?;
        // SAFETY: the calls are on live interfaces of one object, with buffers that outlive
        // them; a null find-data pointer is allowed.
        unsafe {
            link.cast::<IPersistFile>()?
                .Load(&HSTRING::from(path), STGM_READ)?;
            let mut buf = vec![0u16; 32768];
            link.GetPath(&mut buf, std::ptr::null_mut(), SLGP_RAWPATH.0 as u32)?;
            let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            let target = PathBuf::from(OsString::from_wide(&buf[..len]));
            let value = link
                .cast::<IPropertyStore>()?
                .GetValue(&PKEY_AppUserModel_ID)?;
            Ok((target, value))
        }
    }

    /// `app_id` as the `VT_LPWSTR` value `System.AppUserModel.ID` is declared to hold;
    /// `PROPVARIANT::from(&str)` alone would make a `VT_BSTR`.
    fn app_id_value(app_id: &str) -> Result<PROPVARIANT> {
        let mut value = PROPVARIANT::default();
        // SAFETY: both values are live and initialised; the conversion allocates the
        // destination's string, which `PROPVARIANT`'s drop frees.
        unsafe {
            PropVariantChangeType(
                &mut value,
                &PROPVARIANT::from(app_id),
                PROPVAR_CHANGE_FLAGS(0),
                VT_LPWSTR,
            )?;
        }
        Ok(value)
    }

    /// Writes a shortcut holding `fields` at `path`, replacing any there.
    pub(super) fn write(path: &Path, fields: &LinkFields) -> Result<()> {
        let link = shell_link()?;
        // SAFETY: the calls are on live interfaces of one object, with arguments that
        // outlive them.
        unsafe {
            link.SetPath(&HSTRING::from(fields.target.as_path()))?;
            link.SetWorkingDirectory(&HSTRING::from(fields.working_dir.as_path()))?;
            link.SetIconLocation(&HSTRING::from(fields.icon.0.as_path()), fields.icon.1)?;
            link.SetDescription(&HSTRING::from(fields.description.as_str()))?;
            let store = link.cast::<IPropertyStore>()?;
            store.SetValue(&PKEY_AppUserModel_ID, &app_id_value(&fields.app_id)?)?;
            store.Commit()?;
            link.cast::<IPersistFile>()?
                .Save(&HSTRING::from(path), true)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launcher::{LauncherSpec, Root};
    use std::path::{Path, PathBuf};

    const SPEC: LauncherSpec = LauncherSpec {
        id: "corundum-demo",
        name: "Corundum Demo",
        description: "Every piece corundum shares",
        category: "Development",
        icon_svg: b"<svg/>",
        bundle_prefix: "net.fireturtle.corundum",
    };

    #[test]
    fn the_link_starts_the_exe_in_its_folder_with_its_icon() {
        let exe = Path::new(r"C:\Users\u\.local\bin\corundum-demo.exe");
        let fields = link_fields(&SPEC, exe);
        assert_eq!(fields.target, exe);
        // On other OSes a backslash is no separator, so the folder is only checked on Windows.
        assert_eq!(fields.working_dir, exe.parent().unwrap());
        #[cfg(windows)]
        assert_eq!(fields.working_dir, Path::new(r"C:\Users\u\.local\bin"));
        assert_eq!(fields.icon, (exe.to_path_buf(), 0));
        assert_eq!(fields.app_id, "net.fireturtle.corundum.corundum-demo");
        assert_eq!(fields.description, "Every piece corundum shares");
    }

    #[test]
    fn the_link_is_in_the_start_menu_programs_folder() {
        let appdata = PathBuf::from("appdata");
        let root = Root {
            home: "home".into(),
            xdg_data_home: None,
            appdata: Some(appdata.clone()),
        };
        let want: PathBuf = [
            appdata.as_path(),
            Path::new("Microsoft"),
            Path::new("Windows"),
            Path::new("Start Menu"),
            Path::new("Programs"),
            Path::new("Corundum Demo.lnk"),
        ]
        .iter()
        .collect();
        assert_eq!(link_path(&SPEC, &root), Some(want));
    }

    #[test]
    fn a_shortcut_into_the_package_is_the_installers() {
        let root = Path::new(r"C:\Users\u\AppData\Local\CorundumDemo");
        // Velopack's own shortcut, its path spelled in another case or with `/`.
        assert!(!is_left_behind(
            Some(Path::new(
                r"c:\users\U\appdata\local\corundumdemo\current\corundum-demo.exe"
            )),
            root
        ));
        assert!(!is_left_behind(
            Some(Path::new(
                "C:/Users/u/AppData/Local/CorundumDemo/current/corundum-demo.exe"
            )),
            Path::new(r"C:\Users\u\AppData\Local\CorundumDemo\"),
        ));
        // A verbatim path, as canonicalizing gives on Windows.
        assert!(!is_left_behind(
            Some(Path::new(
                r"\\?\C:\Users\u\AppData\Local\CorundumDemo\current\corundum-demo.exe"
            )),
            root
        ));
    }

    #[test]
    fn a_shortcut_to_an_earlier_install_is_left_behind() {
        let root = Path::new(r"C:\Users\u\AppData\Local\CorundumDemo");
        assert!(is_left_behind(
            Some(Path::new(r"C:\Users\u\.cargo\bin\corundum-demo.exe")),
            root
        ));
        // A sibling whose name merely begins with the package's.
        assert!(is_left_behind(
            Some(Path::new(
                r"C:\Users\u\AppData\Local\CorundumDemo2\corundum-demo.exe"
            )),
            root
        ));
    }

    #[test]
    fn a_shortcut_that_cannot_be_read_is_kept() {
        let root = Path::new(r"C:\Users\u\AppData\Local\CorundumDemo");
        assert!(!is_left_behind(None, root));
        assert!(!is_left_behind(Some(Path::new("")), root));
    }

    #[test]
    fn there_is_no_link_without_appdata() {
        let root = Root {
            home: "home".into(),
            xdg_data_home: None,
            appdata: None,
        };
        assert_eq!(link_path(&SPEC, &root), None);
    }

    #[cfg(windows)]
    #[test]
    fn register_writes_once_then_unregister_removes_it() {
        use crate::launcher::{Registered, Status, register_at, status_at, unregister_at};
        let dir = tempfile::tempdir().unwrap();
        let root = Root {
            home: dir.path().into(),
            xdg_data_home: None,
            appdata: Some(dir.path().into()),
        };
        let exe = Path::new(r"C:\Users\u\.local\bin\corundum-demo.exe");
        let link = link_path(&SPEC, &root).unwrap();
        assert_eq!(status_at(&SPEC, &root, exe), Status::Absent);
        assert_eq!(register_at(&SPEC, &root, exe).unwrap(), Registered::Written);
        assert!(link.is_file());
        let _com = super::com::Com::init().unwrap();
        let (_, value) = super::com::read_raw(&link).unwrap();
        assert_eq!(value.vt(), windows::Win32::System::Variant::VT_LPWSTR);
        assert_eq!(
            windows::core::BSTR::try_from(&value).unwrap().to_string(),
            link_fields(&SPEC, exe).app_id
        );
        drop(value);
        drop(_com);
        assert_eq!(
            register_at(&SPEC, &root, exe).unwrap(),
            Registered::Unchanged
        );
        assert_eq!(status_at(&SPEC, &root, exe), Status::Registered);
        let moved = Path::new(r"C:\Users\u\bin\corundum-demo.exe");
        assert_eq!(status_at(&SPEC, &root, moved), Status::Stale);
        unregister_at(&SPEC, &root).unwrap();
        assert!(!link.exists());
        assert_eq!(status_at(&SPEC, &root, exe), Status::Absent);
        unregister_at(&SPEC, &root).unwrap();
    }
}
