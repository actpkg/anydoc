---
name: anydoc
description: Convert Word, PowerPoint, Excel, OpenDocument, RTF, EPUB, CSV and PDF documents to GitHub-Flavored Markdown
metadata:
  act: {}
---

# anydoc

Converts documents to GitHub-Flavored Markdown. Wraps
[`anydoc`](https://github.com/firecrawl/anydoc).

Handles Word (`.doc`, `.docx`, `.docm`), PowerPoint (`.ppt`, `.pptx` and
variants), Excel (`.xls`, `.xlsx`, `.xlsm`, `.xlsb`), OpenDocument (`.odt`,
`.ods`, `.odp`), RTF, EPUB, CSV and PDF.

## Supplying the document

Every tool takes exactly one of:

- `data` — the bytes inline, as `{"$bytes": "<base64>"}` over JSON.
- `path` — a file on the host. **Needs a `wasi:filesystem` read grant**
  covering that path, otherwise the call is denied.

Passing both, or neither, is an error.

Every tool also takes an optional `filename`: a fallback for format
detection when the content carries no signature. Only the extension is
used, content always wins over it, and an explicit `format` (on `convert`
and `extract_assets`) short-circuits both `filename` and detection
entirely. `filename` is the only way to get `detected_from: "extension"`
on an inline `data` payload — see `detect` below.

## Tools

### `convert`

Document → Markdown.

```json
{"data": {"$bytes": "UEsDBBQ..."}, "filename": "report.docx"}
```

Returns `{markdown, format}`. The format is detected from the content,
falling back to `filename`'s extension, then failing.

**CSV is the exception**: it carries no signature, so it cannot be detected
and you must pass `format: "csv"` or a `filename` ending in `.csv`. Same for
any file whose container we cannot recognise.

`format` is a caller assertion — it overrides detection entirely. Pass it when
you know; leave it off when you don't, rather than guessing.

### `detect`

Format identification without conversion. Much cheaper than `convert` — use it
to triage before committing to a large document.

Returns `{format, detected_from}`. `detected_from` is `content` when a
signature identified the file, `extension` when only `filename` did.

**An `extension` result on a format that should carry a signature means the
file is not what its name claims.** Treat that as a red flag.

### `extract_assets`

Pulls the embedded images and objects out of a document — the thing Markdown
alone cannot give you.

Emits a manifest first (`{assets: [{id, media_type, origin_part, byte_len}]}`),
then the bytes of each asset as an image content part. Call once to see the
manifest, then again with `ids: [0, 3]` to fetch only what you need, rather
than pulling every asset in a large deck. Takes the same `format` override as
`convert`. An `ids` value that matches nothing in the document is not an
error — it returns the manifest with no content parts, so check the manifest
if that's unexpected.

**Not supported for PDF** — use the `pdf-inspector` component for that.

## When to use `pdf-inspector` instead

For PDFs, this component just converts. If you need to know *what kind* of PDF
you have — text-based vs scanned, which pages need OCR, layout complexity — or
you need page selection or a password, use `pdf-inspector`.

## Errors

| kind | meaning |
|---|---|
| `std:invalid-args` | unreadable, unrecognised, encrypted, or a bad argument |
| `anydoc:resource-limit` | the document crossed a fixed safety limit — a decompression bomb or runaway expansion. The message names the limit. |
| `std:not-found` | no file at `path` |
| `std:capability-denied` | `path` used without a `wasi:filesystem` read grant |

## Limits

Fixed and not configurable: 128 MiB per archive entry, 512 MiB per archive,
100k entries, XML nesting depth 256, 2M XML nodes, 4M cells per table
expansion, 64 MiB of text duplicated by table expansion, 128 MiB of retained
assets, binary-record nesting depth 64, 16M binary records per legacy
stream. Real documents sit far below all of these.

Conversion is not lossy-free: complex layouts flatten to linear Markdown, and
scanned/image-only PDFs are refused rather than OCR'd.
