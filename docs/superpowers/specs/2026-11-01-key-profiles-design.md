# API keys: every key belongs to a profile

Issue: #85. Builds on `2026-10-31-api-keys-design.md`, and replaces its `subject` and its
"per-key scopes are out of scope".

## Goal

Today any accepted key can do anything the server offers. Apps need to hand out keys that
do less: a read-only key for a family member's phone, a key for sapphire-agent that acts
as one room profile and gets exactly that profile's rights.

#85 first proposed fixed scopes (`admin` / `editor` / `read-only`) in corundum. Instead,
each key names a **profile**, an id that only the app understands, and the app decides
what that profile may do:

- sapphire-agent binds keys to room profiles, which already carry rights such as
  administration;
- other sapphire apps use three fixed profiles, `admin`, `editor` and `read-only`, defined
  by the app (or a shared sapphire crate), not by corundum;
- asteria can use a user id, or its own fixed set.

corundum authenticates (token → key → profile id); the app authorizes (profile id → its
profile → allowed or not). Success means:

- every key has exactly one profile, chosen when the key is added, from a list the app
  gives;
- a request can reach a handler only after the app has resolved the key's profile, so an
  app that forgets to check cannot end up allowing everything;
- a key whose profile the app no longer knows is refused on its next request.

## Decisions

1. **A profile is an opaque id, `ProfileId`**: a non-empty string newtype. corundum stores
   and passes it; it never interprets it. Choosing between ids is the app's.
2. **The profile is required.** `Key::subject: Option<String>` becomes
   `Key::profile: ProfileId`. A key file without `profile` does not parse, so it is
   skipped when read, as other broken files are, and its token is refused. No migration:
   no app uses the crate yet.
3. **The app gives the list of profiles to choose from**, as `Profile { id, label }`. The
   label is for display only and is never stored. The store does not check a profile
   against the list (it does not know the list); the command line and the page do, before
   they add a key.
4. **A key's profile does not change.** No `set-profile`: if a profile goes away, retire
   the key and add a new one.
5. **`require_key` takes a resolver, and it is not optional.** The resolver maps a key to
   the app's profile, or to nothing. Nothing is 403 and the request never reaches a
   handler. An app that wants the bare id writes `|k| Some(k.profile.clone())`, and the
   "allow everything" is then visible in its code.
6. **The resolver runs on every request, uncached**, so removing a profile takes effect at
   once. The key check keeps its cache (`CACHE_TTL`).
7. **Per-route and per-method rules stay in the app**: its handlers or middleware look at
   `Authenticated<P>::profile`. corundum ships no scopes, no roles, no default profiles.
8. **The page lists every key, across profiles.** There is no page per profile; the add
   form has a required profile choice.

## API

### `corundum-framework-keys`

```rust
/// A profile's id: non-empty; otherwise the app's to interpret.
#[serde(transparent)]
pub struct ProfileId(String);
impl ProfileId {
    pub fn new(s: &str) -> Result<ProfileId>;   // Error::InvalidProfile if empty
    pub fn as_str(&self) -> &str;
}

/// A profile an app offers to choose from.
pub struct Profile { pub id: ProfileId, pub label: String }

pub struct Key {
    pub id: KeyId,
    pub name: String,
    pub description: Option<String>,
    pub profile: ProfileId,        // was `subject: Option<String>`
    pub hash: KeyHash,
    pub created_at: DateTime<Utc>,
    pub rotated_at: Option<DateTime<Utc>>,
    pub retired_at: Option<DateTime<Utc>>,
}

pub struct KeyInfo { id, name, description, profile, created_at, rotated_at, retired_at }

impl KeyStore {
    pub fn add(&mut self, name: &str, description: Option<String>,
               profile: ProfileId) -> Result<(Key, Token)>;
    // `import` takes a `Key`, so an imported key carries a profile its caller chose.
    // Everything else is unchanged.
}
```

