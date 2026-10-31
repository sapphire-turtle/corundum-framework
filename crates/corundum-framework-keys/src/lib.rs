//! API keys for an app's server.
//!
//! A client that talks to the server over HTTP (a recording pendant, a webhook, another
//! machine's agent) proves who it is with a bearer token. A [`KeyStore`] is a directory of
//! named keys, one TOML file each, in a place the app chooses: a synced directory works,
//! because two hosts adding keys never write the same file. Only a hash of each token is
//! kept; the token is shown once, when it is made.
//!
//! - [`KeyStore`]: add, list, retire, restore, rotate and import keys, and authenticate a
//!   token.
//! - [`KeyVerifier`]: the check a server makes on every request, with a short cache.
//! - `axum` feature: [`require_key`], a layer that does that check for a router.
//! - `clap` feature: [`KeyCommand`], the subcommand that manages a store from a terminal.
//!
//! See `docs/superpowers/specs/2026-10-31-api-keys-design.md`.

mod error;
mod hash;
mod key;
mod store;
mod verify;

pub use error::{Error, Result};
pub use hash::KeyHash;
pub use key::{Key, KeyId, KeyInfo, Token};
pub use store::KeyStore;
pub use verify::{CACHE_TTL, KeyVerifier, Verdict};
