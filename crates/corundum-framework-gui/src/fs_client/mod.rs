//! A workspace driven over the `fs.*` and `journal.*` methods of corundum-framework-fs-api.
//!
//! [`FsClient`] makes the calls, typed: each answers in a
//! [`Pending`](corundum_framework_platform::pending::Pending) the app polls each
//! frame, so the UI thread never waits for the workspace. The calls go through an
//! [`FsTransport`], a single method the app implements over whatever reaches its server
//! (asteria's HTTP, sapphire's local socket); with the `fs-local` feature,
//! [`LocalTransport`] answers in this process, from corundum-framework-fs's `FsService`.
//!
//! Each client is one origin, `gui:` and a random id, so that its undo and redo are its own.
//! [`FsClient::check_version`] asks the server's API version first: after a mismatch, every
//! call answers [`FsError::ApiMismatch`] at once.
//!
//! # An app's own transport
//!
//! ```no_run
//! use corundum_framework_gui::fs_client::{FsClient, FsError, FsTransport};
//! use corundum_framework_platform::pending::{self, Pending};
//! use serde_json::Value;
//!
//! /// Sends each call to the app's server on a thread of its own.
//! struct MyServer {
//!     wake: std::sync::Arc<dyn Fn() + Send + Sync>,
//! }
//!
//! fn ask_my_server(method: &str, params: Value) -> Result<Value, FsError> {
//!     // A request to the app's server; its JSON-RPC error becomes `FsError::Rpc`, a lost
//!     // connection `FsError::Transport`.
//!     # let _ = (method, params);
//!     Err(FsError::Transport("not connected".into()))
//! }
//!
//! impl FsTransport for MyServer {
//!     fn call(&self, method: &str, params: Value) -> Pending<Result<Value, FsError>> {
//!         let wake = self.wake.clone();
//!         // `None` if the thread is lost before it answers.
//!         let (answer, sender) = pending::channel(move || wake());
//!         let method = method.to_owned();
//!         std::thread::spawn(move || sender.send(Some(ask_my_server(&method, params))));
//!         answer.map(|answer| answer.unwrap_or_else(|| Err(FsError::default())))
//!     }
//! }
//!
//! # let ctx = corundum_framework_gui::egui::Context::default();
//! let client = FsClient::new(MyServer {
//!     wake: std::sync::Arc::new(move || ctx.request_repaint()),
//! });
//! let mut listing = client.list("", None);
//! // Each frame:
//! if let Some(answer) = listing.poll() {
//!     // ...
//! }
//! ```

mod client;
#[cfg(feature = "fs-local")]
mod local;
#[cfg(all(test, feature = "fs-local"))]
mod tests;
mod transport;

pub use client::FsClient;
#[cfg(feature = "fs-local")]
pub use local::LocalTransport;
pub use transport::{FsError, FsTransport};
