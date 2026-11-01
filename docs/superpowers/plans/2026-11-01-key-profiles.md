# Key Profiles Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every API key names exactly one app-defined profile; the command line and the Settings page choose it from a list the app gives, and `require_key` lets a request through only once the app has resolved that profile.

**Architecture:** A new `profile.rs` in `corundum-framework-keys` holds `ProfileId`, `Profile` and `check_profile`. `Key.subject` becomes `Key.profile`. `KeyCommand::run` and `LocalKeys::new` take the app's profiles. `require_key` takes a resolver and puts `Authenticated<P>` in the request's extensions, answering 403 when the resolver returns `None`. The page gains a `Profiles` request/answer pair and a required profile combo box.

**Tech Stack:** Rust 2024, serde/toml, thiserror, clap, axum 0.8, egui (corundum-framework-gui), Fluent strings.

**Spec:** `docs/superpowers/specs/2026-11-01-key-profiles-design.md`

## Global Constraints

- A `ProfileId` is any non-empty string; corundum never interprets it.
- `Profile.label` is for display only and is never written to a key file.
- A key file without `profile`, or with `profile = ""`, does not parse, so it is skipped when read and its token is refused. No migration code.
- A key's profile never changes after `add` (no `set-profile`).
- `require_key`'s resolver is required, runs inside the same `spawn_blocking` as the token check, and is called on every request (its result is never cached). `CACHE_TTL` for the key check is unchanged.
- 401 for no or wrong token, 403 when the resolver returns `None`, a `warn` log with the key id and profile id on 403.
- No scopes, roles or default profiles in corundum.
- Unknown profile shows as `<id> (unknown)` (en) / `<id>（不明）` (ja).
- Released as a breaking change: commit type `feat(keys)!` on the first breaking commit.
- Every commit message ends with the line `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- CI builds with `--all-features`; run `cargo test --workspace --all-features` and `cargo clippy --workspace --all-features --all-targets -- -D warnings` before the last commit.

## Review Focus

1. A key file in the old format (`subject` but no `profile`): it must be skipped and its token refused, not accepted with an empty profile. Covered in Task 1.
2. A key file with `profile = ""`: it must be skipped, not accepted. Covered in Task 1.
3. A resolver that panics: the request must get a 500, never reach the handler. Covered in Task 3.
4. A profile that goes away between the page listing it and the user pressing Add: `LocalKeys` must answer `Failed`, not write the key. Covered in Task 4.
5. A selected profile that disappears from a refreshed `Profiles` answer: the page must clear the selection so Add is disabled again. Covered in Task 4.

---

### Task 1: `ProfileId`, `Profile` and a required `Key.profile`

**Files:**
- Create: `crates/corundum-framework-keys/src/profile.rs`
- Modify: `crates/corundum-framework-keys/src/key.rs` (`Key`, `Key::info`, `KeyInfo`)
- Modify: `crates/corundum-framework-keys/src/error.rs`
- Modify: `crates/corundum-framework-keys/src/store.rs` (`add` and its tests)
- Modify: `crates/corundum-framework-keys/src/lib.rs` (`mod profile; pub use profile::{Profile, ProfileId, check_profile};`)
- Modify (call sites only, to keep the crate compiling): `src/cli.rs`, `src/axum.rs`, `src/verify.rs` tests

**Interfaces:**
- Produces:
  - `pub struct ProfileId(String)`: `#[serde(transparent)]` `Serialize`; `Deserialize` that validates through `new`; derives `Clone, PartialEq, Eq, Hash, PartialOrd, Ord`; `Display` writes the string; `Debug` writes `ProfileId(<s>)`.
  - `ProfileId::new(s: &str) -> Result<ProfileId>` (`Error::InvalidProfile(s)` if empty), `ProfileId::as_str(&self) -> &str`.
  - `#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)] pub struct Profile { pub id: ProfileId, pub label: String }`.
  - `pub fn check_profile(profiles: &[Profile], id: &ProfileId) -> Result<()>`: `Ok` if some profile has that id, else `Error::UnknownProfile { id: id.to_string(), known: <every id in profiles, in order> }`.
  - `Error::InvalidProfile(String)`, message `"a profile id cannot be empty"`.
  - `Error::UnknownProfile { id: String, known: Vec<ProfileId> }`, message `"{id:?} is not a profile; choose one of: a, b"` (known ids joined by `", "`; `"(none)"` when empty).
  - `Key.profile: ProfileId` and `KeyInfo.profile: ProfileId` (both required, no `serde(default)`), replacing `subject`.
  - `KeyStore::add(&mut self, name: &str, description: Option<String>, profile: ProfileId) -> Result<(Key, Token)>`.
  - Test helper used by later tasks' tests: none; they write `ProfileId::new("admin").unwrap()`.

