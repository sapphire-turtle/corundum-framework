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
| Platform | The clipboard (copy, read, and Android's Paste button), the safe area and window insets, the screen's rects, and the app's directories and the system's languages from corundum-framework-platform |
| Media | A made-up player (a clock over three tracks; no sound) under the OS's media keys and in its "now playing", through the media session: its controls, its progress bar, whether the OS sees it, and the last event the OS sent |
| Settings | An app's Settings screen (`settings::Settings`): a page of the demo's own (marked in the list until applied), Appearance over two scopes (a server's look and this device's own, which wins), and About |

The demo logs the window's rects and the safe area whenever they change (`corundum demo:
window`), so a CI log shows what the insets did.

## The OS's features

The demo turns on every feature of corundum that its OS has an implementation of, so that
each can be tried from the demo as built, with no flags: today that is the media session
(`media-session`) on Linux, macOS, Windows and iOS. Android has no media session yet;
there the Media screen says so, and its player runs without the OS.

To try the media session, open Media and press Play, then:

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

## Desktops (Linux, macOS, Windows)

```sh
cargo run -p corundum-framework-demo
```

Make the window narrower than 600 points to see the phone layout.

To see the demo in the app menu, install it (`cargo install --path demo`) and run it once.
A build in a cargo target directory, as `cargo run` makes, adds no entry. Settings, About
removes the entry and adds it back.

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
xcrun simctl launch --console-pty booted io.github.sapphireturtle.corundum.demo
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
adb shell am start -n io.github.sapphireturtle.corundum.demo/.MainActivity
adb logcat -s corundum-demo
```

`build-rust.sh` builds the cdylib and puts it in `app/src/main/jniLibs`; the app is
GameActivity as it is (`MainActivity`), loading it and calling its `android_main`. Open
`demo/android` in Android Studio to run it from there instead.

A debug build runs because the workspace's `Cargo.toml` builds android-activity without
debug assertions (`[profile.dev.package.android-activity]`): android-activity 0.6 makes a
slice of the keyboard's text from a null pointer before there is any, and the check aborts.

## CI

CI builds and tests the demo on the desktops. Mobile CI runs clippy on it for iOS and
Android, builds the iOS app and the APK, starts each on the simulator and the emulator,
checks it is still running 20 seconds later, and uploads a screenshot (`demo-ios`,
`demo-android` artifacts).
