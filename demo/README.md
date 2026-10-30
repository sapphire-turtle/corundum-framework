# corundum-framework-demo

An app that shows every piece corundum-framework shares, to try them on each OS apart
from the sapphire apps and asteria. It is a workspace member, built and tested with the
rest, and never published.

| Screen | What it shows |
|---|---|
| Overview | corundum's logo (`assets/corundum.svg`: sapphire's hexagon holding asteria's sparkle), and the `Shell`: the rail (a bottom bar below 600 points), the rail's action (counted here), the footer (Settings), the foot slot (the dot), and wide, the status bar (the last event, the dot, the detail pane's toggle) |
| Theme | `theme::settings_ui`, a `TintPicker` on its own, the palette (every tone by every swatch), widgets in the theme |
| Fonts | The system's CJK fallback font, with Japanese, Chinese and Korean samples |
| Icons | Every bundled Lucide icon, in the theme's colours and in a button's states (normal, selected, disabled) |
| Language | `i18n::settings_ui` (system, English, Japanese), switched in place; the demo's own messages, plurals included |
| Detail | `detail::DetailPane`: albums, and the picked one's tracks in the right column (hidden with the button at the status bar's right end), on a card over the screen with it hidden, or in the list's place on a phone |
| Files | `explorer::FileExplorer` over a sample folder tree in the demo's cache directory (made again with a button): drag entries into a folder (or onto the trail), rename with F2 or the menu — done for real there — page through Many files; on a desktop, any folder picked, to look at only |
| Platform | The clipboard (copy, read, and Android's Paste button), the safe area and window insets, the screen's rects, the file dialogs, the photo picker (`MediaPicker`), QR codes (`QrCode` for typed text, and `scan_qr`), and the app's directories and the system's languages from corundum-framework-platform |
| Media | A made-up player (a clock over three tracks; no sound) under the OS's media keys and in its "now playing", through the media session: its controls, its progress bar, whether the OS sees it, and the last event the OS sent |
| Settings | An app's Settings screen (`settings::Settings`): a page of the demo's own (marked in the list until applied), Appearance over two scopes (a server's look and this device's own, which wins), and About |

## The OS features it turns on

corundum's OS features are off by default, as each asks an app for declarations of its
own. The demo turns on every one its OS has, with no flag to pass, and declares what they
need, so a build as it is tries them all on a device. Where an OS has none, the screen
says so: the Platform screen in the feature's section, the Media screen for the media
session.

| Feature | Desktops | iOS | Android | What the demo declares for it |
|---|---|---|---|---|
| `media-picker` | the file dialog | PHPicker | the Photo Picker (`ACTION_GET_CONTENT` before Android 11) | Android: `MainActivity` forwards `onActivityResult` to `corundumActivityResult`. iOS: nothing |
| `qr-code` | yes | yes | yes | nothing |
| `qr-scan` | none | the camera (AVFoundation) | Google's code scanner | iOS: `NSCameraUsageDescription` in `project.yml`. Android: `play-services-code-scanner` in `app/build.gradle.kts`, and the `com.google.mlkit.vision.DEPENDENCIES` meta-data in the manifest, so Play installs the scanner with the app |
| `media-session` | MPRIS (Linux), the media flyout (Windows), Now Playing (macOS) | the lock screen and Control Centre | none yet | iOS: `UIBackgroundModes: audio` in `project.yml`, and MediaPlayer and AVFAudio linked |
| `single-instance` | one window: a second start brings the first's to the front and exits | not turned on (the OS runs an app once) | not turned on | nothing |

On a device, from the Platform screen:

- **Photos and videos**: pick photos, one video, and up to five of either. Each pick is
  listed with its size, as copied into the cache directory (shown below it). Cancelling
  lists nothing; the screen keeps drawing while the picker is open.
- **QR codes**: type in the field and watch the code change; scan it from another phone or
  the desktop demo's screen with **Scan a QR code**. The first scan on Android may wait for
  Play to download the scanner; iOS asks for the camera the first time. A simulator and an
  emulator have no camera worth scanning with: try these on a device.

To try the media session, open Media and press Play, then (on Android, which has no media
session yet, the Media screen says so and its player runs without the OS):

- Linux: the keyboard's media keys; GNOME's media panel (in the clock's menu) or KDE's
  media widget, which show the track and its cover; or `playerctl -p corundum_demo
  status|play-pause|next|position 30`. `busctl --user introspect
  org.mpris.MediaPlayer2.corundum_demo /org/mpris/MediaPlayer2` shows the player as D-Bus
  sees it.
- Windows: the keyboard's media keys, and the media flyout over the volume (Windows 10)
  or in the quick settings (Windows 11), with the track, the cover and the seek bar.
- macOS: the keyboard's media keys, and Now Playing in the Control Centre.
- iOS: the lock screen and the Control Centre, and headphones' buttons. The player plays
  silence while it plays, since iOS shows only a player that plays sound; the Info.plist
  lists `audio` under `UIBackgroundModes`, so it plays on with the screen locked. A call
  or pulling the headphones out pauses it.

The screen shows the last event the OS sent, by name.

The demo logs the window's rects and the safe area whenever they change (`corundum demo:
window`), so a CI log shows what the insets did.

## Desktops (Linux, macOS, Windows)

```sh
cargo run -p corundum-framework-demo
```

Make the window narrower than 600 points to see the phone layout.

To see the demo in the app menu, install it (`cargo install --path demo`) and run it once.
A build in a cargo target directory, as `cargo run` makes, adds no entry. Settings, About
removes the entry and adds it back.

### As a Velopack package

The `velopack` feature builds the demo as a Velopack package (corundum-framework#51,
`docs/superpowers/specs/2026-10-29-velopack-runtime-design.md`), on
`corundum-framework-package`: Velopack's start and hooks, updates from the repository's
pre-releases (checked at start and daily, applied from the About page's row or the status
bar's badge), and the command line `corundum-demo` (on `Path` by the installer on Windows,
through the console shim `corundum-framework-demo-console`; installed from the About page
on macOS and Linux). A second start of the package exits before it applies a
downloaded update: `main` takes the single-instance lock first and hands it to the window.
A Settings › Velopack page starts `corundum-framework-demo-daemon`,
which an update stops first, and shows the log the package's steps write. Pushing a tag
`demo-v<x.y.z>` packages it on each desktop (`demo-release.yml`) and puts the installers on
that pre-release.

## iOS simulator

Needs an Apple-silicon Mac with Xcode, [XcodeGen](https://github.com/yonaskolb/XcodeGen)
and the iOS targets:

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
cd demo/ios
xcodegen generate
open CorundumDemo.xcodeproj   # pick a simulator and Run
```

Or from the command line:

```sh
xcodebuild -project CorundumDemo.xcodeproj -scheme CorundumDemo -sdk iphonesimulator \
  -configuration Debug -derivedDataPath build build
xcrun simctl boot "iPhone 16 Pro"
xcrun simctl install booted build/Build/Products/Debug-iphonesimulator/CorundumDemo.app
xcrun simctl launch --console-pty booted net.sapphire-turtle.corundum-demo
```

The project's pre-build script (`build-rust.sh`) builds the demo's staticlib for the SDK
Xcode targets; `Sources/main.m` hands the main thread to it. The Info.plist declares a
scene with `CorundumSceneDelegate` as its delegate, as iOS 27 requires of an app built with
its SDK (corundum's window puts winit's in that scene). For a real device, copy
`Local.xcconfig.example` to `Local.xcconfig` and set your `DEVELOPMENT_TEAM` (and a bundle
id of your own).

## Android emulator

Needs the Android SDK with an NDK (`ANDROID_NDK_HOME`),
[cargo-ndk](https://github.com/bbqsrc/cargo-ndk), Gradle 8.11 or later and JDK 17:

```sh
rustup target add aarch64-linux-android x86_64-linux-android
cargo install cargo-ndk
demo/android/build-rust.sh            # arm64-v8a and x86_64; ABIS=x86_64 for one
gradle -p demo/android assembleDebug
adb install -r demo/android/app/build/outputs/apk/debug/app-debug.apk
adb shell am start -n net.sapphire_turtle.corundum_demo/.MainActivity
adb logcat -s corundum-demo
```

`build-rust.sh` builds the cdylib and puts it in `app/src/main/jniLibs`; the app is
GameActivity (`MainActivity`), loading it and calling its `android_main`, with the one
addition corundum's pickers need: it forwards `onActivityResult` to the native
`corundumActivityResult`, which corundum registers when the window starts. Open
`demo/android` in Android Studio to run it from there instead.

A debug build runs because the workspace's `Cargo.toml` builds android-activity without
debug assertions (`[profile.dev.package.android-activity]`): android-activity 0.6 makes a
slice of the keyboard's text from a null pointer before there is any, and the check aborts.

## CI

CI builds and tests the demo on the desktops. Mobile CI runs clippy on it for iOS and
Android, builds the iOS app and the APK, starts each on the simulator and the emulator,
checks it is still running 20 seconds later, and uploads a screenshot (`demo-ios`,
`demo-android` artifacts).
