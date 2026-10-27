//! What a window needs from the platform and egui cannot give it: the clipboard where egui
//! has none, the margins the system's bars and keyboard take (Android), the system's
//! dialogs to pick a file or a folder, and its file manager.
//!
//! The clipboard and the margins are read through `corundum-framework-platform`, which
//! holds the JNI; everywhere but Android they say there is nothing to read. The dialogs and
//! the file manager are the desktops' (rfd's dialogs, and `explorer`, `open` or
//! `xdg-open`); on a phone there are none yet, which [`file_picker_available`] and its kin
//! say, so a view offers something else there.

use std::path::{Path, PathBuf};

/// Text on the system clipboard, where egui cannot read it itself.
///
/// egui-winit's clipboard covers the desktops; on Android it has none, so a field that
/// takes a long pasted value (an invite ticket) offers a Paste button reading this.
/// `None` where nothing is there, or where egui's own clipboard is the one to use.
pub fn clipboard_text() -> Option<String> {
    corundum_framework_platform::clipboard::text()
}

/// The margins the window must leave for the system's bars and the keyboard, in physical
/// pixels (left, top, right, bottom), where the window is drawn edge to edge under them.
pub fn window_insets() -> Option<[i32; 4]> {
    corundum_framework_platform::insets::window_insets()
}

/// Tell the platform layer which activity the window belongs to (Android).
#[cfg(all(target_os = "android", feature = "android"))]
pub(crate) fn set_android_activity(activity: *mut std::ffi::c_void) {
    corundum_framework_platform::android::set_activity(activity);
}

/// Does this platform need [`clipboard_text`] (a Paste button) rather than egui's paste?
pub fn needs_paste_button() -> bool {
    cfg!(target_os = "android")
}

/// Whether the system's dialogs and file manager are here: on the desktops.
const DESKTOP: bool = cfg!(not(any(target_os = "android", target_os = "ios")));

/// Image files, for a [`FilePicker::filter`]: an attachment, a cover.
pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp", "heic", "avif"];
/// Audio files, for a [`FilePicker::filter`].
pub const AUDIO_EXTENSIONS: &[&str] = &["mp3", "m4a", "aac", "flac", "ogg", "opus", "wav"];
/// Video files, for a [`FilePicker::filter`].
pub const VIDEO_EXTENSIONS: &[&str] = &["mp4", "m4v", "mov", "mkv", "webm"];

/// Can the person pick files with [`FilePicker`] here (a desktop)?
pub fn file_picker_available() -> bool {
    DESKTOP
}

/// Can the person pick a folder anywhere with [`pick_folder`] here (a desktop)?
pub fn folder_picker_available() -> bool {
    DESKTOP
}

/// Ask the person for a folder, titled `title`. `None` when they cancel — or always, where
/// there is no folder dialog ([`folder_picker_available`]). The dialog holds the frame
/// until it closes.
pub fn pick_folder(title: &str) -> Option<PathBuf> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        rfd::FileDialog::new().set_title(title).pick_folder()
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = title;
        None
    }
}

/// The system's dialog to pick files to open: the media to upload, the image to attach.
/// The dialog holds the frame until it closes. Where there is none
/// ([`file_picker_available`]), nothing is picked.
///
/// ```no_run
/// use corundum_framework_gui::platform::{FilePicker, IMAGE_EXTENSIONS};
///
/// let images = FilePicker::new()
///     .title("Attach images")
///     .filter("Images", IMAGE_EXTENSIONS)
///     .pick_files();
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FilePicker {
    title: Option<String>,
    filters: Vec<(String, Vec<String>)>,
    directory: Option<PathBuf>,
}

impl FilePicker {
    /// A dialog for any file, titled by the system.
    pub fn new() -> FilePicker {
        FilePicker::default()
    }

    /// Title the dialog `title`.
    pub fn title(mut self, title: impl Into<String>) -> FilePicker {
        self.title = Some(title.into());
        self
    }

    /// Offer the files whose extension is one of `extensions` (no dot, any case), as `name`.
    /// The first filter given is the one the dialog starts with.
    pub fn filter(mut self, name: impl Into<String>, extensions: &[&str]) -> FilePicker {
        let extensions = extensions.iter().map(|e| e.to_string()).collect();
        self.filters.push((name.into(), extensions));
        self
    }

    /// Open the dialog in `dir`.
    pub fn directory(mut self, dir: impl Into<PathBuf>) -> FilePicker {
        self.directory = Some(dir.into());
        self
    }

    /// One file; `None` when the person cancels.
    pub fn pick_file(self) -> Option<PathBuf> {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            self.dialog().pick_file()
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            None
        }
    }

    /// Any number of files; none when the person cancels.
    pub fn pick_files(self) -> Vec<PathBuf> {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            self.dialog().pick_files().unwrap_or_default()
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            Vec::new()
        }
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    fn dialog(self) -> rfd::FileDialog {
        let mut dialog = rfd::FileDialog::new();
        if let Some(title) = self.title {
            dialog = dialog.set_title(title);
        }
        for (name, extensions) in &self.filters {
            dialog = dialog.add_filter(name, extensions);
        }
        if let Some(dir) = self.directory {
            dialog = dialog.set_directory(dir);
        }
        dialog
    }
}

/// Can [`open_in_file_manager`] show a folder here (a desktop)?
pub fn file_manager_available() -> bool {
    DESKTOP
}

/// Show `path` in the system's file manager. Failures are ignored: it is a convenience.
pub fn open_in_file_manager(path: &Path) {
    #[cfg(windows)]
    let program = Some("explorer");
    #[cfg(target_os = "macos")]
    let program = Some("open");
    #[cfg(all(
        unix,
        not(any(target_os = "macos", target_os = "android", target_os = "ios"))
    ))]
    let program = Some("xdg-open");
    #[cfg(any(target_os = "android", target_os = "ios"))]
    let program: Option<&str> = None;
    if let Some(program) = program {
        let _ = std::process::Command::new(program).arg(path).spawn();
    }
}

// Off Android only: on a phone these read the real clipboard and insets.
#[cfg(all(test, not(target_os = "android")))]
mod tests {
    use super::*;

    #[test]
    fn off_android_there_is_no_clipboard_insets_or_paste_button() {
        assert_eq!(clipboard_text(), None);
        assert_eq!(window_insets(), None);
        assert!(!needs_paste_button());
    }

    #[cfg(not(target_os = "ios"))]
    #[test]
    fn a_desktop_has_the_dialogs_and_a_file_manager() {
        assert!(file_picker_available());
        assert!(folder_picker_available());
        assert!(file_manager_available());
    }

    #[test]
    fn the_picker_keeps_its_filters_in_order() {
        let picker = FilePicker::new()
            .title("Upload")
            .filter("Images", IMAGE_EXTENSIONS)
            .filter("Audio", &["mp3"])
            .directory("/tmp");
        assert_eq!(picker.title.as_deref(), Some("Upload"));
        let names: Vec<_> = picker.filters.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["Images", "Audio"]);
        assert_eq!(picker.filters[1].1, ["mp3"]);
        assert_eq!(picker.directory, Some(PathBuf::from("/tmp")));
    }
}
