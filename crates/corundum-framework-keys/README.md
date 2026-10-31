# corundum-framework-keys

API keys for an app's server. A client that talks to the server over HTTP — a recording
pendant, a webhook, another machine's agent — proves who it is with a bearer token.

- `KeyStore`: a directory of named keys, one TOML file each, in a place the app chooses.
  A synced directory works: two hosts adding keys never write the same file, and a retire
  left in a sync's conflict copy still holds. Only a hash of each token is kept (BLAKE3;
  SHA-256 is read for imported keys); the token is shown once.
- `KeyVerifier`: the check a server makes on every request; a success is kept for 30 s,
  anything else reads the directory again.
- `require_key` (feature `axum`): a layer that lets in only requests with a valid
  `Authorization: Bearer` token, with the key in the request's extensions.
- `KeyCommand` (feature `clap`): `list`, `add`, `retire`, `restore` and `rotate`, on the
  files, so it needs no running server.

The Settings page that manages keys is `corundum-framework-gui`'s `keys::KeysPage`
(feature `keys`).

```rust
use corundum_framework_keys::{KeyStore, KeyVerifier, Verdict};

let dir = std::path::Path::new("/srv/app/api_keys");
let mut store = KeyStore::open(dir)?.token_prefix("myapp-");
let (_key, token) = store.add("pendant", None, None)?;
println!("{}", token.expose()); // shown once

let verifier = KeyVerifier::new(dir);
assert!(matches!(verifier.verify(token.expose()), Verdict::Accepted(_)));
```

Licensed under MIT or Apache-2.0, at your option.
