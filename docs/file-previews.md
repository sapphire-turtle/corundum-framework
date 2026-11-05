# Why corundum previews files its apps were not made for

> Language: **English** | [日本語](file-previews.ja.md)

corundum's file explorer and detail pane show and play files that the app around them
has no use for: an asteria-music library shows a JPEG, a sapphire-sync folder plays a
FLAC, and later a PDF, a plain-text file or a Markdown note. Judged by each app's purpose
alone this is unreasonable — a music player has no business rendering photos — and every
format adds code and dependencies to a framework that would otherwise stay small. This
page records why corundum does it anyway, so that the question need not be settled again
each time a format is added.

## The reason: the apps' data is an ordinary folder

The apps built on corundum keep no database of their own as the source of truth. Their
data is a folder the person chose, with plain files in it:

- asteria's library is a directory of photos or music, with sidecars, playlists and
  covers beside them;
- sapphire's workspaces (sapphire-sync and the others) are directories of the person's
  own files, synced between devices.

Nothing hides that folder, and nothing stops other files from living in it. A music
folder holds the scan of the booklet, the album's cue sheet and a `notes.txt`; a photo
folder holds a PDF of the trip's tickets; a synced folder holds whatever the person put
there. **That is normal use, not a mistake to be corrected.** The apps list such files
next to their own rather than hiding them, because hiding them would make the folder in
the app disagree with the folder on disk, and a person who moves, renames or deletes
files in the app must see what they are moving.

Once a file is listed, "what is this?" is the first thing a person asks of it. The
detail pane is where that question is answered, so it answers it for every file it
reasonably can, not only for the formats the app was made for.

## What this allows, and its limits

- **Previews of more formats are welcome**: images and audio first (asteria needs them
  anyway), then documents such as PDF, plain text and Markdown, as far as Rust crates make
  them practical.
- **A preview is a look, not an editor.** Showing, playing and scrolling belong here;
  editing, annotating and converting do not — that is the app's (or another app's) job.
- **Each format is behind a feature**, off by default, so an app that does not want it
  builds without it, and the default build of corundum stays as small as before.
- **A format that would need a heavy or awkward dependency waits.** Pure-Rust decoders are
  preferred; a bundled C library (a `-sys` crate) is acceptable behind its feature when no
  good pure-Rust decoder exists; a dependency that must be installed on the system, or
  whose licence would bind every app, is not taken in.
- **Unknown or unsupported files still get an answer**: their name, size, kind and dates,
  and an "open with the system's app" action — never an empty pane.

## Where this is used

- [Images and audio](superpowers/specs/2026-11-02-media-image-audio-design.md):
  `corundum-framework-image`, `corundum-framework-audio`, and the `image-preview` /
  `audio-preview` widgets.
- [Detail-pane previews over content the app hands in](superpowers/specs/2026-11-05-detail-pane-previews-design.md):
  the app hands in bytes or a reader, wherever the file lives; `FilePreview` and `InfoCard`
  need no feature; the sound plays through a `Player<Content>` spawned with `ContentOpener`.
- [Text, Markdown, PDF and date-time](superpowers/specs/2026-11-05-text-markdown-pdf-datetime-design.md):
  `text-preview` shows a text file read-only, read as UTF-8 or UTF-16 (with a BOM); `FilePreview` uses it for
  the text extensions. `markdown-preview` shows a Markdown file formatted and read-only, and reports a clicked link in
  `PreviewResponse::link` for the app to open or not; `FilePreview` uses it for `.md` and `.markdown`.
