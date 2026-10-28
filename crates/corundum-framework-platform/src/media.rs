//! The phone's photo picker: the photos and videos in the person's album, picked in the
//! system's own screen and copied into the app's cache.
//!
//! A phone has no file system the person browses, so where a desktop opens a file dialog
//! (corundum-framework-gui's `FilePicker`) a phone offers the album. The picker answers in a
//! callback, after the app has drawn many frames, so [`pick`] returns a [`Pending`] the app
//! polls. The files are copied into [`cache_dir`](crate::dirs::cache_dir), under
//! `media-picker/<request>/`, before the answer is given: neither an Android `content://`
//! URI nor an iOS picker's temporary file can be read by path once the callback is over.
//! The copies are the app's to move or delete; the system may also clear the cache.
//!
//! - **Android**: the Photo Picker (`MediaStore.ACTION_PICK_IMAGES`), on Android 13 and
//!   on Android 11 and 12 with the system's SDK extension 2 (a Google Play system update);
//!   elsewhere `ACTION_GET_CONTENT` for images and videos. None needs a storage permission.
//!   The result comes back to the activity's `onActivityResult`, which the app's activity
//!   must forward (`corundumActivityResult`, see [`available`]); the files are copied
//!   through `ContentResolver.openInputStream` on a thread of their own.
//! - **iOS**: `PHPickerViewController`, presented over the window's root view controller.
//!   It needs no permission and no Info.plist key: the picker runs out of the app's process
//!   and hands over only what was picked.
//! - **Elsewhere**: nothing; a desktop app opens its file dialog.

use std::path::{Path, PathBuf};

pub use crate::pending::Pending;

/// What the picker offers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MediaKind {
    /// Photos and other images.
    Images,
    /// Videos.
    Videos,
    /// Both.
    #[default]
    ImagesAndVideos,
}

impl MediaKind {
    /// Does this kind take images?
    pub fn images(self) -> bool {
        matches!(self, MediaKind::Images | MediaKind::ImagesAndVideos)
    }

    /// Does this kind take videos?
    pub fn videos(self) -> bool {
        matches!(self, MediaKind::Videos | MediaKind::ImagesAndVideos)
    }
}

/// Can [`pick`] open the system's photo picker here?
///
/// iOS always can. Android can once the window has given the platform its activity, and
/// only when the app's activity forwards the pickers' results to corundum. In the activity
/// that extends `GameActivity`:
///
/// ```java
/// @Override
/// @SuppressWarnings("deprecation")
/// protected void onActivityResult(int requestCode, int resultCode, Intent data) {
///     super.onActivityResult(requestCode, resultCode, data);
///     corundumActivityResult(requestCode, resultCode, data);
/// }
///
/// // Registered by corundum-framework-platform when the window starts.
/// private native void corundumActivityResult(int requestCode, int resultCode, Intent data);
/// ```
///
/// Off the phones, `false`.
pub fn available() -> bool {
    #[cfg(target_os = "android")]
    return android::available();
    #[cfg(target_os = "ios")]
    return true;
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    return false;
}

/// Open the system's photo picker for `kind`, letting the person pick up to `limit` items
/// (0: as many as the system allows), and return at once.
///
/// The answer, through [`Pending::poll`], is the copies' paths in the order picked: none
/// when the person cancels, and none for an item that could not be copied (which is
/// logged). `wake` runs, on another thread, once the answer is there: a window asks itself
/// for a frame. Where the picker is not [`available`], the answer is there at once, and
/// empty. Call it from the thread the window runs on (iOS's main thread).
pub fn pick(
    kind: MediaKind,
    limit: usize,
    wake: impl Fn() + Send + Sync + 'static,
) -> Pending<Vec<PathBuf>> {
    let (pending, sender) = crate::pending::channel(wake);
    #[cfg(target_os = "android")]
    android::pick(kind, limit, sender);
    #[cfg(target_os = "ios")]
    ios::pick(kind, limit, sender);
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let _ = (kind, limit);
        tracing::debug!("no photo picker off the phones");
        drop(sender);
    }
    pending
}

