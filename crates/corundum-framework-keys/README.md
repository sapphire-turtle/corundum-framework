# corundum-framework-keys

API keys for an app's server. A client that talks to the server over HTTP — a recording
pendant, a webhook, another machine's agent — proves who it is with a bearer token.

- `KeyStore`: a directory of named keys, one TOML file each, in a place the app chooses.
  A synced directory works: two hosts adding keys never write the same file, and a retire
  left in a sync's conflict copy still holds. Only a hash of each token is kept (BLAKE3;
  SHA-256 is read for imported keys); the token is shown once.
- Every key names a profile: an id only the app understands (a role such as `admin`, a
  room profile, a user). corundum authenticates the token; what the profile may do is the
  app's to decide.
- `KeyVerifier`: the check a server makes on every request; a success is kept for 30 s,
  anything else reads the directory again.
- `require_key` (feature `axum`): a layer that lets in only requests with a valid
  `Authorization: Bearer` token (401 otherwise) whose key's profile the app resolves (403
  otherwise), with the key and the resolved profile in the request's extensions.
- `KeyCommand` (feature `clap`): `list`, `add`, `retire`, `restore` and `rotate`, on the
  files, so it needs no running server; `add --profile` takes one of the app's profiles.

The Settings page that manages keys is `corundum-framework-gui`'s `keys::KeysPage`
(feature `keys`).

```rust
use corundum_framework_keys::{KeyStore, KeyVerifier, ProfileId, Verdict};

let dir = std::path::Path::new("/srv/app/api_keys");
let mut store = KeyStore::open(dir)?.token_prefix("myapp-");
let (_key, token) = store.add("pendant", None, ProfileId::new("admin")?)?;
println!("{}", token.expose()); // shown once

let verifier = KeyVerifier::new(dir);
assert!(matches!(verifier.verify(token.expose()), Verdict::Accepted(_)));
```

With the `axum` feature, the app resolves each key's profile to its own:

```rust,ignore
let app = require_key(verifier, |key| roles.get(key.profile.as_str()).cloned(), router);
// In a handler: `Extension(who): Extension<Authenticated<Role>>`, then `who.profile`.
```

Licensed under MIT or Apache-2.0, at your option.
