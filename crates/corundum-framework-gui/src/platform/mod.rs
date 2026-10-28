//! What a window needs from the platform and egui cannot give it: the clipboard where egui
//! has none, the margins the system's bars and keyboard take (Android), the system's
//! dialogs to pick a file or a folder, and its file manager.
//!
//! The clipboard and the margins are read through `corundum-framework-platform`, which
//! holds the JNI; everywhere but Android they say there is nothing to read. The dialogs and
//! the file manager are the desktops' (rfd's dialogs, and `explorer`, `open` or
//! `xdg-open`); on a phone there are none yet, which [`file_picker_available`] and its kin
//! say, so a view offers something else there.
//!
//! What a phone offers instead of a file dialog is its photo album: with the
//! `media-picker` feature, [`MediaPicker`] picks photos and videos there, and from the file
//! dialog on a desktop. The phone's picker answers later, through a [`Pending`] the app
//! polls each frame.

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

/// What a [`MediaPicker`] offers: images, videos, or both.
#[cfg(feature = "media-picker")]
pub use corundum_framework_platform::media::MediaKind;
/// A request the system answers later, polled each frame: a [`MediaPicker`]'s.
#[cfg(feature = "media-picker")]
pub use corundum_framework_platform::pending::Pending;

/// Can the person pick photos and videos with [`MediaPicker`] here? On the desktops, and on
/// iOS; on Android, when the app's activity forwards the picker's result
/// (`corundum_framework_platform::media::available` has the code it needs).
#[cfg(feature = "media-picker")]
pub fn media_picker_available() -> bool {
    DESKTOP || corundum_framework_platform::media::available()
}

/// Photos and videos to attach or upload: the system's photo picker on a phone, and on a
/// desktop the file dialog, filtered to [`IMAGE_EXTENSIONS`] and [`VIDEO_EXTENSIONS`].
///
/// A phone's picker answers in a callback while the app goes on drawing, so [`pick`]
/// returns a [`Pending`] at once, which the app polls each frame until it answers: the
/// paths picked, in order, or none when the person cancels. The window is asked for a frame
/// when the answer arrives, so an app that draws only on input sees it too. A desktop's
/// dialog holds the frame until it closes, as [`FilePicker`]'s does, and its request has
/// answered by the time `pick` returns.
///
/// On a phone the paths are copies in the app's cache directory, under `media-picker/`
/// (neither an Android `content://` URI nor iOS's temporary file can be read later): move
/// or delete them once used. On a desktop they are the files themselves.
///
/// What the app must declare (corundum-framework-platform's `media` has the details): on
/// iOS nothing (PHPickerViewController needs no Info.plist key); on Android, its activity
/// forwards `onActivityResult` to corundum, as [`media_picker_available`] says.
///
/// ```no_run
/// use corundum_framework_gui::egui;
/// use corundum_framework_gui::platform::{MediaKind, MediaPicker, Pending};
/// # use std::path::PathBuf;
///
/// struct Attach {
///     request: Option<Pending<Vec<PathBuf>>>,
///     attached: Vec<PathBuf>,
/// }
///
/// fn ui(ui: &mut egui::Ui, attach: &mut Attach) {
///     if ui.button("Attach photos").clicked() {
///         let picker = MediaPicker::new().kind(MediaKind::Images).limit(4);
///         attach.request = Some(picker.pick(ui.ctx()));
///     }
///     if let Some(picked) = attach.request.as_mut().and_then(Pending::poll) {
///         attach.attached.extend(picked);
///         attach.request = None;
///     }
/// }
/// ```
///
/// [`pick`]: MediaPicker::pick
#[cfg(feature = "media-picker")]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MediaPicker {
    kind: MediaKind,
    limit: usize,
    title: Option<String>,
}

#[cfg(feature = "media-picker")]
impl MediaPicker {
    /// A picker for any number of images and videos.
    pub fn new() -> MediaPicker {
        MediaPicker::default()
    }

    /// Offer `kind`: images, videos, or both.
    pub fn kind(mut self, kind: MediaKind) -> MediaPicker {
        self.kind = kind;
        self
    }

    /// Let the person pick up to `limit` items; 0, the default, for as many as the system
    /// allows. A phone's picker stops at the limit; the desktop's dialog cannot, and the
    /// first `limit` files are kept.
    pub fn limit(mut self, limit: usize) -> MediaPicker {
        self.limit = limit;
        self
    }

    /// Title the desktop's dialog `title`. A phone's picker has the system's title.
    pub fn title(mut self, title: impl Into<String>) -> MediaPicker {
        self.title = Some(title.into());
        self
    }

    /// Open the picker, and return the request to poll each frame. `ctx` is asked for a
    /// frame when the answer arrives. Where there is no picker
    /// ([`media_picker_available`]), the request answers at once, with nothing.
    pub fn pick(self, ctx: &egui::Context) -> Pending<Vec<PathBuf>> {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            Pending::ready(self.pick_on_desktop(ctx))
        }
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let ctx = ctx.clone();
            corundum_framework_platform::media::pick(self.kind, self.limit, move || {
                ctx.request_repaint()
            })
        }
    }

    /// The file dialog, filtered to the kind's extensions: both kinds together first, then
    /// each alone.
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    fn pick_on_desktop(self, ctx: &egui::Context) -> Vec<PathBuf> {
        let mut dialog = FilePicker::new();
        if let Some(title) = self.title {
            dialog = dialog.title(title);
        }
        let images = crate::i18n::tr(ctx, "corundum-media-images");
        let videos = crate::i18n::tr(ctx, "corundum-media-videos");
        dialog = match self.kind {
            MediaKind::Images => dialog.filter(images, IMAGE_EXTENSIONS),
            MediaKind::Videos => dialog.filter(videos, VIDEO_EXTENSIONS),
            MediaKind::ImagesAndVideos => {
                let both: Vec<&str> = IMAGE_EXTENSIONS
                    .iter()
                    .chain(VIDEO_EXTENSIONS)
                    .copied()
                    .collect();
                dialog
                    .filter(
                        crate::i18n::tr(ctx, "corundum-media-images-and-videos"),
                        &both,
                    )
                    .filter(images, IMAGE_EXTENSIONS)
                    .filter(videos, VIDEO_EXTENSIONS)
            }
        };
        let mut picked = if self.limit == 1 {
            dialog.pick_file().into_iter().collect()
        } else {
            dialog.pick_files()
        };
        if self.limit > 0 {
            picked.truncate(self.limit);
        }
        picked
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

    #[cfg(feature = "media-picker")]
    #[test]
    fn the_media_picker_takes_any_number_of_both_kinds_by_default() {
        let picker = MediaPicker::new();
        assert_eq!(picker.kind, MediaKind::ImagesAndVideos);
        assert_eq!(picker.limit, 0);
        let picker = picker.kind(MediaKind::Videos).limit(1).title("Video");
        assert_eq!(picker.kind, MediaKind::Videos);
        assert_eq!(picker.limit, 1);
        assert_eq!(picker.title.as_deref(), Some("Video"));
    }

    #[cfg(all(feature = "media-picker", not(target_os = "ios")))]
    #[test]
    fn a_desktop_has_the_media_picker() {
        assert!(media_picker_available());
    }
}
