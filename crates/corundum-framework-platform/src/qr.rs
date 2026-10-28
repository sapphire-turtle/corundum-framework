//! The phone's QR code scanner: the camera opened in a screen of the system's or of
//! corundum's, and the text of the first QR code it sees.
//!
//! A scanner answers after the app has drawn many frames, so [`scan`] returns a
//! [`Pending`] the app polls, as [`media`](crate::media)'s picker does.
//!
//! - **Android**: Google's code scanner (`GmsBarcodeScanning`, in Google Play services).
//!   Play services draws the camera's screen in its own process, so the app asks for no
//!   camera permission. The app depends on
//!   `com.google.android.gms:play-services-code-scanner`, and may ask Play to install the
//!   scanner's module with the app rather than at the first scan (see [`available`]).
//! - **iOS**: AVFoundation's capture session with a metadata output for QR codes, shown full
//!   screen over the window with a close button. VisionKit's `DataScannerViewController`
//!   would draw a richer screen, but it is a Swift class with no Objective-C interface, out
//!   of reach from Rust. The camera needs `NSCameraUsageDescription` in the app's
//!   Info.plist: iOS ends an app that opens the camera without it, so [`available`] says
//!   `false` when the key is missing.
//! - **Elsewhere**: nothing; the desktops show codes (corundum-framework-gui's `qr`) rather
//!   than read them.

pub use crate::pending::Pending;

/// Can [`scan`] open a scanner here?
///
/// - **Android**: once the window has given the platform its activity, when Google Play
///   services is on the phone and the app has the code scanner's library. In the app's
///   `build.gradle.kts`:
///
///   ```kotlin
///   dependencies {
///       implementation("com.google.android.gms:play-services-code-scanner:16.1.0")
///   }
///   ```
///
///   Play downloads the scanner's module at the first scan unless the manifest's
///   `<application>` asks for it at install:
///
///   ```xml
///   <meta-data
///       android:name="com.google.mlkit.vision.DEPENDENCIES"
///       android:value="barcode_ui" />
///   ```
///
/// - **iOS**: when the phone has a camera (a simulator has none) and the app's Info.plist
///   says why it uses it:
///
///   ```xml
///   <key>NSCameraUsageDescription</key>
///   <string>Scans QR codes, such as invite tickets.</string>
///   ```
///
/// Off the phones, `false`.
pub fn available() -> bool {
    #[cfg(target_os = "android")]
    return android::available();
    #[cfg(target_os = "ios")]
    return ios::available();
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    return false;
}

/// Open the scanner and return at once.
///
/// The answer, through [`Pending::poll`], is the first QR code's text, or `None` when the
/// person closes the scanner, or it fails (which is logged). `wake` runs, on another
/// thread or the main one, once the answer is there. Where there is no scanner
/// ([`available`]), the answer is there at once: `None`. Call it from the thread the
/// window runs on (iOS's main thread).
pub fn scan(wake: impl Fn() + Send + Sync + 'static) -> Pending<Option<String>> {
    let (pending, sender) = crate::pending::channel(wake);
    #[cfg(target_os = "android")]
    android::scan(sender);
    #[cfg(target_os = "ios")]
    ios::scan(sender);
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        tracing::debug!("no QR code scanner off the phones");
        drop(sender);
    }
    pending
}

#[cfg(target_os = "android")]
mod android {
    //! Google's code scanner, whose `Task` a thread of corundum's waits on with
    //! `Tasks.await`, so the app's activity needs no code of its own for it.
    //!
    //! The scanner's classes are the app's (its Gradle dependency), which a native thread's
    //! `FindClass` cannot see: they are loaded through the activity's class loader.

    use jni::errors::Result;
    use jni::objects::{JClass, JObject, JString};
    use jni::refs::Global;
    use jni::{Env, jni_sig, jni_str};

    use crate::pending::Sender;

    /// `Barcode.FORMAT_QR_CODE`.
    const FORMAT_QR_CODE: i32 = 256;
    /// `ConnectionResult.SUCCESS`.
    const SUCCESS: i32 = 0;