/// A new directory for one request's copies, under the cache's `media-picker`.
#[cfg_attr(not(any(target_os = "android", target_os = "ios")), allow(dead_code))]
fn request_dir() -> Option<PathBuf> {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNT: AtomicU32 = AtomicU32::new(0);
    let since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let count = COUNT.fetch_add(1, Ordering::Relaxed);
    let dir = crate::dirs::cache_dir()?
        .join("media-picker")
        .join(format!("{since}-{count}"));
    match std::fs::create_dir_all(&dir) {
        Ok(()) => Some(dir),
        Err(err) => {
            tracing::warn!("could not make {}: {err}", dir.display());
            None
        }
    }
}

/// Where in `dir` to copy the `index`th item, named `name` by the system: its last path
/// component, without leading dots, or `media-<index>` when nothing is left of it; with
/// the index in front when an earlier item took the name.
#[cfg_attr(not(any(target_os = "android", target_os = "ios")), allow(dead_code))]
fn destination(dir: &Path, name: Option<&str>, index: usize) -> PathBuf {
    let name = name
        .and_then(|name| name.rsplit(['/', '\\']).next())
        .map(|name| name.trim_start_matches('.'))
        .filter(|name| !name.is_empty())
        .map_or_else(|| format!("media-{index}"), str::to_owned);
    let path = dir.join(&name);
    if path.exists() {
        dir.join(format!("{index}-{name}"))
    } else {
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_keeps_the_system_s_name_but_not_its_path() {
        let dir = tempfile::tempdir().unwrap();
        let at = |name| destination(dir.path(), name, 3);
        assert_eq!(at(Some("IMG_0001.HEIC")), dir.path().join("IMG_0001.HEIC"));
        assert_eq!(at(Some("../../etc/passwd")), dir.path().join("passwd"));
        assert_eq!(at(Some(".hidden")), dir.path().join("hidden"));
        assert_eq!(at(Some("")), dir.path().join("media-3"));
        assert_eq!(at(None), dir.path().join("media-3"));
    }

    #[test]
    fn a_name_taken_already_gets_the_index() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("photo.jpg"), b"").unwrap();
        assert_eq!(
            destination(dir.path(), Some("photo.jpg"), 1),
            dir.path().join("1-photo.jpg")
        );
    }

    #[test]
    fn the_kinds_say_what_they_take() {
        assert!(MediaKind::Images.images() && !MediaKind::Images.videos());
        assert!(!MediaKind::Videos.images() && MediaKind::Videos.videos());
        assert!(MediaKind::default().images() && MediaKind::default().videos());
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[test]
    fn off_the_phones_the_picker_answers_nothing_at_once() {
        assert!(!available());
        assert_eq!(pick(MediaKind::Images, 1, || {}).poll(), Some(Vec::new()));
    }
}

#[cfg(target_os = "android")]
mod android {
    //! The Photo Picker, or `ACTION_GET_CONTENT`, started for a result, and its URIs copied
    //! on a thread of their own.

    use std::io::Write;
    use std::path::{Path, PathBuf};

    use jni::errors::Result;
    use jni::objects::{JByteArray, JObject, JString};
    use jni::{Env, jni_sig, jni_str};

    use super::{MediaKind, destination, request_dir};
    use crate::pending::Sender;

    /// The request code the picker is started with. Any number the app does not use itself.
    const REQUEST_CODE: i32 = 0x434f_0001;
    /// `Activity.RESULT_OK`.
    const RESULT_OK: i32 = -1;

    pub(super) fn available() -> bool {
        crate::android::with_env(|env| {
            let Some(activity) = crate::android::activity(env) else {
                return Ok(false);
            };
            crate::android::forwards_results(env, &activity)
        })
        .inspect_err(|err| tracing::debug!("no photo picker: {err}"))
        .unwrap_or(false)
    }

