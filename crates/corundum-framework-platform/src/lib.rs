//! What the sapphire apps and asteria ask of the OS, without egui: so that neither they nor
//! the crates without a window touch JNI or Objective-C themselves.
//!
//! [`dirs`] is where an app keeps its settings, data and cache, on the desktops and on
//! phones alike; [`locale`] is the system's preferred languages; [`clipboard`] and
//! [`insets`] are what a window needs and its toolkit cannot give it on a phone. On
//! Android, [`android`] is the JNI underneath: the process's JavaVM and context, and the
//! activity a window draws in. [`package`] tells whether the running executable is a
//! Velopack package.
//!
//! The `icon` and `launcher` features, on by default, are the desktop's extras: `icon`
//! renders an app's SVG to PNG, icns and ico, and `launcher` puts a desktop app in the OS's
//! app menu (it uses `icon`). A crate without a window that needs only the directories, the
//! languages or [`package`] turns them off with `default-features = false`, which leaves
//! out resvg and, on Windows, the shell's COM bindings.
//!
//! The system's screens an app opens and waits for are behind features, off by default:
//! they need declarations of the app (an Info.plist key, a manifest entry, a Gradle
//! dependency, code in its activity) and link frameworks an app without them should not.
//! `media-picker` is [`media`], the phone's photo picker; `qr-scan` is [`qr`], its QR
//! code scanner. Each answers through a [`pending::Pending`] the app polls.
//!
//! With the `media-session` feature, `media_session` is the OS's media keys and its "now
//! playing", for a music player.
//!
//! With the `single-instance` feature, `instance` keeps a desktop app to one running copy:
//! a second start brings the first's window to the front and exits.

#[cfg(target_os = "android")]
pub mod android;
pub mod clipboard;
pub mod dirs;
#[cfg(all(feature = "icon", not(any(target_os = "ios", target_os = "android"))))]
pub mod icon;
pub mod insets;
#[cfg(feature = "single-instance")]
pub mod instance;
#[cfg(all(target_os = "ios", any(feature = "media-picker", feature = "qr-scan")))]
mod ios;
#[cfg(feature = "launcher")]
pub mod launcher;
pub mod locale;
#[cfg(feature = "media-picker")]
pub mod media;
#[cfg(feature = "media-session")]
pub mod media_session;
pub mod package;
#[cfg(any(feature = "media-picker", feature = "qr-scan"))]
pub mod pending;
#[cfg(feature = "qr-scan")]
pub mod qr;