    pub(super) fn available() -> bool {
        crate::android::with_env(|env| {
            let Some(activity) = crate::android::activity(env) else {
                return Ok(false);
            };
            if load(
                env,
                &activity,
                "com.google.mlkit.vision.codescanner.GmsBarcodeScanning",
            )
            .is_err()
            {
                tracing::debug!("no play-services-code-scanner in the app: no QR code scanner");
                return Ok(false);
            }
            let availability = load(
                env,
                &activity,
                "com.google.android.gms.common.GoogleApiAvailability",
            )?;
            let instance = env
                .call_static_method(
                    &availability,
                    jni_str!("getInstance"),
                    jni_sig!("()Lcom/google/android/gms/common/GoogleApiAvailability;"),
                    &[],
                )?
                .l()?;
            let status = env
                .call_method(
                    &instance,
                    jni_str!("isGooglePlayServicesAvailable"),
                    jni_sig!("(Landroid/content/Context;)I"),
                    &[(&activity).into()],
                )?
                .i()?;
            Ok(status == SUCCESS)
        })
        .inspect_err(|err| tracing::debug!("no QR code scanner: {err}"))
        .unwrap_or(false)
    }

    pub(super) fn scan(sender: Sender<Option<String>>) {
        let started = crate::android::with_env(|env| {
            let Some(activity) = crate::android::activity(env) else {
                return Ok(None);
            };
            let task = start(env, &activity)?;
            let tasks = load(env, &activity, "com.google.android.gms.tasks.Tasks")?;
            Ok(Some((
                env.new_global_ref(&task)?,
                env.new_global_ref(&tasks)?,
            )))
        });
        match started {
            Ok(Some((task, tasks))) => {
                std::thread::spawn(move || {
                    let text = crate::android::with_env(|env| wait(env, &task, &tasks))
                        .unwrap_or_else(|err| {
                            tracing::warn!("the QR code scanner failed: {err}");
                            None
                        });
                    sender.send(text);
                });
            }
            Ok(None) => tracing::warn!("no activity: no QR code scanner"),
            Err(err) => tracing::warn!("could not open the QR code scanner: {err}"),
        }
    }

    /// `GmsBarcodeScanning.getClient(activity, options).startScan()`, for QR codes alone.
    fn start<'local>(env: &mut Env<'local>, activity: &JObject<'_>) -> Result<JObject<'local>> {
        let builder = load(
            env,
            activity,
            "com.google.mlkit.vision.codescanner.GmsBarcodeScannerOptions$Builder",
        )?;
        let builder = env.new_object(&builder, jni_sig!("()V"), &[])?;
        let more = env.new_int_array(0)?;
        env.call_method(
            &builder,
            jni_str!("setBarcodeFormats"),
            jni_sig!("(I[I)Lcom/google/mlkit/vision/codescanner/GmsBarcodeScannerOptions$Builder;"),
            &[FORMAT_QR_CODE.into(), (&more).into()],
        )?;
        let options = env
            .call_method(
                &builder,
                jni_str!("build"),
                jni_sig!("()Lcom/google/mlkit/vision/codescanner/GmsBarcodeScannerOptions;"),
                &[],
            )?
            .l()?;
        let scanning = load(
            env,
            activity,
            "com.google.mlkit.vision.codescanner.GmsBarcodeScanning",
        )?;
        let scanner = env
            .call_static_method(
                &scanning,
                jni_str!("getClient"),
                jni_sig!(
                    "(Landroid/content/Context;Lcom/google/mlkit/vision/codescanner/GmsBarcodeScannerOptions;)Lcom/google/mlkit/vision/codescanner/GmsBarcodeScanner;"
                ),
                &[activity.into(), (&options).into()],
            )?
            .l()?;
        env.call_method(
            &scanner,
            jni_str!("startScan"),
            jni_sig!("()Lcom/google/android/gms/tasks/Task;"),
            &[],
        )?
        .l()
    }