    pub(super) fn pick(kind: MediaKind, limit: usize, sender: Sender<Vec<PathBuf>>) {
        let started = crate::android::with_env(|env| {
            let intent = intent(env, kind, limit)?;
            let handler = Box::new(move |env: &mut Env<'_>, code: i32, data: &JObject<'_>| {
                answer(env, code, data, limit, sender);
            });
            crate::android::start_activity_for_result(env, &intent, REQUEST_CODE, handler)
        });
        match started {
            Ok(true) => {}
            // The handler, and its sender with it, is dropped: the request answers nothing.
            Ok(false) => tracing::warn!("no activity forwards its results: no photo picker"),
            Err(err) => tracing::warn!("could not open the photo picker: {err}"),
        }
    }

    /// The Photo Picker's intent where the system has it, `ACTION_GET_CONTENT`'s elsewhere.
    fn intent<'local>(
        env: &mut Env<'local>,
        kind: MediaKind,
        limit: usize,
    ) -> Result<JObject<'local>> {
        let mime = match kind {
            MediaKind::Images => Some("image/*"),
            MediaKind::Videos => Some("video/*"),
            MediaKind::ImagesAndVideos => None,
        };
        if photo_picker(env)? {
            let intent = new_intent(env, "android.provider.action.PICK_IMAGES")?;
            if let Some(mime) = mime {
                set_type(env, &intent, mime)?;
            }
            if limit != 1 {
                let most = env
                    .call_static_method(
                        jni_str!("android/provider/MediaStore"),
                        jni_str!("getPickImagesMaxLimit"),
                        jni_sig!("()I"),
                        &[],
                    )?
                    .i()?;
                let max = match limit {
                    0 => most,
                    limit => most.min(i32::try_from(limit).unwrap_or(i32::MAX)),
                };
                let key = env.new_string("android.provider.extra.PICK_IMAGES_MAX")?;
                env.call_method(
                    &intent,
                    jni_str!("putExtra"),
                    jni_sig!("(Ljava/lang/String;I)Landroid/content/Intent;"),
                    &[(&key).into(), max.into()],
                )?;
            }
            return Ok(intent);
        }
        let intent = new_intent(env, "android.intent.action.GET_CONTENT")?;
        let openable = env.new_string("android.intent.category.OPENABLE")?;
        env.call_method(
            &intent,
            jni_str!("addCategory"),
            jni_sig!("(Ljava/lang/String;)Landroid/content/Intent;"),
            &[(&openable).into()],
        )?;
        match mime {
            Some(mime) => set_type(env, &intent, mime)?,
            None => {
                set_type(env, &intent, "*/*")?;
                let image = env.new_string("image/*")?;
                let video = env.new_string("video/*")?;
                let types = env.new_object_array(2, jni_str!("java/lang/String"), &image)?;
                types.set_element(env, 1, &video)?;
                let key = env.new_string("android.intent.extra.MIME_TYPES")?;
                env.call_method(
                    &intent,
                    jni_str!("putExtra"),
                    jni_sig!("(Ljava/lang/String;[Ljava/lang/String;)Landroid/content/Intent;"),
                    &[(&key).into(), (&types).into()],
                )?;
            }
        }
        if limit != 1 {
            let key = env.new_string("android.intent.extra.ALLOW_MULTIPLE")?;
            env.call_method(
                &intent,
                jni_str!("putExtra"),
                jni_sig!("(Ljava/lang/String;Z)Landroid/content/Intent;"),
                &[(&key).into(), true.into()],
            )?;
        }
        Ok(intent)
    }

    /// Has the system the Photo Picker: Android 13, or 11 and 12 with SDK extension 2?
    fn photo_picker(env: &mut Env<'_>) -> Result<bool> {
        let sdk = crate::android::sdk_int(env)?;
        if sdk >= 33 {
            return Ok(true);
        }
        if sdk < 30 {
            return Ok(false);
        }
        let extension = env
            .call_static_method(
                jni_str!("android/os/ext/SdkExtensions"),
                jni_str!("getExtensionVersion"),
                jni_sig!("(I)I"),
                &[30.into()],
            )?
            .i()?;
        Ok(extension >= 2)
    }

    fn new_intent<'local>(env: &mut Env<'local>, action: &str) -> Result<JObject<'local>> {
        let action = env.new_string(action)?;
        env.new_object(
            jni_str!("android/content/Intent"),
            jni_sig!("(Ljava/lang/String;)V"),
            &[(&action).into()],
        )
    }

    fn set_type(env: &mut Env<'_>, intent: &JObject<'_>, mime: &str) -> Result<()> {
        let mime = env.new_string(mime)?;
        env.call_method(
            intent,
            jni_str!("setType"),
            jni_sig!("(Ljava/lang/String;)Landroid/content/Intent;"),
            &[(&mime).into()],
        )?;
        Ok(())
    }

    /// The picker's result, on the activity's main thread: read the URIs, and copy them on
    /// a thread of their own, as a video takes a while.
    fn answer(
        env: &mut Env<'_>,
        code: i32,
        data: &JObject<'_>,
        limit: usize,
        sender: Sender<Vec<PathBuf>>,
    ) {
        if code != RESULT_OK || data.is_null() {
            sender.send(Vec::new());
            return;
        }
        let mut uris = match uris(env, data) {
            Ok(uris) => uris,
            Err(err) => {
                tracing::warn!("could not read the photo picker's result: {err}");
                sender.send(Vec::new());
                return;
            }
        };
        // ACTION_GET_CONTENT has no limit of its own.
        if limit > 0 {
            uris.truncate(limit);
        }
        std::thread::spawn(move || sender.send(copy_all(&uris)));
    }

    /// The result's URIs, as strings: its clip data's, or its data's alone.
    fn uris(env: &mut Env<'_>, data: &JObject<'_>) -> Result<Vec<String>> {
        let clip = env
            .call_method(
                data,
                jni_str!("getClipData"),
                jni_sig!("()Landroid/content/ClipData;"),
                &[],
            )?
            .l()?;
        let mut uris = Vec::new();
        if !clip.is_null() {
            let count = env
                .call_method(&clip, jni_str!("getItemCount"), jni_sig!("()I"), &[])?
                .i()?;
            for at in 0..count {
                let item = env
                    .call_method(
                        &clip,
                        jni_str!("getItemAt"),
                        jni_sig!("(I)Landroid/content/ClipData$Item;"),
                        &[at.into()],
                    )?
                    .l()?;
                let uri = env
                    .call_method(
                        &item,
                        jni_str!("getUri"),
                        jni_sig!("()Landroid/net/Uri;"),
                        &[],
                    )?
                    .l()?;
                if let Some(uri) = uri_string(env, &uri)? {
                    uris.push(uri);
                }
            }
        } else {
            let uri = env
                .call_method(
                    data,
                    jni_str!("getData"),
                    jni_sig!("()Landroid/net/Uri;"),
                    &[],
                )?
                .l()?;
            uris.extend(uri_string(env, &uri)?);
        }
        Ok(uris)
    }

    fn uri_string(env: &mut Env<'_>, uri: &JObject<'_>) -> Result<Option<String>> {
        if uri.is_null() {
            return Ok(None);
        }
        let text = env
            .call_method(
                uri,
                jni_str!("toString"),
                jni_sig!("()Ljava/lang/String;"),
                &[],
            )?
            .l()?;
        let text = env.cast_local::<JString>(text)?.try_to_string(env)?;
        Ok(Some(text))
    }

    /// Copy each of `uris` into a new directory in the cache; those that fail are logged
    /// and left out.
    fn copy_all(uris: &[String]) -> Vec<PathBuf> {
        let Some(dir) = request_dir() else {
            return Vec::new();
        };
        let mut copies = Vec::new();
        for (index, uri) in uris.iter().enumerate() {
            match crate::android::with_env(|env| copy(env, uri, &dir, index)) {
                Ok(Ok(path)) => copies.push(path),
                Ok(Err(err)) => tracing::warn!("could not copy {uri}: {err}"),
                Err(err) => tracing::warn!("could not read {uri}: {err}"),
            }
        }
        copies
    }

    /// Copy `uri` into `dir`, under the name its provider gives it.
    fn copy(
        env: &mut Env<'_>,
        uri: &str,
        dir: &Path,
        index: usize,
    ) -> Result<std::io::Result<PathBuf>> {
        let context = crate::android::context(env)?;
        let resolver = env
            .call_method(
                &context,
                jni_str!("getContentResolver"),
                jni_sig!("()Landroid/content/ContentResolver;"),
                &[],
            )?
            .l()?;
        let text = env.new_string(uri)?;
        let uri = env
            .call_static_method(
                jni_str!("android/net/Uri"),
                jni_str!("parse"),
                jni_sig!("(Ljava/lang/String;)Landroid/net/Uri;"),
                &[(&text).into()],
            )?
            .l()?;
        let name = display_name(env, &resolver, &uri)
            .inspect_err(|err| tracing::debug!("no display name: {err}"))
            .ok()
            .flatten();
        let path = destination(dir, name.as_deref(), index);
        let stream = env
            .call_method(
                &resolver,
                jni_str!("openInputStream"),
                jni_sig!("(Landroid/net/Uri;)Ljava/io/InputStream;"),
                &[(&uri).into()],
            )?
            .l()?;
        let copied = copy_stream(env, &stream, &path);
        let closed = env.call_method(&stream, jni_str!("close"), jni_sig!("()V"), &[]);
        let copied = copied?;
        closed?;
        Ok(copied.map(|()| path))
    }

    /// Read `stream` to its end into a new file at `path`.
    fn copy_stream(
        env: &mut Env<'_>,
        stream: &JObject<'_>,
        path: &Path,
    ) -> Result<std::io::Result<()>> {
        const CHUNK: usize = 64 * 1024;
        let mut file = match std::fs::File::create(path) {
            Ok(file) => file,
            Err(err) => return Ok(Err(err)),
        };
        let array: JByteArray = env.new_byte_array(CHUNK)?;
        let mut buffer = vec![0i8; CHUNK];
        loop {
            let read = env
                .call_method(
                    stream,
                    jni_str!("read"),
                    jni_sig!("([B)I"),
                    &[(&array).into()],
                )?
                .i()?;
            if read < 0 {
                return Ok(file.flush());
            }
            let read = read as usize;
            array.get_region(env, 0, &mut buffer[..read])?;
            // SAFETY: i8 and u8 have the same size and alignment.
            let bytes = unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), read) };
            if let Err(err) = file.write_all(bytes) {
                return Ok(Err(err));
            }
        }
    }

    /// The provider's `OpenableColumns.DISPLAY_NAME` for `uri`: the name the person knows the
    /// file by.
    fn display_name(
        env: &mut Env<'_>,
        resolver: &JObject<'_>,
        uri: &JObject<'_>,
    ) -> Result<Option<String>> {
        let column = env.new_string("_display_name")?;
        let columns = env.new_object_array(1, jni_str!("java/lang/String"), &column)?;
        let cursor = env
            .call_method(
                resolver,
                jni_str!("query"),
                jni_sig!(
                    "(Landroid/net/Uri;[Ljava/lang/String;Ljava/lang/String;[Ljava/lang/String;Ljava/lang/String;)Landroid/database/Cursor;"
                ),
                &[
                    uri.into(),
                    (&columns).into(),
                    (&JObject::null()).into(),
                    (&JObject::null()).into(),
                    (&JObject::null()).into(),
                ],
            )?
            .l()?;
        if cursor.is_null() {
            return Ok(None);
        }
        let name = (|| {
            let first = env
                .call_method(&cursor, jni_str!("moveToFirst"), jni_sig!("()Z"), &[])?
                .z()?;
            if !first {
                return Ok(None);
            }
            let name = env
                .call_method(
                    &cursor,
                    jni_str!("getString"),
                    jni_sig!("(I)Ljava/lang/String;"),
                    &[0.into()],
                )?
                .l()?;
            if name.is_null() {
                return Ok(None);
            }
            Ok(Some(env.cast_local::<JString>(name)?.try_to_string(env)?))
        })();
        env.call_method(&cursor, jni_str!("close"), jni_sig!("()V"), &[])?;
        name
    }
}

