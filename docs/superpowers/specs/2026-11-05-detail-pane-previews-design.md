# The detail pane's preview: parts that draw content the app hands in

Asked for on 2026-11-05, in issue #97, for the sapphire apps and asteria.

## Why

`ImagePreview` and `AudioPreview` exist, but nothing chooses between them. Each app that
shows a detail pane has to write the same branch: "an image? a sound? else the name, size
and dates". Apps also need to put their own view over that default, or in its place, for
the files they manage.

The person should see what an ordinary file explorer shows: a picture or a play button at
the top, and the file's metadata under it. That is the **default**. An app replaces it for
its own files. asteria-photos shows a photo with its tags and other sidecar data;
sapphire-journal shows a Markdown note in the journal's own view. For those files, the
file-system metadata (name, size, dates) is dropped or moved to the end.

The apps get their files in different ways:

- **sapphire** previews local files.
- **asteria** previews files on its server. Even a local server is reached through the
  server.

So **fetching belongs to the apps, and corundum only draws.** The app gets the content and
hands it to corundum without saying where it came from. corundum decodes the content off
the UI thread and draws it. Today's parts read local paths themselves; they stop doing
that.

Why corundum previews files its apps were not made for at all is in
[file-previews.md](../../file-previews.md).

## Goals

- **Content that does not depend on location.** The app hands a preview one of two things:
  - **bytes** (`Arc<[u8]>`, or similar);
  - **a reader opener**: something corundum can call, more than once, to get a fresh
    `Read + Seek + Send` source. A local app hands in an opener of the `File`, so a long
    sound is not loaded into memory.

  Along with the content, the app gives the file's name or extension and a **key**. The key
  names the content and changes when the content changes, so corundum knows when to decode
  again. The app decides when a file has changed; corundum no longer stats paths.
- **Loading and failure are drawn by corundum and decided by the app.** For a file, the
  app hands in one of three states:
  - *loading*: corundum shows a spinner;
  - *failed, with a reason*: corundum shows the reason and a Retry button, and reports a
    click on Retry back to the app;
  - *ready, with the content*: corundum shows the preview.

  The look is the same in every app.
- **`FilePreview`** (name open): the default for one file. It chooses a part by the file's
  extension and the enabled features:
  - an image goes to `ImagePreview` (`image-preview`);
  - a sound goes to `AudioPreview` (`audio-preview`);
  - anything else gets the info card.

  It draws the preview at the top and the file's metadata under it.
- **The info card** (name open), a part of its own. It shows the icon, name, size, kind
  and dates, from what the app knows (the explorer's `Entry` has these). It has an
  optional "Open with the system's app" button. The card only shows the button when the
  app asks for it, and reports the click back to the app. sapphire asks for it; asteria
  does not (decided by the person). It can also be drawn as a smaller **footer**, for an
  app's own view to end with.
- **`ImagePreview` and `AudioPreview`** take the same content and states, and stay usable
  on their own.
- **No hook trait.** The app composes the parts:
  - **Default:** draw `FilePreview`.
  - **Replace:** draw the app's own view, and the info card's footer if the app wants the
    metadata at the end.
  - **Add:** draw the app's section above or below `FilePreview`.

  The demo and the module doc show all three.

## Non-goals

- **Fetching, reading files and watching for changes.** These are the apps' job.
- **Text and Markdown previews.** These are a separate issue. They will be drawn with
  alumina-code and alumina-markdown (sapphire-turtle/alumina, which depends only on egui),
  behind a feature. This design only leaves the dispatcher room to add them.
- **asteria's managed media.** asteria-music and asteria-photos show their own files through
  Replace. The default is for the few files the apps do not manage: an album's cover in
  `$HOME/Music`, the script that comes with an audio drama, and the like.
- **Editing anything.** A preview is a look (file-previews.md).
- **Changes to `FileExplorer`.** It keeps returning `ExplorerEvent`s. The app keeps
  deciding what is selected, and it gets and hands in the content.

## Anti-goals

- **The UI thread waiting on decoding, reading tags, or a slow reader.** All of these
  happen on corundum's workers. A reader opener is only ever called on a worker, because
  it may touch a network mount.
- **Loading a long local sound whole.** This is why the reader opener exists.
- **An app rebuilding the default** to add a section, or to get the metadata footer.
- **The default build growing.** Everything that needs a dependency stays behind the
  existing features.

## Decisions

1. **Location-agnostic content: bytes, or a reader opener** (decided by the person). There
   are no path or fetcher types in the preview API.
2. **The app decides when content has changed, and says so through the key.** This replaces
   `ImagePreview`'s two-second re-stat of the path.
3. **The parts' APIs may change incompatibly** (decided by the person). This includes:
   - `ImagePreview::ui`;
   - `AudioPreview::ui`;
   - the `Player` item type the preview uses, and `FileOpener`.

   No app outside corundum uses them yet. corundum's own callers, tests, README, docs and
   CHANGELOG are updated with them. The app still holds **one** `Player` for all its
   previews (from the media design). Its item type must be able to carry the content that
   was handed in, so bytes and reader openers both play.
4. **Images** decode from bytes with `corundum_framework_image::Source::Bytes`, which
   exists. From a reader opener, corundum may read the file whole on the worker, since
   pictures are small, or add a reader source to the image crate. That is the builder's
   call.
5. **A broken or unsupported image or sound** shows the part's own message ("not an image",
   "not audio", or the error), with the metadata under it as usual. It does not switch to
   the info card, so the parts need no new public status for this. *(Builder's call, made
   while writing this up.)*
6. **Retry and "Open with the system's app" are reported back to the app**, through a
   returned response or event. corundum does not refetch anything or open anything itself.
   A small helper that opens a local path with the system's app may be added for sapphire's
   convenience, but it is optional and separate from the card.
7. **Tests draw frames without a window**, with egui_kittest as the preview tests do now.
   They cover:
   - `FilePreview` choosing image, audio and info card;
   - the loading, failed (with Retry reported) and ready states;
   - bytes and reader openers, for both an image and a sound;
   - a change of key causing a new decode;
   - the info card with and without "Open with", and its footer form;
   - Replace and Add composed from the parts.
8. **The demo**, on its Files screen:
   - **Detail pane:** it gains a detail pane driven by the selection, which hands local
     files in through reader openers.
   - **App's own view:** it shows one view of its own for one kind of file, for example a
     `.txt` "notes" view with the info card's footer, next to the default for the rest.
   - **Previews:** it turns on `image-preview` and `audio-preview` (decided by the person)
     and holds one `Player`.
   - **Samples:** it adds a sample picture and a short sound to `make_sample`.

   How many formats the demo covers does not matter, but it must still build for iOS and
   Android. If a part of `audio-preview` does not build for a phone target, the demo turns
   that feature on only for the targets where it builds, instead of dropping it everywhere.
9. **Docs:** these are updated:
   - the `preview` module doc, with the three compositions;
   - the features' comments in `Cargo.toml`;
   - a line in file-previews.md's "Where this is used" that points here.

   Strings go through `tr!` in the existing Fluent files (English and Japanese).

## Left to the builder

- **Names and exact signatures:** of `FilePreview`, the info card, the content and state
  types, and the response that reports Retry and Open.
- **The reader opener's form:** a trait or a boxed closure.
- **How kept decodes are bounded:** for example, the last few keys, as now.
- **How the `Player` item type carries content:** see decision 3.
- **The kind label on the card:** taken from the extension, from a short localised list.
- **Whether to add the open-with helper,** and where.
