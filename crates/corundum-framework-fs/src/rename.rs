//! A rename that never replaces what is at its destination.

use std::io;

use cap_std::fs::Dir;

/// Renames `from` in `from_dir` to `to` in `to_dir`, failing with
/// [`io::ErrorKind::AlreadyExists`] if anything is at `to`. Both names are single segments.
///
/// The check is atomic where the system has one: `renameat2` with `RENAME_NOREPLACE` on
/// Linux, `renameatx_np` with `RENAME_EXCL` on Apple systems, and on Windows a rename by
/// handle that does not replace (see [`rename_by_handle`]). Only on unix systems without
/// it (another unix, or a Linux file system that refuses the flag with `EINVAL`) does it
/// fall back to looking first and renaming then, which has a window: an entry made at `to`
/// between the look and the rename is replaced (a file, or an empty folder by a folder).
pub(crate) fn rename_noreplace(
    from_dir: &Dir,
    from: &str,
    to_dir: &Dir,
    to: &str,
) -> io::Result<()> {
    #[cfg(windows)]
    {
        rename_by_handle(from_dir, from, to_dir, to)
    }
    #[cfg(unix)]
    {
        #[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
        {
            use rustix::fs::{RenameFlags, renameat_with};
            match renameat_with(from_dir, from, to_dir, to, RenameFlags::NOREPLACE) {
                Ok(()) => Ok(()),
                // A Linux file system that does not take the flag.
                #[cfg(any(target_os = "linux", target_os = "android"))]
                Err(rustix::io::Errno::INVAL) => rename_checked(from_dir, from, to_dir, to),
                Err(e) => Err(e.into()),
            }
        }
        #[cfg(not(any(target_os = "linux", target_os = "android", target_vendor = "apple")))]
        {
            rename_checked(from_dir, from, to_dir, to)
        }
    }
}

/// Looks at `to`, and renames only if nothing is there; see [`rename_noreplace`]'s window.
#[cfg(unix)]
// Apple systems always have `RENAME_EXCL`: there only the tests call it.
#[cfg_attr(target_vendor = "apple", allow(dead_code))]
fn rename_checked(from_dir: &Dir, from: &str, to_dir: &Dir, to: &str) -> io::Result<()> {
    match to_dir.symlink_metadata(to) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => from_dir.rename(from, to_dir, to),
        Err(e) => Err(e),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{to:?} exists"),
        )),
    }
}

/// The Windows rename: opens `from` itself, relative to `from_dir` and without following it
/// (a symlink or junction is renamed as itself), and renames that handle to `to` relative to
/// `to_dir`'s handle with `ReplaceIfExists` false, so the file system refuses an existing
/// `to` atomically. The handle is closed before this returns.
#[cfg(windows)]
fn rename_by_handle(from_dir: &Dir, from: &str, to_dir: &Dir, to: &str) -> io::Result<()> {
    use std::os::windows::io::AsRawHandle;

    use cap_fs_ext::{FollowSymlinks, OpenOptionsFollowExt};
    use cap_std::fs::{OpenOptions, OpenOptionsExt};
    use windows_sys::Win32::Foundation::{
        ERROR_ALREADY_EXISTS, ERROR_FILE_EXISTS, ERROR_NOT_SAME_DEVICE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        DELETE, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    };

    let mut options = OpenOptions::new();
    options
        .access_mode(DELETE)
        // Folders open only with backup semantics; a reparse point opens as itself.
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .follow(FollowSymlinks::No);
    let source = from_dir.open_with(from, &options)?;
    let renamed = set_rename_info(source.as_raw_handle(), to_dir.as_raw_handle(), to);
    drop(source);
    renamed.map_err(|e| {
        let kind = match e.raw_os_error().map(|c| c as u32) {
            Some(ERROR_ALREADY_EXISTS | ERROR_FILE_EXISTS) => io::ErrorKind::AlreadyExists,
            Some(ERROR_NOT_SAME_DEVICE) => io::ErrorKind::CrossesDevices,
            _ => return e,
        };
        io::Error::new(kind, e)
    })
}

