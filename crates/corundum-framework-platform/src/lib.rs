//! What the sapphire apps and asteria ask of the OS, without egui: so that neither they nor
//! the crates without a window touch JNI or Objective-C themselves.
//!
//! [`dirs`] is where an app keeps its settings, data and cache, on the desktops and on
//! phones alike; [`locale`] is the system's preferred languages; [`clipboard`] and
//! [`insets`] are what a window needs and its toolkit cannot give it on a phone. On
//! Android, [`android`] is the JNI underneath: the process's JavaVM and context, and the
//! activity a window draws in. [`icon`] renders an app's SVG to PNG, icns and ico. [`launcher`] puts a desktop app in the OS's app menu.
//!
//! The system's screens an app opens and waits for are behind features, off by default:
//! they need declarations of the app (an Info.plist key, a manifest entry, a Gradle
//! dependency, code in its activity) and link frameworks an app without them should not.
//! `media-picker` is [`media`], the phone's photo picker. Each answers through a
//! [`pending::Pending`] the app polls.

#[cfg(target_os = "android")]
pub mod android;
pub mod clipboard;
pub mod dirs;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod icon;
pub mod insets;
#[cfg(all(target_os = "ios", feature = "media-picker"))]
mod ios;
pub mod launcher;
pub mod locale;
#[cfg(feature = "media-picker")]
pub mod media;
#[cfg(feature = "media-picker")]
pub mod pending;