#[cfg(target_os = "ios")]
mod ios {
    //! `PHPickerViewController`, presented over the window, and the files it hands over
    //! copied in their completion blocks, before the system deletes them.
    //!
    //! objc2-photos-ui binds the picker's view controller for macOS alone (its superclass
    //! there is AppKit's), so the class is looked up by name and sent its messages
    //! directly; the configuration, the filters and the results are bound for iOS.

    use std::cell::RefCell;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use block2::RcBlock;
    use objc2::rc::{Allocated, Retained};
    use objc2::runtime::{AnyClass, AnyObject, NSObject, NSObjectProtocol};
    use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
    use objc2_foundation::{NSArray, NSError, NSString, NSURL};
    use objc2_photos_ui::{
        PHPickerConfiguration, PHPickerFilter, PHPickerResult, PHPickerViewControllerDelegate,
    };
    use objc2_ui_kit::UIViewController;

    use super::{MediaKind, destination, request_dir};
    use crate::pending::Sender;

    /// What the delegate needs once the person has picked.
    struct Request {
        kind: MediaKind,
        sender: Sender<Vec<PathBuf>>,
    }

    struct Ivars {
        request: RefCell<Option<Request>>,
    }

    define_class!(
        // SAFETY: NSObject has no subclassing requirements, and the class does not implement
        // Drop.
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "CorundumMediaPickerDelegate"]
        #[ivars = Ivars]
        struct Delegate;