`Error` gains `InvalidProfile(String)` (an empty id) and `UnknownProfile { id, known:
Vec<ProfileId> }` (an id not in the app's list; used by the command line and the page).

### `KeyCommand` (feature `clap`)

```rust
Add { name: String, #[arg(long)] description: Option<String>,
      #[arg(long)] profile: String },            // required

impl KeyCommand {
    pub fn run(self, store: &mut KeyStore, profiles: &[Profile]) -> Result<i32>;
    pub fn run_to(self, store: &mut KeyStore, profiles: &[Profile],
                  out: &mut dyn Write, err: &mut dyn Write) -> Result<i32>;
}
```

- `add` with a `--profile` not in `profiles` fails with `UnknownProfile`; its message lists
  the known ids.
- `list` gains a profile column: the label, or `<id> (unknown)` for an id not in
  `profiles`.

### `require_key` (feature `axum`)

```rust
pub fn require_key<P, F>(verifier: Arc<KeyVerifier>, resolve: F, router: Router) -> Router
where
    P: Clone + Send + Sync + 'static,
    F: Fn(&KeyInfo) -> Option<P> + Send + Sync + 'static;

/// The key a request was let in with, and its resolved profile; in the request's
/// extensions.
#[derive(Clone, Debug)]
pub struct Authenticated<P> { pub key: KeyInfo, pub profile: P }
```

| Request | Answer |
|---|---|
| no `Authorization: Bearer`, or a token that matches no unretired key | 401, `WWW-Authenticate: Bearer` (unchanged) |
| a good token, `resolve` returns `None` | 403; a `warn` log with the key's id and profile id |
| a good token, `resolve` returns `Some(p)` | the handler runs with `Authenticated { key, profile: p }` |

`resolve` runs inside the same `spawn_blocking` as the token check, so it may read files or
a database synchronously. Its result is not cached.

`KeyVerifier` and `Verdict` are unchanged.

### `corundum-framework-gui` (feature `keys`)

```rust
pub enum KeysRequest {
    List,
    Profiles,                                            // new
    Add { name: String, description: Option<String>, profile: ProfileId },
    Retire(KeyId), Restore(KeyId), Rotate(KeyId),
}
pub enum KeysAnswer {
    List(Vec<KeyInfo>),
    Profiles(Vec<Profile>),                              // new
    Changed(KeyInfo), WithToken(KeyInfo, Token), Failed(String),
}

impl LocalKeys {
    pub fn new(store: KeyStore, profiles: impl Fn() -> Vec<Profile> + 'static) -> LocalKeys;
}
```

- The page asks for `Profiles` whenever it asks for `List`: when first drawn and after
  each change. A backend whose profiles come and go (room profiles) is asked again each
  time; a remote backend (asteria) asks its server.
- `LocalKeys` answers `Add` with `Failed` if the profile is not in its list, as the
  command line does.
- The add form has a combo box of profiles, by label, with nothing selected at first; Add
  is disabled until one is. With an empty list the combo box is replaced by a line saying
  there are no profiles to choose from.
- The key list gains a profile column: the label, or `<id> (unknown)`.
- New strings in `en` and `ja`, under `corundum-keys-profile*`.

## Testing

- keys: `ProfileId::new("")` fails; a key file without `profile` is skipped and its token
  refused; `add` stores the profile and `info` carries it.
- CLI: `add --profile` with a known id prints a token; with an unknown id fails and lists
  the known ids; `list` shows labels and `(unknown)`.
- axum: 401 for no or a wrong token; 403 when `resolve` returns `None`; 200 and the
  handler sees `Authenticated<P>` when it returns `Some`; `resolve` is called again on the
  next request (no caching), so a profile removed between two requests turns 200 into 403.
- gui: `LocalKeys` answers `Profiles`; `Add` with an unknown profile is `Failed`; the page
  keeps Add disabled until a profile is chosen; an unknown profile shows as
  `<id> (unknown)`.

## Changes elsewhere

- `2026-10-31-api-keys-design.md`: point its `subject` and its out-of-scope "per-key scopes"
  at this spec.
- The keys README, the top README and `corundum-framework/src/lib.rs`: the new `add`,
  `run` and `require_key` signatures.
- Released as a breaking change, `feat(keys)!`.

## Out of scope

- Changing a key's profile.
- Scopes, roles or default profiles in corundum.
- Caching the resolver's answers.
- A page per profile, or filtering the page by profile.
