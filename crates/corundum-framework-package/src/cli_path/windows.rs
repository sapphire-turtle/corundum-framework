//! Windows: the package's `current` directory on the user `Path`, kept in
//! `HKCU\Environment` with the type it has (`REG_EXPAND_SZ` when new), then announced to
//! the running programs so that a new terminal sees it.

use corundum_framework_platform::package::{self, PackageKind};
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, LPARAM, WPARAM};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_EXPAND_SZ, REG_VALUE_TYPE,
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
};
use windows::core::w;

use super::{CliSpec, CliStatus, path_with, path_without};
use crate::error::Error;

/// The package's `current` directory, as the `Path` names it; `None` outside a package.
fn dir() -> Option<String> {
    let layout = package::current()?;
    if layout.kind != PackageKind::Windows {
        return None;
    }
    layout.bin_dir.to_str().map(str::to_owned)
}

pub(super) fn status(_spec: &CliSpec) -> CliStatus {
    let Some(dir) = dir() else {
        return CliStatus::Unsupported;
    };
    match Environment::open().and_then(|env| env.path()) {
        Ok(Some((value, _))) if path_with(&value, &dir).is_none() => CliStatus::Installed,
        Ok(_) => CliStatus::Missing,
        Err(e) => {
            tracing::warn!("could not read the user Path: {e}");
            CliStatus::Missing
        }
    }
}

pub(super) fn install(_spec: &CliSpec) -> Result<(), Error> {
    let dir = dir().ok_or(Error::NotPackaged)?;
    let env = Environment::open()?;
    let (value, ty) = env.path()?.unwrap_or((String::new(), REG_EXPAND_SZ));
    if let Some(value) = path_with(&value, &dir) {
        env.set_path(&value, ty)?;
        announce();
    }
    Ok(())
}

pub(super) fn uninstall(_spec: &CliSpec) -> Result<(), Error> {
    let dir = dir().ok_or(Error::NotPackaged)?;
    let env = Environment::open()?;
    let Some((value, ty)) = env.path()? else {
        return Ok(());
    };
    if let Some(value) = path_without(&value, &dir) {
        env.set_path(&value, ty)?;
        announce();
    }
    Ok(())
}

/// `HKCU\Environment`, open to read and write; closed when dropped.
struct Environment(HKEY);

impl Environment {
    fn open() -> Result<Self, Error> {
        let mut key = HKEY::default();
        // SAFETY: the subkey is a NUL-terminated literal and `key` outlives the call.
        let err = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                w!("Environment"),
                None,
                KEY_QUERY_VALUE | KEY_SET_VALUE,
                &mut key,
            )
        };
        if err != ERROR_SUCCESS {
            return Err(registry_error("open HKCU\\Environment", err.0));
        }
        Ok(Self(key))
    }

    /// The `Path` value and its type, or `None` when the user has none.
    fn path(&self) -> Result<Option<(String, REG_VALUE_TYPE)>, Error> {
        let mut ty = REG_VALUE_TYPE::default();
        let mut size = 0u32;
        // SAFETY: a size query; the pointers are to locals that outlive the call.
        let err = unsafe {
            RegQueryValueExW(
                self.0,
                w!("Path"),
                None,
                Some(&mut ty),
                None,
                Some(&mut size),
            )
        };
        if err == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if err != ERROR_SUCCESS {
            return Err(registry_error("read the user Path", err.0));
        }
        // u16s, so the bytes are aligned for reading back as UTF-16.
        let mut data = vec![0u16; (size as usize).div_ceil(2)];
        // SAFETY: `data` holds at least `size` bytes, as `size` says.
        let err = unsafe {
            RegQueryValueExW(
                self.0,
                w!("Path"),
                None,
                Some(&mut ty),
                Some(data.as_mut_ptr().cast()),
                Some(&mut size),
            )
        };
        if err != ERROR_SUCCESS {
            return Err(registry_error("read the user Path", err.0));
        }
        data.truncate((size as usize) / 2);
        while data.last() == Some(&0) {
            data.pop();
        }
        // Not UTF-16 cannot be written back unchanged, so it is not written at all.
        let value = String::from_utf16(&data)
            .map_err(|_| Error::CliPath("the user Path is not valid UTF-16".to_owned()))?;
        Ok(Some((value, ty)))
    }

    fn set_path(&self, value: &str, ty: REG_VALUE_TYPE) -> Result<(), Error> {
        let bytes: Vec<u8> = value
            .encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect();
        // SAFETY: `bytes` is a NUL-terminated UTF-16 string that outlives the call.
        let err = unsafe { RegSetValueExW(self.0, w!("Path"), None, ty, Some(&bytes)) };
        if err != ERROR_SUCCESS {
            return Err(registry_error("write the user Path", err.0));
        }
        Ok(())
    }
}

impl Drop for Environment {
    fn drop(&mut self) {
        // SAFETY: the key was opened by `open` and is closed once.
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

/// Tells the running programs (Explorer, so the terminals it starts) that the environment
/// changed. A program that does not answer in 5 seconds is skipped, not waited for: the
/// install hook has 30.
fn announce() {
    // SAFETY: the string is a NUL-terminated literal, read during the call only.
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            LPARAM(w!("Environment").as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            5000,
            None,
        );
    }
}

fn registry_error(what: &str, code: u32) -> Error {
    Error::CliPath(format!(
        "could not {what}: {}",
        std::io::Error::from_raw_os_error(code as i32)
    ))
}