- [ ] **Step 1: Write the failing tests**

In `profile.rs`:
- `empty_id_is_invalid`: `ProfileId::new("")` is `Err(Error::InvalidProfile(_))`; `ProfileId::new("room-1").unwrap().as_str() == "room-1"`.
- `deserialize_validates`: `serde_json::from_str::<ProfileId>("\"\"")` is `Err`; `"\"x\""` is `Ok`.
- `check_profile_lists_known_ids`: with profiles `admin`, `editor`, `check_profile(&p, &id("admin"))` is `Ok`; `check_profile(&p, &id("x"))` is `Err(UnknownProfile { id, known })` with `id == "x"`, `known == [admin, editor]`, and its `to_string()` contains `"admin, editor"`.

In `store.rs` tests:
- `add_stores_the_profile`: `add("p", None, id("editor"))`, then a fresh `KeyStore::open` lists one key with `profile.as_str() == "editor"` and `info().profile` equal to it; the file text contains `profile = "editor"`.
- `file_without_profile_is_skipped`: write a valid key with `add`, rewrite its file with the `profile = ...` line removed (and a `subject = "x"` line added, the old format); a fresh store lists no keys and `authenticate` of its token is `None`.
- `file_with_empty_profile_is_skipped`: same, with the line replaced by `profile = ""`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p corundum-framework-keys --all-features`
Expected: compile errors (`ProfileId` not found, `add` takes `Option<String>`).

- [ ] **Step 3: Implement**

`profile.rs` as in Interfaces, with the same style as `KeyId` in `key.rs`. Replace `subject` with `profile` in `Key`, `Key::info` and `KeyInfo` (doc: "The profile the key acts as; the app decides what it may do."). Change `add`'s parameter. Update every existing call `add(.., None)` in `store.rs`, `cli.rs`, `axum.rs`, `verify.rs` tests to pass `ProfileId::new("p").unwrap()`. The `import` test builds its `Key` with `profile: ProfileId::new("p").unwrap()`. In `cli.rs`, for now, rename the `subject: Option<String>` arg to `#[arg(long)] profile: String` and pass `ProfileId::new(&profile)?`; Task 2 adds the list.

The gui crate stops compiling here; Task 4 fixes it. Until then, run tests with `-p corundum-framework-keys`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p corundum-framework-keys --all-features`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/corundum-framework-keys
git commit -m "feat(keys)!: every key names a profile (#85)"
```

---

### Task 2: The command line chooses from the app's profiles

**Files:**
- Modify: `crates/corundum-framework-keys/src/cli.rs`

**Interfaces:**
- Consumes: `Profile`, `ProfileId`, `check_profile` (Task 1).
- Produces:
  - `KeyCommand::run(self, store: &mut KeyStore, profiles: &[Profile]) -> Result<i32>`
  - `KeyCommand::run_to(self, store: &mut KeyStore, profiles: &[Profile], out: &mut dyn Write, err: &mut dyn Write) -> Result<i32>`
  - `Add { name, description, #[arg(long)] profile: String }` (doc: "The profile the key acts as; one the app offers.")

- [ ] **Step 1: Write the failing tests**

The test helper `run` takes `profiles: &[Profile]`; a `profiles()` helper returns `admin` ("Admin") and `editor` ("Editor"); `add(name)` uses `profile: "admin"`.
- `add_with_unknown_profile_fails`: `add` with `profile: "nope"` returns `Err(Error::UnknownProfile { .. })` whose message contains `"admin, editor"`; `out` is empty and the store lists no keys.
- `add_with_empty_profile_fails`: `profile: ""` returns `Err(Error::InvalidProfile(_))`.
- `list_shows_profile_label_or_unknown`: add `a` with `admin` directly through the store and `b` with `ProfileId::new("gone")`; `List { all: false }` with `profiles()` prints a line for `a` containing `"Admin"` and a line for `b` containing `"gone (unknown)"`.
- Existing tests keep passing with the new helper.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p corundum-framework-keys --all-features cli::`
Expected: compile errors (`run_to` takes 3 arguments).

- [ ] **Step 3: Implement**

`Add` does `ProfileId::new(&profile)?`, then `check_profile(profiles, &id)?`, then `store.add`. `line(key, profiles)` becomes `id  name  profile  created  [retired]`, where profile is the matching label or `"<id> (unknown)"`. Update the doc comment on `run_to` (`token=$(app api-key add pendant --profile admin)`).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p corundum-framework-keys --all-features`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/corundum-framework-keys/src/cli.rs
git commit -m "feat(keys): keys add chooses from the app's profiles (#85)"
```

---

### Task 3: `require_key` resolves the profile

**Files:**
- Modify: `crates/corundum-framework-keys/src/axum.rs`

**Interfaces:**
- Consumes: `KeyInfo.profile` (Task 1).
- Produces:
  - `pub fn require_key<P, F>(verifier: Arc<KeyVerifier>, resolve: F, router: Router) -> Router where P: Clone + Send + Sync + 'static, F: Fn(&KeyInfo) -> Option<P> + Send + Sync + 'static`
  - `#[derive(Clone, Debug)] pub struct Authenticated<P> { pub key: KeyInfo, pub profile: P }` (doc: "The key a request was let in with, and its resolved profile; in the request's extensions.")

