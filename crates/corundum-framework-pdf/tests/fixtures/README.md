# PDF test fixtures

Six small PDFs for the `corundum-framework-pdf` tests. Each is under 20 KB. All were made
for this crate with a throwaway generator (not in the repo), so there is no third-party
document in here; the only third-party material is the font subset in `ja-embedded.pdf`.

| File | What it is | How it was made |
| --- | --- | --- |
| `ja-embedded.pdf` | One page (420 x 200 pt), four lines of Japanese ("旅行のチケット" and three more), Noto Sans JP embedded as a subset (CID TrueType/CFF subset written by krilla). | `krilla` 0.8 (MIT OR Apache-2.0), `Document::start_page_with` and `Surface::draw_text`, with the Noto Sans JP variable font pinned to `wght=400`. |
| `ja-not-embedded.pdf` | The same text as a Type0 font with a `CIDFontType0` descendant, `CIDSystemInfo` Adobe-Japan1, `/Encoding /90ms-RKSJ-H`, and a `FontDescriptor` with no `FontFile`. | Hand-written PDF: text is Shift-JIS, hex-encoded in the content stream. |
| `scan.pdf` | One A4 page (595 x 842 pt) holding only a 320 x 452 grayscale JPEG (DCTDecode), a synthetic "scanned page" of grey bars. | Hand-written PDF around a JPEG made with the `image` crate. |
| `encrypted.pdf` | One page (420 x 200 pt) of Helvetica text. Standard security handler, V2 / R3, RC4 128-bit, empty user password, owner password "owner". | Hand-written PDF; the key, `/O`, `/U` and the content-stream encryption follow ISO 32000-1 algorithms 2, 3, 5 and 1. |
| `password.pdf` | One page (420 x 200 pt) of Helvetica text. Same handler (V2 / R3, RC4 128-bit) but with a non-empty user password, "secret" (owner password "owner"). Cannot be opened without the password. | Same generator as `encrypted.pdf`, with the padded user password in the key and `/O`/`/U` computation. |
| `two-pages.pdf` | Page 1 A4 (595 x 842 pt), page 2 US Letter (612 x 792 pt); each has a frame and text in the standard-14 font Helvetica (not embedded). | Hand-written PDF. |

## Licences

- The generated structure, text and image are original to this repository and fall under the
  repository's own licence.
- The glyph outlines in `ja-embedded.pdf` come from **Noto Sans JP**, licensed under the
  **SIL Open Font License, Version 1.1** (<https://openfontlicense.org>, full text at
  <https://github.com/google/fonts/blob/main/ofl/notosansjp/OFL.txt>).
  Copyright 2014-2021 Adobe (<http://www.adobe.com/>), with Reserved Font Name 'Source'.
  Source of the font file: <https://github.com/google/fonts/tree/main/ofl/notosansjp>
  (`NotoSansJP[wght].ttf`). The OFL allows embedding a subset in a document; the font is not
  sold on its own and its name is not used for a modified font.
- `krilla` (MIT OR Apache-2.0) and `hayro` (MIT OR Apache-2.0) were only used to make and check
  the files; nothing of them is in the fixtures beyond ordinary PDF syntax.