        unsafe impl NSObjectProtocol for Delegate {}

        unsafe impl PHPickerViewControllerDelegate for Delegate {}

        impl Delegate {
            // PHPickerViewControllerDelegate's one method, which objc2-photos-ui binds for
            // macOS alone.
            #[unsafe(method(picker:didFinishPicking:))]
            fn did_finish_picking(&self, picker: &UIViewController, results: &NSArray<PHPickerResult>) {
                picker.dismissViewControllerAnimated_completion(true, None);
                if let Some(request) = self.ivars().request.take() {
                    copy_all(request, results);
                }
                DELEGATE.with_borrow_mut(|delegate| delegate.take());
            }
        }
    );

    thread_local! {
        /// The delegate of the picker on screen: the picker holds it weakly.
        static DELEGATE: RefCell<Option<Retained<Delegate>>> = const { RefCell::new(None) };
    }

    pub(super) fn pick(kind: MediaKind, limit: usize, sender: Sender<Vec<PathBuf>>) {
        let Some(mtm) = MainThreadMarker::new() else {
            tracing::warn!("the photo picker opens from the main thread alone");
            return;
        };
        let Some(over) = crate::ios::presenting_controller(mtm) else {
            tracing::warn!("no view controller to present the photo picker over");
            return;
        };
        let Some(picker) = picker(kind, limit) else {
            tracing::warn!("no PHPickerViewController (PhotosUI) here");
            return;
        };
        let delegate = Delegate::alloc(mtm).set_ivars(Ivars {
            request: RefCell::new(Some(Request { kind, sender })),
        });
        // SAFETY: NSObject's `init`, on a newly allocated object.
        let delegate: Retained<Delegate> = unsafe { msg_send![super(delegate), init] };
        // SAFETY: `setDelegate:` takes an object conforming to PHPickerViewControllerDelegate,
        // which the delegate does; the picker holds it weakly, and DELEGATE keeps it.
        let () = unsafe { msg_send![&picker, setDelegate: &*delegate] };
        // A request still on screen is replaced: its delegate goes, and its sender with it.
        DELEGATE.with_borrow_mut(|current| *current = Some(delegate));
        over.presentViewController_animated_completion(&picker, true, None);
    }

    /// A `PHPickerViewController` for `kind`, with `limit` (0: no limit, as UIKit has it).
    fn picker(kind: MediaKind, limit: usize) -> Option<Retained<UIViewController>> {
        // SAFETY: PHPickerConfiguration's and PHPickerFilter's constructors and setters,
        // with the values they document.
        let configuration = unsafe {
            let configuration = PHPickerConfiguration::new();
            let filter = match kind {
                MediaKind::Images => PHPickerFilter::imagesFilter(),
                MediaKind::Videos => PHPickerFilter::videosFilter(),
                MediaKind::ImagesAndVideos => {
                    PHPickerFilter::anyFilterMatchingSubfilters(&NSArray::from_retained_slice(&[
                        PHPickerFilter::imagesFilter(),
                        PHPickerFilter::videosFilter(),
                    ]))
                }
            };
            configuration.setFilter(Some(&filter));
            configuration.setSelectionLimit(isize::try_from(limit).unwrap_or(0));
            configuration
        };
        let class = AnyClass::get(c"PHPickerViewController")?;
        // SAFETY: `+alloc` and `-initWithConfiguration:`, PHPickerViewController's
        // initialiser, which returns the view controller (a UIViewController).
        unsafe {
            let picker: Allocated<AnyObject> = msg_send![class, alloc];
            let picker: Option<Retained<AnyObject>> =
                msg_send![picker, initWithConfiguration: &*configuration];
            Some(Retained::cast_unchecked(picker?))
        }
    }

    /// Copy each result's file into a new directory in the cache, and answer once the last
    /// one is in. The files come to blocks on queues of the system's.
    fn copy_all(request: Request, results: &NSArray<PHPickerResult>) {
        let Request { kind, sender } = request;
        let count = results.count();
        if count == 0 {
            sender.send(Vec::new());
            return;
        }
        let Some(dir) = request_dir() else {
            return;
        };
        let copies: Arc<Mutex<Vec<Option<PathBuf>>>> = Arc::new(Mutex::new(vec![None; count]));
        let left = Arc::new(AtomicUsize::new(count));
        let sender = Arc::new(Mutex::new(Some(sender)));
        let done = move |copies: &Mutex<Vec<Option<PathBuf>>>| {
            let copies = std::mem::take(&mut *copies.lock().unwrap_or_else(|p| p.into_inner()));
            if let Some(sender) = sender.lock().unwrap_or_else(|p| p.into_inner()).take() {
                sender.send(copies.into_iter().flatten().collect());
            }
        };
        let done = Arc::new(done);
        for (index, result) in results.iter().enumerate() {
            // SAFETY: PHPickerResult's `itemProvider`, which every result has.
            let provider = unsafe { result.itemProvider() };
            let image = NSString::from_str("public.image");
            let movie = NSString::from_str("public.movie");
            let kind_id = if kind.images() && provider.hasItemConformingToTypeIdentifier(&image) {
                Some(image)
            } else if kind.videos() && provider.hasItemConformingToTypeIdentifier(&movie) {
                Some(movie)
            } else {
                None
            };
            let Some(kind_id) = kind_id else {
                if left.fetch_sub(1, Ordering::SeqCst) == 1 {
                    done(&copies);
                }
                continue;
            };
            let (copies, left, done, dir) = (
                Arc::clone(&copies),
                Arc::clone(&left),
                Arc::clone(&done),
                dir.clone(),
            );
            let block = RcBlock::new(move |url: *mut NSURL, error: *mut NSError| {
                // SAFETY: the block's arguments are the file's URL or the error, either
                // nil, valid for the block's call.
                let (url, error) = unsafe { (url.as_ref(), error.as_ref()) };
                match url.and_then(|url| url.path()) {
                    Some(from) => {
                        let from = PathBuf::from(from.to_string());
                        let name = from.file_name().map(|name| name.to_string_lossy());
                        let to = destination(&dir, name.as_deref(), index);
                        match std::fs::copy(&from, &to) {
                            Ok(_) => {
                                copies.lock().unwrap_or_else(|p| p.into_inner())[index] = Some(to)
                            }
                            Err(err) => tracing::warn!("could not copy {}: {err}", from.display()),
                        }
                    }
                    None => tracing::warn!(
                        "the photo picker gave no file: {}",
                        error.map_or_else(String::new, |error| error
                            .localizedDescription()
                            .to_string())
                    ),
                }
                if left.fetch_sub(1, Ordering::SeqCst) == 1 {
                    done(&copies);
                }
            });
            // SAFETY: the block is Send (it holds Arcs and paths), as the method requires.
            let _progress = unsafe {
                provider.loadFileRepresentationForTypeIdentifier_completionHandler(&kind_id, &block)
            };
        }
    }
}
