# API keys: a key store, its verification and its management page

Issue: sapphire-framework#364 (this crate is what it builds on). Users: sapphire-framework
(keys in a workspace, synced with it) and asteria (keys in the server's config directory,
managed through the server's API). The sapphire side is
sapphire-framework's `docs/superpowers/specs/2026-10-31-workspace-api-keys-design.md`.

## Goal

An app server that speaks HTTP lets clients in with a bearer token: a recording pendant, a
webhook, another machine's agent, a phone. Both sapphire-framework and asteria have their
own version of this today — sapphire's "external devices" in a workgroup ledger owned by its
bridge, asteria's per-user tokens in its server config. After this, corundum has one:

- a store of named keys, one file per key, in a directory the app chooses;
- verification of a presented token against it, cheap enough for every request;
- an axum layer that does the checking for a router;
- a clap subcommand to manage the store from a terminal;
- a Settings page to manage keys from the GUI, over the files or over an app's own API.

Success means:

- sapphire keeps its keys in a workspace, and a key added on a host without HTTP is
  accepted, once the workspace has synced, by the host that serves HTTP;
- asteria keeps its keys in `~/.config/asteria-<app>/api_keys/` and its GUI manages a
  remote server's keys through that server's API, with the same page;
- a token is never stored, and is shown once.

## Decisions

1. **New crate `corundum-framework-keys`.** Features `axum` (the layer) and `clap` (the
   subcommand), both off by default, so a server that only verifies pulls in neither. The
   page lives in `corundum-framework-gui` behind its own `keys` feature.
2. **The store does not choose its directory.** `KeyStore::open(dir)`: the app decides —
   a synced workspace directory for sapphire, a config directory for asteria. The store
   never assumes it is the only writer of `dir`.
3. **One TOML file per key, `<id>.toml`.** Two hosts adding keys at once never touch the
   same file, which is what makes a synced directory work.
4. **Only a hash is stored**, with its algorithm: `hash = "blake3:<hex>"` for every key
   made here, `"sha256:<hex>"` accepted for keys imported from sapphire's ledger. A random
   256-bit token needs no slow hash. Every record is compared, in constant time, so the
   time taken does not say which one matched (as asteria does today).
5. **Tokens:** 32 random bytes, base64url without padding, after an optional prefix the
   app sets (`KeyStore::token_prefix("sapphire-ak-")`), so a leaked token says what it is.
   The hash covers the whole token, prefix included. Shown once, by `add` and `rotate`.
6. **Retire, don't delete.** A retired key stays in the store and can be restored; its id
   may be referenced (who wrote this?) after it stops working.
7. **Conflict copies retire, never accept.** In a synced directory, two hosts editing the
   same key at once leave a copy beside `<id>.toml` (sapphire names it
   `<id>.conflict-<replica>-<n>.toml`). The store reads every `*.toml`, groups records by
   their `id` field, and treats the one in `<id>.toml` as the key. A copy never lets a
   token in, but if any copy is retired, the key counts as retired: a retire made on one
   host is never lost to a concurrent edit on another.
8. **Verification reloads on a miss.** `KeyVerifier` keeps successes for 30 s (by the
   token's hash; the token is never kept) and reads the directory again on anything else.
   A key added by the CLI, the page or a sync is accepted on its first use; a retire takes
   effect within the cache's lifetime (plus, when synced, the sync delay).
9. **The page talks to a backend, not to files.** `KeysPage` draws whatever a `KeysBackend`
   reports and sends it the user's requests. `LocalKeys` is the backend over a `KeyStore`;
   asteria writes one over its server's API.

## The crate

### Types

```rust
/// File-name safe: ASCII letters, digits, `-` and `_`. New keys get a UUIDv7 in its simple
/// form; imported keys keep theirs (sapphire's are grain ids).
pub struct KeyId(String);

/// One key, as stored and as reported. Serialize/Deserialize, so an app can put it in its
/// own API's answers. Never holds the token.
pub struct Key {
    pub id: KeyId,
    /// Unique among the store's keys when added; see "Names" below.
    pub name: String,
    pub description: Option<String>,
    /// Who the key acts for, when the app has users (asteria's user id). sapphire leaves it
    /// empty.
    pub subject: Option<String>,
    pub hash: KeyHash,             // "blake3:<hex>" | "sha256:<hex>" in the file
    pub created_at: DateTime<Utc>,
    pub rotated_at: Option<DateTime<Utc>>,
    pub retired_at: Option<DateTime<Utc>>,
}

/// What a client sees of a key: everything but the hash.
pub struct KeyInfo { id, name, description, subject, created_at, rotated_at, retired_at }

/// The token, shown once. Debug prints `Token(…)`, not the token.
pub struct Token(String);
```

### `KeyStore`

```rust
impl KeyStore {
    pub fn open(dir: &Path) -> Result<KeyStore>;          // creates dir if absent
    pub fn token_prefix(self, prefix: &str) -> KeyStore;
    pub fn list(&self) -> &[Key];                          // sorted by created_at
    pub fn resolve(&self, selector: &str) -> Result<&Key>; // id, or a unique name
    pub fn add(&mut self, name: &str, description: Option<String>,
               subject: Option<String>) -> Result<(Key, Token)>;
    pub fn retire(&mut self, selector: &str) -> Result<Key>;
    pub fn restore(&mut self, selector: &str) -> Result<Key>;
    pub fn rotate(&mut self, selector: &str) -> Result<(Key, Token)>;
    /// Take a record made elsewhere as it is, hash included. Errs if the id exists.
    pub fn import(&mut self, key: Key) -> Result<Key>;
    pub fn authenticate(&self, token: &str) -> Option<&Key>;
}
```

- Writes go to a temporary file in `dir` and are renamed into place, so a reader (the
  verifier, a sync) never sees half a record.
- A file that does not parse is skipped with a `tracing` warning: one bad file does not
  lock every client out.
- **Names:** `add` refuses a name an unretired key has. Two hosts can still add the same
  name at once in a synced directory; `resolve` then refuses the name as ambiguous and
  asks for the id.
- `rotate` keeps the id and replaces the hash; `restore` clears `retired_at`.

### `KeyVerifier`

```rust
pub enum Verdict { Accepted(KeyInfo), Refused }

impl KeyVerifier {
    pub fn new(dir: &Path) -> KeyVerifier;
    pub fn cache_ttl(self, ttl: Duration) -> KeyVerifier;  // default 30 s
    pub fn verify(&self, token: &str) -> Verdict;
}
```

A cache hit answers without touching the disk. A miss opens the store afresh and
authenticates; a success is cached, a refusal removes the token's entry.

### axum layer (feature `axum`)

```rust
pub fn require_key(verifier: Arc<KeyVerifier>, router: Router) -> Router;
pub struct Authenticated(pub KeyInfo);  // in the request's extensions
```

`Authorization: Bearer <token>`; missing, malformed or refused → 401 with
`WWW-Authenticate: Bearer`. Verification runs on a blocking thread (it may read files).

### clap subcommand (feature `clap`)

```rust
#[derive(clap::Subcommand)]
pub enum KeyCommand {
    List { #[arg(long)] all: bool },                    // retired too with --all
    Add { name: String, #[arg(long)] description: Option<String>,
          #[arg(long)] subject: Option<String> },        // prints the token alone on stdout
    Retire { selector: String },
    Restore { selector: String },
    Rotate { selector: String },                         // prints the new token
}

impl KeyCommand {
    pub fn run(self, store: &mut KeyStore) -> Result<i32>;
}
```

The app mounts it under any name (`api-key`, `token`) and opens the store itself. It works
on the files, so it needs no running server.

## The page (`corundum-framework-gui`, feature `keys`)

```rust
pub trait KeysBackend {
    /// Start a request; its answer arrives through `poll`.
    fn send(&mut self, request: KeysRequest);
    /// Answers since the last call.
    fn poll(&mut self) -> Vec<KeysAnswer>;
}

pub enum KeysRequest { List, Add { name, description, subject }, Retire(KeyId),
                       Restore(KeyId), Rotate(KeyId) }
pub enum KeysAnswer { List(Vec<KeyInfo>), Changed(KeyInfo),
                      WithToken(KeyInfo, Token), Failed(String) }

pub struct KeysPage { … }
impl KeysPage {
    pub fn new(backend: Box<dyn KeysBackend>) -> KeysPage;
    pub fn ui(&mut self, ui: &mut egui::Ui);
}

/// The backend over a store on this machine. Answers on the next `poll`.
pub struct LocalKeys { … }
impl LocalKeys { pub fn new(store: KeyStore) -> LocalKeys; }
```

- The page: the keys (retired ones folded away), Add (name, description), and per key
  Retire / Restore / Rotate. Rotate and Retire ask to confirm. A new token is shown in a
  dialog with a copy button and a "shown only once" note, and is dropped when the dialog
  closes. A `Failed` answer shows as an error line above the list.
- `subject` is not edited on the page: an app with users sets it from its own screens.
- Strings are this crate's, in `en` and `ja`, under `corundum-keys-*`.
- The page asks for `List` when it is first drawn and after each change.

## Errors

`corundum_framework_keys::Error`: `Io { path, source }`, `Parse { path, message }` (only
from `import` and the write path: reading skips bad files), `NotFound(selector)`,
`Ambiguous(name)`, `NameTaken(name)`, `IdTaken(id)`, `InvalidId(String)`.

## Testing

- `KeyStore`: add → authenticate; retire → refused; restore → accepted; rotate → the old
  token refused, the new one accepted, the id kept; both `blake3:` and `sha256:` records
  accepted; `import` keeps the hash and refuses an existing id; a file that does not parse
  is skipped and the others still work; a token prefix is part of what is hashed.
- Conflict copies: a copy alone never accepts its token; a retired copy beside an
  unretired `<id>.toml` refuses the key; a copy of a different id is read as its own key's
  copy, not as a key.
- Names: `add` refuses a taken name; two records with one name make `resolve` ambiguous.
- `KeyVerifier`: a key written to the directory after the verifier was made is accepted;
  with a short TTL, a retire is refused once the TTL has passed.
- axum: no header, a malformed header and a wrong token each give 401; a good token
  reaches the handler with `Authenticated`.
- `KeyCommand`: `add` prints only the token on stdout; `list --all` includes retired keys.
- `KeysPage` with `egui_kittest` over a fake backend: Add sends the request and the token
  dialog shows the token once; closing it drops it; Retire asks to confirm before sending.

## Release

A new crate, so the first publish meets crates.io's new-crate rate limit: publish it on its
own before the release that depends on it. `corundum-framework-gui` gains a feature only,
which is not breaking; release-plz picks the versions.

## Out of scope

- Moving asteria to this crate: an issue in asteria (keys in
  `~/.config/asteria-<app>/api_keys/`, its GUI's backend over its server API).
- Per-key scopes or permissions beyond `subject`.
- Expiry dates.