/// `NtSetInformationFile(FileRenameInformation)` on `file`, to `name` in the folder `dir`,
/// never replacing. The system's error comes back as its Win32 code.
///
/// Not kernel32's `SetFileInformationByHandle(FileRenameInfo)`: that one turns the name into
/// a full path from the current folder, which with a `RootDirectory` is refused as an
/// invalid parameter.
#[cfg(windows)]
fn set_rename_info(
    file: std::os::windows::io::RawHandle,
    dir: std::os::windows::io::RawHandle,
    name: &str,
) -> io::Result<()> {
    use std::mem::{offset_of, size_of};

    use windows_sys::Wdk::Storage::FileSystem::{
        FILE_RENAME_INFORMATION, FILE_RENAME_INFORMATION_0, FileRenameInformation,
        NtSetInformationFile,
    };
    use windows_sys::Win32::Foundation::RtlNtStatusToDosError;
    use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;

    let wide: Vec<u16> = name.encode_utf16().collect();
    let name_bytes = wide.len() * size_of::<u16>();
    let offset = offset_of!(FILE_RENAME_INFORMATION, FileName);
    // The name is inline, after the header; `u64`s align the buffer for the header's handle.
    let size = (offset + name_bytes).max(size_of::<FILE_RENAME_INFORMATION>());
    let mut buffer = vec![0u64; size.div_ceil(size_of::<u64>())];
    let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
    let too_long = || io::Error::new(io::ErrorKind::InvalidInput, "the name is too long");
    let name_len = u32::try_from(name_bytes).map_err(|_| too_long())?;
    let buffer_len = u32::try_from(size).map_err(|_| too_long())?;
    let mut status_block = IO_STATUS_BLOCK::default();
    // SAFETY: `buffer` is zeroed, aligned for `FILE_RENAME_INFORMATION` (8 bytes, as `u64`),
    // and at least `size` bytes long: room for the header and for `wide.len()` `u16`s from
    // `FileName` on. The header is written whole (`Flags: 0` sets the whole union, which
    // means "do not replace"), then the name is copied into the bytes after `FileName`'s
    // offset, all inside the buffer. `file` and `dir` are open handles the caller keeps alive
    // for the call, and the system reads `buffer_len` bytes of `buffer` and writes
    // `status_block` only during it.
    let status = unsafe {
        info.write(FILE_RENAME_INFORMATION {
            Anonymous: FILE_RENAME_INFORMATION_0 { Flags: 0 },
            RootDirectory: dir,
            FileNameLength: name_len,
            FileName: [0],
        });
        std::ptr::copy_nonoverlapping(
            wide.as_ptr(),
            (&raw mut (*info).FileName).cast::<u16>(),
            wide.len(),
        );
        NtSetInformationFile(
            file,
            &mut status_block,
            info.cast(),
            buffer_len,
            FileRenameInformation,
        )
    };
    if status >= 0 {
        Ok(())
    } else {
        // SAFETY: a pure conversion of a status code.
        let code = unsafe { RtlNtStatusToDosError(status) };
        Err(io::Error::from_raw_os_error(code as i32))
    }
}

#[cfg(test)]
mod tests {
    use cap_std::ambient_authority;

    use super::*;

    fn dir() -> (tempfile::TempDir, Dir) {
        let t = tempfile::tempdir().unwrap();
        let d = Dir::open_ambient_dir(t.path(), ambient_authority()).unwrap();
        (t, d)
    }

    #[cfg(unix)]
    #[test]
    fn the_fallback_renames_and_never_replaces() {
        let (_t, d) = dir();
        d.write("a", b"a").unwrap();
        d.write("b", b"b").unwrap();
        let e = rename_checked(&d, "a", &d, "b").unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(d.read("b").unwrap(), b"b");
        rename_checked(&d, "a", &d, "c").unwrap();
        assert_eq!(d.read("c").unwrap(), b"a");
        assert!(!d.exists("a"));
    }

    #[test]
    fn the_rename_never_replaces() {
        let (_t, d) = dir();
        d.write("a", b"a").unwrap();
        d.create_dir("f").unwrap();
        let e = rename_noreplace(&d, "a", &d, "f").unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::AlreadyExists);
        rename_noreplace(&d, "a", &d, "c").unwrap();
        assert_eq!(d.read("c").unwrap(), b"a");
    }
}