    /// Wait for the scan's `task`, off the main thread, and read its barcode's text. `None`
    /// when the person closed the scanner.
    fn wait(
        env: &mut Env<'_>,
        task: &Global<JObject<'static>>,
        tasks: &Global<JClass<'static>>,
    ) -> Result<Option<String>> {
        let barcode = env.call_static_method(
            tasks,
            jni_str!("await"),
            jni_sig!("(Lcom/google/android/gms/tasks/Task;)Ljava/lang/Object;"),
            &[task.as_obj().into()],
        );
        let barcode = match barcode.and_then(|barcode| barcode.l()) {
            Ok(barcode) => barcode,
            Err(err) => {
                let cancelled = env
                    .call_method(task.as_obj(), jni_str!("isCanceled"), jni_sig!("()Z"), &[])?
                    .z()?;
                return if cancelled { Ok(None) } else { Err(err) };
            }
        };
        let text = env
            .call_method(
                &barcode,
                jni_str!("getRawValue"),
                jni_sig!("()Ljava/lang/String;"),
                &[],
            )?
            .l()?;
        if text.is_null() {
            tracing::warn!("the QR code holds no text");
            return Ok(None);
        }
        Ok(Some(env.cast_local::<JString>(text)?.try_to_string(env)?))
    }

    /// The app's class `name` (with dots), through the activity's class loader.
    fn load<'local>(
        env: &mut Env<'local>,
        activity: &JObject<'_>,
        name: &str,
    ) -> Result<JClass<'local>> {
        let loader = env
            .call_method(
                activity,
                jni_str!("getClassLoader"),
                jni_sig!("()Ljava/lang/ClassLoader;"),
                &[],
            )?
            .l()?;
        let name = env.new_string(name)?;
        let class = env
            .call_method(
                &loader,
                jni_str!("loadClass"),
                jni_sig!("(Ljava/lang/String;)Ljava/lang/Class;"),
                &[(&name).into()],
            )?
            .l()?;
        env.cast_local::<JClass>(class)
    }
}

#[cfg(target_os = "ios")]
mod ios {
    //! A full-screen view controller of corundum's: the camera's picture under a close
    //! button, and a capture session whose metadata output looks for QR codes.

    use std::cell::RefCell;

