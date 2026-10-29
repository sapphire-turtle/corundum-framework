//! What the sapphire apps and asteria ask of the OS, without egui: so that neither they nor
//! the crates without a window touch JNI or Objective-C themselves.
//!
//! [`dirs`] is where an app keeps its settings, data and cache, on the desktops and on
//! phones alike; [`locale`] is the system's preferred languages; [`clipboard`] and
//! [`insets`] are what a window needs and its toolkit cannot give it on a phone. On
//! Android, [`android`] is the JNI underneath: the process's JavaVM and context, and the
//! activity a window draws in. [`icon`] renders an app's SVG to PNG, icns and ico. [`launcher`] puts a desktop app in the OS's app menu. [`package`] tells whether the running
//! executable is a Velopack package.

#[cfg(target_os = "android")]
pub mod android;
pub mod clipboard;
pub mod dirs;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod icon;
pub mod insets;
pub mod launcher;
pub mod locale;
pub mod package;
