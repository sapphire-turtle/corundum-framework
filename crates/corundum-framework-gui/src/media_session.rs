//! The OS's media keys and its "now playing", for an egui app: corundum-framework-platform's
//! [`MediaSession`], and with the `eframe` feature [`start`], which makes one for the app's
//! window and has each event the OS sends wake the window for a frame.
//!
//! The app reads the events at the start of its frame and tells the session what changed:
//!
//! ```no_run
//! use corundum_framework_gui::media_session::{self, MediaEvent, MediaSession, MediaSessionSpec};
//! use corundum_framework_gui::{eframe, egui};
//!
//! struct Player {
//!     session: MediaSession,
//!     playing: bool,
//! }
//!
//! impl Player {
//!     fn new(cc: &eframe::CreationContext<'_>) -> Player {
//!         let spec = MediaSessionSpec { id: "net.example.Player", name: "Player", desktop_entry: None };
//!         // Without a session (no session bus, say), the player still plays.
//!         let session = media_session::start(cc, &spec).unwrap_or_else(|_| MediaSession::disconnected());
//!         Player { session, playing: false }
//!     }
//! }
//!
//! impl eframe::App for Player {
//!     fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
//!         while let Some(event) = self.session.poll() {
//!             if event == MediaEvent::Toggle {
//!                 self.playing = !self.playing;
//!             }
//!         }
//!     }
//! }
//! ```

pub use corundum_framework_platform::media_session::*;

/// A media session for the window `cc` makes, which repaints the window when the OS sends an
/// event, so that the app reads it within a frame even when idle.
#[cfg(feature = "eframe")]
pub fn start(
    cc: &eframe::CreationContext<'_>,
    spec: &MediaSessionSpec<'_>,
) -> std::io::Result<MediaSession> {
    let ctx = cc.egui_ctx.clone();
    MediaSession::new(spec, Some(cc), move || ctx.request_repaint())
}