- [ ] **Step 1: Write the failing tests**

The test `app` takes a resolver; the handler is `get(|Extension(who): Extension<Authenticated<String>>| async move { format!("{} {}", who.key.name, who.profile) })`. A resolver over `Arc<Mutex<HashSet<String>>>` returns `Some(id.to_owned())` when the set holds the key's profile id.
- Existing 401 tests keep passing (with a resolver that would accept anything).
- `good_token_reaches_handler`: key `pendant` with profile `admin`, set `{admin}` → 200, body `"pendant admin"`.
- `unresolved_profile_is_403`: key with profile `gone`, set `{admin}` → 403, and no `WWW-Authenticate` header.
- `resolver_is_not_cached`: set `{admin}`; first request 200; remove `admin` from the set; second request with the same token (same router) → 403.
- `panicking_resolver_is_500`: resolver `|_| -> Option<String> { panic!() }` → 500.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p corundum-framework-keys --all-features axum::`
Expected: compile errors (`require_key` takes 2 arguments).

- [ ] **Step 3: Implement**

State is `Arc<Gate<F>>` with `struct Gate<F> { verifier: Arc<KeyVerifier>, resolve: F }`. In `check`, the `spawn_blocking` closure verifies the token and, on `Accepted(info)`, calls `resolve(&info)`, returning a three-way outcome (refused / unresolved(info) / resolved(info, p)). Unresolved: `tracing::warn!(key = %info.id, profile = %info.profile, "the key's profile is not known; refused")` and plain `StatusCode::FORBIDDEN`. A `JoinError` (a panic in either) stays the existing 500 path. Update the module and `require_key` docs to describe 401/403.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p corundum-framework-keys --all-features`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/corundum-framework-keys/src/axum.rs
git commit -m "feat(keys)!: require_key resolves the key's profile, 403 if it cannot (#85)"
```

---

### Task 4: The Settings page chooses a profile

**Files:**
- Modify: `crates/corundum-framework-gui/src/keys/mod.rs` (`KeysRequest`, `KeysAnswer`, `LocalKeys`, re-exports, tests)
- Modify: `crates/corundum-framework-gui/src/keys/page.rs`
- Modify: `crates/corundum-framework-gui/locales/en/corundum.ftl`, `crates/corundum-framework-gui/locales/ja/corundum.ftl`

**Interfaces:**
- Consumes: `Profile`, `ProfileId`, `check_profile`, `KeyStore::add(.., ProfileId)` (Task 1).
- Produces:
  - `pub use corundum_framework_keys::{KeyId, KeyInfo, KeyStore, Profile, ProfileId, Token};`
  - `KeysRequest::Profiles`; `KeysRequest::Add { name: String, description: Option<String>, profile: ProfileId }`
  - `KeysAnswer::Profiles(Vec<Profile>)`
  - `LocalKeys::new(store: KeyStore, profiles: impl Fn() -> Vec<Profile> + 'static) -> LocalKeys` (stored as `Box<dyn Fn() -> Vec<Profile>>`)
  - `KeysPage`: `pub(crate) fn can_add(&self) -> bool` (name non-empty after trim and a profile selected); `fn profile_label<'a>(profiles: &'a [Profile], id: &ProfileId) -> Option<&'a str>`

Strings (exact):

| key | en | ja |
|---|---|---|
| `corundum-keys-profile` | `Profile` | `プロファイル` |
| `corundum-keys-profile-choose` | `Choose a profile` | `プロファイルを選択` |
| `corundum-keys-profile-none` | `There are no profiles to choose from.` | `選べるプロファイルがありません。` |
| `corundum-keys-profile-unknown` | `{ $id } (unknown)` | `{ $id }（不明）` |

- [ ] **Step 1: Write the failing tests**

In `mod.rs` (`LocalKeys` over profiles `admin` / `Admin`; for the vanishing case, profiles read from an `Rc<RefCell<Vec<Profile>>>`):
- `local_keys_round_trip`: `Add { profile: admin }` gives `WithToken` with `info.profile == admin`; rest as today.
- `local_keys_answers_profiles`: `send(Profiles)` → `[KeysAnswer::Profiles(v)]` with `v == [admin]`.
- `local_keys_refuses_unknown_profile`: `Add { profile: "nope" }` → `[Failed(_)]` and a fresh store lists no keys.
- `profile_removed_before_add_fails`: the shared list holds `admin`; send `Profiles`; empty the list; `Add { profile: admin }` → `[Failed(_)]`.

In `page.rs` (the `key()` helper adds with `ProfileId::new("admin")`):
- `first_frame_asks_for_the_list`: sent is `[List, Profiles]`.
- `token_is_shown_once`: after the change, sent is `[List, Profiles, List, Profiles]`.
- `add_needs_a_name_and_a_profile`: with name `"p"` and no profile, `!can_add()`; after a `Profiles([admin])` answer and selecting `admin`, `can_add()`; with name `"  "`, `!can_add()`.
- `vanished_profile_is_deselected`: select `admin`, then a `Profiles([editor])` answer → selection is `None`, `!can_add()`.
- `profile_label_or_none`: `profile_label(&[admin], &admin) == Some("Admin")`; for `gone`, `None`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p corundum-framework-gui --all-features keys::`
Expected: compile errors.

- [ ] **Step 3: Implement**

`LocalKeys`: `Profiles` → `Profiles((self.profiles)())`; `Add` → `check_profile(&(self.profiles)(), &profile)` then `store.add`. Update the module doc (the app gives the profiles).

`KeysPage` gains `profiles: Vec<Profile>` and `profile: Option<ProfileId>`. Wherever the page sends `List` (first frame, after `Changed`, after `WithToken`) it also sends `Profiles`. On `Profiles(v)`, store `v` and clear `profile` if it is no longer among `v`. The add grid gets a third row, `corundum-keys-profile`: an `egui::ComboBox` (id `corundum_keys_profile`) whose selected text is the chosen label or `corundum-keys-profile-choose`, one `selectable_value` per profile by label; or, if `profiles` is empty, a weak `corundum-keys-profile-none`. The Add button uses `can_add()` and sends `profile` with the request; after sending, clear name, description and the selection. Each list row shows, after the name, the profile label or `tr!(ui, "corundum-keys-profile-unknown", id = key.profile.as_str())`. Update the struct doc ("a form to add one, with its profile").

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p corundum-framework-gui --all-features`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/corundum-framework-gui
git commit -m "feat(gui)!: the keys page chooses each key's profile (#85)"
```

---

### Task 5: Docs, and the whole workspace

**Files:**
- Modify: `crates/corundum-framework-keys/README.md` (the example: `add("pendant", None, ProfileId::new("admin")?)`, and a short paragraph plus a `require_key(verifier, |k| profiles.get(&k.profile), router)`-shaped example of resolving a profile; mention 401/403)
- Modify: `crates/corundum-framework-keys/src/lib.rs` (crate doc: keys name a profile; the axum line mentions the resolver; point at the new spec too)
- Modify: `README.md` lines for `keys`, `keys-axum`, `keys-clap` (the page chooses a profile from the app's list; the layer resolves each key's profile)
- Modify: `crates/corundum-framework/src/lib.rs:26-29` (same wording as the README)
- Modify: `docs/superpowers/specs/2026-10-31-api-keys-design.md` (at `subject` and the out-of-scope "Per-key scopes": "Replaced by `2026-11-01-key-profiles-design.md`.")

- [ ] **Step 1: Make the doc changes above**

- [ ] **Step 2: Check the whole workspace**

Run: `cargo test --workspace --all-features && cargo clippy --workspace --all-features --all-targets -- -D warnings && cargo doc --workspace --all-features --no-deps`
Expected: tests pass, no clippy warnings, no rustdoc warnings about broken links.

Run: `git grep -n 'subject' -- crates README.md`
Expected: no hits about keys.

- [ ] **Step 3: Commit**

```bash
git add README.md crates docs
git commit -m "docs: keys name a profile (#85)"
```
