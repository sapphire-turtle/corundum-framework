//! The process's resource limits, where an app needs more than the OS starts it with.
//!
//! [`raise_open_files`] lifts the soft limit on open files to the hard one. macOS starts a
//! process from launchd (a login item, a service, an app opened in Finder) with a soft
//! limit of 256, and iOS starts every app with it. A network app spends descriptors on every
//! socket, and a burst of them — lookups to a nameserver that does not answer yet, after
//! the host wakes — reaches 256 well before it reaches anything the hard limit protects.
//! Go does the same for every program since 1.19, and caps the limit on Apple platforms at
//! `kern.maxfilesperproc` as this does.
//!
//! A child process inherits the raised limit. Go puts the original back in the children it
//! starts, for programs that still use `select(2)`, which cannot watch a descriptor above
//! 1023; [`OpenFiles::before`] is that original, for an app that starts such programs.

use std::io;

/// The soft limit on open files before and after [`raise_open_files`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenFiles {
    /// The soft limit the process started with.
    pub before: u64,
    /// The soft limit in force now.
    pub now: u64,
}

/// Raises the soft limit on open files to the hard limit, capped on macOS and iOS at
/// `kern.maxfilesperproc`, and returns it before and after.
///
/// A soft limit already at that target is left as it is. `None` on an OS with no such
/// limit to raise (Windows, where sockets are not counted against the C runtime's file
/// limit) or none this crate reads.
///
/// Call it once, early in `main`: the limit is the process's, so it covers every thread,
/// and raising it before any socket is open means none was refused under the old one.
pub fn raise_open_files() -> io::Result<Option<OpenFiles>> {
    imp::raise()
}

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
mod imp {
    use super::OpenFiles;
    use rustix::process::{Resource, Rlimit, getrlimit, setrlimit};
    use std::io;

    pub(super) fn raise() -> io::Result<Option<OpenFiles>> {
        let limit = getrlimit(Resource::Nofile);
        // `None` is RLIM_INFINITY.
        let before = limit.current.unwrap_or(u64::MAX);
        let mut target = limit.maximum.unwrap_or(u64::MAX);
        if let Some(cap) = per_process_cap() {
            target = target.min(cap);
        }
        if before >= target {
            return Ok(Some(OpenFiles {
                before,
                now: before,
            }));
        }
        setrlimit(
            Resource::Nofile,
            Rlimit {
                // An unlimited hard limit with no cap: unlimited, as RLIM_INFINITY.
                current: (target != u64::MAX).then_some(target),
                maximum: limit.maximum,
            },
        )?;
        Ok(Some(OpenFiles {
            before,
            now: target,
        }))
    }

    /// The most descriptors the kernel lets one process have, on Apple platforms: their
    /// hard limit is usually unlimited, a soft limit above this cap is refused on some
    /// versions, and a soft limit of `u64::MAX` would be read by any code that sizes a
    /// table, or walks every descriptor, by the limit.
    #[cfg(target_vendor = "apple")]
    pub(super) fn per_process_cap() -> Option<u64> {
        let mut value: libc::c_int = 0;
        let mut size = std::mem::size_of::<libc::c_int>();
        // SAFETY: the name is NUL-terminated, and `value` and `size` describe a writable
        // buffer of the size the kernel reports this integer in.
        let ok = unsafe {
            libc::sysctlbyname(
                c"kern.maxfilesperproc".as_ptr(),
                (&raw mut value).cast(),
                &mut size,
                std::ptr::null_mut(),
                0,
            )
        } == 0;
        (ok && value > 0).then_some(value as u64)
    }

    /// Linux and Android cap the hard limit itself (`fs.nr_open`), so it needs no other.
    #[cfg(not(target_vendor = "apple"))]
    pub(super) fn per_process_cap() -> Option<u64> {
        None
    }
}

#[cfg(not(any(target_os = "linux", target_os = "android", target_vendor = "apple")))]
mod imp {
    use super::OpenFiles;
    use std::io;

    pub(super) fn raise() -> io::Result<Option<OpenFiles>> {
        Ok(None)
    }
}

#[cfg(all(
    test,
    any(target_os = "linux", target_os = "android", target_vendor = "apple")
))]
mod tests {
    use super::*;
    use rustix::process::{Resource, Rlimit, getrlimit, setrlimit};

    /// One test, as the limit is the process's: tests run on threads of one process, and
    /// two that set it would race.
    #[test]
    fn the_soft_limit_is_raised_to_the_hard_one_and_kept_there() {
        let start = getrlimit(Resource::Nofile);
        let hard = start.maximum.unwrap_or(u64::MAX);

        // Start low, as a process launchd started would.
        setrlimit(
            Resource::Nofile,
            Rlimit {
                current: Some(256.min(hard)),
                maximum: start.maximum,
            },
        )
        .unwrap();
        let raised = raise_open_files().unwrap().unwrap();
        assert_eq!(raised.before, 256.min(hard));
        assert!(raised.now > raised.before || hard <= 256, "{raised:?}");
        assert!(raised.now <= hard);
        assert_eq!(getrlimit(Resource::Nofile).current, Some(raised.now));

        // Raising again changes nothing.
        let again = raise_open_files().unwrap().unwrap();
        assert_eq!(again.before, raised.now);
        assert_eq!(again.now, raised.now);

        // The hard limit is not touched.
        assert_eq!(getrlimit(Resource::Nofile).maximum, start.maximum);

        // Descriptors past the old limit can now be opened.
        let files: Vec<_> = (0..300)
            .map(|_| std::fs::File::open("/dev/null"))
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(files.len(), 300);
    }

    #[cfg(target_vendor = "apple")]
    #[test]
    fn apple_caps_at_the_per_process_maximum() {
        let cap = imp::per_process_cap().expect("kern.maxfilesperproc");
        assert!(cap >= 256, "{cap}");
    }
}