    use dispatch2::{DispatchQoS, DispatchQueue, GlobalQueueIdentifier};
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject};
    use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
    use objc2_av_foundation::{
        AVAuthorizationStatus, AVCaptureConnection, AVCaptureDevice, AVCaptureDeviceInput,
        AVCaptureMetadataOutput, AVCaptureMetadataOutputObjectsDelegate, AVCaptureSession,
        AVCaptureVideoPreviewLayer, AVLayerVideoGravityResizeAspectFill, AVMediaTypeVideo,
        AVMetadataMachineReadableCodeObject, AVMetadataObject, AVMetadataObjectTypeQRCode,
    };
    use objc2_core_foundation::{CGPoint, CGRect, CGSize};
    use objc2_foundation::{NSArray, NSBundle, ns_string};
    use objc2_ui_kit::{
        UIButton, UIButtonType, UIColor, UIControlEvents, UIControlState, UIImage,
        UIModalPresentationStyle, UIResponder, UIViewController,
    };

    use crate::pending::Sender;

    /// The close button's side, in points: the smallest a finger hits, as Apple advises.
    const CLOSE_SIDE: f64 = 44.0;

    struct Ivars {
        session: Retained<AVCaptureSession>,
        preview: Retained<AVCaptureVideoPreviewLayer>,
        close: RefCell<Option<Retained<UIButton>>>,
        sender: RefCell<Option<Sender<Option<String>>>>,
    }

    define_class!(
        // SAFETY: UIViewController may be subclassed, the overridden methods call their
        // superclass's, and the class does not implement Drop.
        #[unsafe(super(UIViewController, UIResponder, NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "CorundumQrScanner"]
        #[ivars = Ivars]
        struct Scanner;

        unsafe impl NSObjectProtocol for Scanner {}

        // The output calls this on the main queue, as `scan` sets it.
        unsafe impl AVCaptureMetadataOutputObjectsDelegate for Scanner {
            #[unsafe(method(captureOutput:didOutputMetadataObjects:fromConnection:))]
            fn did_output(
                &self,
                _output: &AVCaptureMetadataOutput,
                objects: &NSArray<AVMetadataObject>,
                _connection: &AVCaptureConnection,
            ) {
                let text = objects.iter().find_map(|object| {
                    let code = object.downcast::<AVMetadataMachineReadableCodeObject>().ok()?;
                    // SAFETY: a machine-readable code's `stringValue`, nil when its bytes
                    // are not text.
                    unsafe { code.stringValue() }
                });
                if let Some(text) = text {
                    self.finish(Some(text.to_string()));
                }
            }
        }

        impl Scanner {
            #[unsafe(method(viewDidLoad))]
            fn view_did_load(&self) {
                // SAFETY: UIViewController's own `viewDidLoad`, as an override must call it.
                let () = unsafe { msg_send![super(self), viewDidLoad] };
                let Some(view) = self.view() else {
                    return;
                };
                view.setBackgroundColor(Some(&UIColor::blackColor()));
                view.layer().addSublayer(&self.ivars().preview);
                let close = UIButton::buttonWithType(UIButtonType::System, self.mtm());
                let image = UIImage::systemImageNamed(ns_string!("xmark.circle.fill"));
                close.setImage_forState(image.as_deref(), UIControlState::Normal);
                // SAFETY: white, on a button of the main thread's.
                unsafe { close.setTintColor(Some(&UIColor::whiteColor())) };
                // SAFETY: `close:` is this class's action method, taking the sender; the
                // button holds its target weakly, and the view controller outlives it.
                unsafe {
                    close.addTarget_action_forControlEvents(
                        Some(self),
                        sel!(close:),
                        UIControlEvents::TouchUpInside,
                    );
                }
                view.addSubview(&close);
                *self.ivars().close.borrow_mut() = Some(close);
            }

            #[unsafe(method(viewDidLayoutSubviews))]
            fn view_did_layout_subviews(&self) {
                // SAFETY: UIViewController's own `viewDidLayoutSubviews`.
                let () = unsafe { msg_send![super(self), viewDidLayoutSubviews] };
                let Some(view) = self.view() else {
                    return;
                };
                let bounds = view.bounds();
                self.ivars().preview.setFrame(bounds);
                if let Some(close) = &*self.ivars().close.borrow() {
                    let safe = view.safeAreaInsets();
                    let origin = CGPoint::new(
                        bounds.size.width - safe.right - CLOSE_SIDE - 8.0,
                        safe.top + 8.0,
                    );
                    close.setFrame(CGRect::new(origin, CGSize::new(CLOSE_SIDE, CLOSE_SIDE)));
                }
            }

            #[unsafe(method(close:))]
            fn close(&self, _sender: Option<&AnyObject>) {
                self.finish(None);
            }
        }
    );

    impl Scanner {
        /// Stop the camera, close the screen, and answer `text`; once.
        fn finish(&self, text: Option<String>) {
            let Some(sender) = self.ivars().sender.borrow_mut().take() else {
                return;
            };
            run_off_main(self.ivars().session.clone(), false);
            self.dismissViewControllerAnimated_completion(true, None);
            sender.send(text);
        }
    }

    /// The camera's media type, `AVMediaTypeVideo`.
    fn video() -> Option<&'static objc2_av_foundation::AVMediaType> {
        // SAFETY: an AVFoundation constant, set when the framework loads.
        unsafe { AVMediaTypeVideo }
    }

    pub(super) fn available() -> bool {
        let described = NSBundle::mainBundle()
            .objectForInfoDictionaryKey(ns_string!("NSCameraUsageDescription"))
            .is_some();
        if !described {
            tracing::warn!("no NSCameraUsageDescription in Info.plist: no QR code scanner");
            return false;
        }
        let Some(video) = video() else {
            return false;
        };
        // SAFETY: AVCaptureDevice's class method, with a media type it takes.
        unsafe { AVCaptureDevice::defaultDeviceWithMediaType(video) }.is_some()
    }

    pub(super) fn scan(sender: Sender<Option<String>>) {
        let Some(mtm) = MainThreadMarker::new() else {
            tracing::warn!("the QR code scanner opens from the main thread alone");
            return;
        };
        if !available() {
            return;
        }
        let Some(over) = crate::ios::presenting_controller(mtm) else {
            tracing::warn!("no view controller to present the QR code scanner over");
            return;
        };
        let Some(session) = session() else {
            return;
        };
        // SAFETY: a new metadata output.
        let output = unsafe { AVCaptureMetadataOutput::new() };
        // SAFETY: the session's own `canAddOutput:` and `addOutput:`, on the main thread
        // before it runs.
        unsafe {
            if !session.canAddOutput(&output) {
                tracing::warn!("the camera's session takes no metadata output");
                return;
            }
            session.addOutput(&output);
        }
        // SAFETY: a preview layer of the session, filling its bounds.
        let preview = unsafe {
            let preview = AVCaptureVideoPreviewLayer::layerWithSession(&session);
            if let Some(gravity) = AVLayerVideoGravityResizeAspectFill {
                preview.setVideoGravity(gravity);
            }
            preview
        };
        let scanner = Scanner::alloc(mtm).set_ivars(Ivars {
            session: session.clone(),
            preview,
            close: RefCell::new(None),
            sender: RefCell::new(Some(sender)),
        });
        // SAFETY: UIViewController's `init`, on a newly allocated object.
        let scanner: Retained<Scanner> = unsafe { msg_send![super(scanner), init] };
        // SAFETY: the delegate conforms to the protocol and is called on the main queue,
        // where its thread kind says it lives; the QR code type is set after the output is
        // in the session, as the output takes only types its session's inputs have.
        unsafe {
            output.setMetadataObjectsDelegate_queue(
                Some(ProtocolObject::from_ref(&*scanner)),
                Some(DispatchQueue::main()),
            );
            output
                .setMetadataObjectTypes(Some(&NSArray::from_slice(&[AVMetadataObjectTypeQRCode])));
        }
        scanner.setModalPresentationStyle(UIModalPresentationStyle::FullScreen);
        over.presentViewController_animated_completion(&scanner, true, None);
        run_off_main(session, true);
    }

    /// A capture session with the default camera as its input. Opening the camera asks the
    /// person for it the first time; a camera they refused gives none.
    fn session() -> Option<Retained<AVCaptureSession>> {
        let video = video()?;
        // SAFETY: AVCaptureDevice's class methods, with the camera's media type.
        let (status, device) = unsafe {
            (
                AVCaptureDevice::authorizationStatusForMediaType(video),
                AVCaptureDevice::defaultDeviceWithMediaType(video),
            )
        };
        if status == AVAuthorizationStatus::Denied || status == AVAuthorizationStatus::Restricted {
            tracing::warn!("the camera is refused to the app: no QR code scanner");
            return None;
        }
        let device = device?;
        // SAFETY: an input for the camera, made on the main thread.
        let input = match unsafe { AVCaptureDeviceInput::deviceInputWithDevice_error(&device) } {
            Ok(input) => input,
            Err(error) => {
                tracing::warn!(
                    "could not open the camera: {}",
                    error.localizedDescription()
                );
                return None;
            }
        };
        // SAFETY: a new session, given the input before it runs.
        unsafe {
            let session = AVCaptureSession::new();
            if !session.canAddInput(&input) {
                tracing::warn!("the camera's session takes no camera input");
                return None;
            }
            session.addInput(&input);
            Some(session)
        }
    }

    /// The session, for a queue of the system's.
    struct Session(Retained<AVCaptureSession>);

    // SAFETY: AVCaptureSession is documented to be started and stopped off the main thread,
    // and is used here for nothing else there.
    unsafe impl Send for Session {}

    impl Session {
        fn run(self, run: bool) {
            // SAFETY: see `Session`.
            unsafe {
                if run {
                    self.0.startRunning();
                } else {
                    self.0.stopRunning();
                }
            }
        }
    }

    /// Start or stop `session` on a background queue: both block, for a moment the main
    /// thread would freeze.
    fn run_off_main(session: Retained<AVCaptureSession>, run: bool) {
        let session = Session(session);
        let queue = DispatchQueue::global_queue(GlobalQueueIdentifier::QualityOfService(
            DispatchQoS::UserInitiated,
        ));
        queue.exec_async(move || session.run(run));
    }
}
