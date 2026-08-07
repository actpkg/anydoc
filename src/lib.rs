//! Document conversion to GitHub-Flavored Markdown, wrapping the
//! [`anydoc`](https://github.com/firecrawl/anydoc) crate.
//!
//! The component runs legacy OLE/CFB binary parsers, ZIP+XML package readers
//! and an RTF parser over untrusted input — a classic memory-corruption and
//! decompression-bomb surface. Its declared ceiling is read-only
//! `wasi:filesystem` and nothing else, so a weaponised document cannot reach
//! the network or write to disk no matter what it does to the parser.

// Host-compilable: no SDK, no wit-bindgen. `cargo test` on the host builds
// only these. Any type mentioning `Bytes` or `ActResult` must stay out.
pub mod error;
pub mod format;

#[cfg(test)]
pub(crate) mod testzip;

#[cfg(target_family = "wasm")]
use act_sdk::prelude::*;
// The prelude re-exports `Deserialize` (tool args) but not `Serialize`
// (tool results).
#[cfg(target_family = "wasm")]
use serde::Serialize;

#[cfg(target_family = "wasm")]
use crate::format::{DetectedFrom, Format};

// ── Input ────────────────────────────────────────────────────────────

/// Where the document bytes come from. Supply exactly one of `data` or `path`.
//
// Kept as two optional fields rather than an untagged enum because a
// flattened `#[serde(untagged)]` enum contributes no properties to the
// generated JSON Schema — the source fields would be invisible to any agent
// reading the tool catalogue. The "exactly one" invariant is enforced in
// `Source::read`.
#[cfg(target_family = "wasm")]
#[derive(Deserialize, JsonSchema)]
struct Source {
    /// Inline document bytes, as a CBOR byte string — or the canonical
    /// `{"$bytes": "<base64>"}` envelope over JSON transports.
    data: Option<Bytes>,
    /// Path to a document on the host. Requires a `wasi:filesystem` read
    /// grant covering this path.
    path: Option<String>,
}

#[cfg(target_family = "wasm")]
impl Source {
    /// Read the bytes, and return the source path alongside them when there
    /// was one — its extension is the last-resort format hint, matching what
    /// upstream `anydoc::to_markdown` does for a path input.
    fn read(self) -> ActResult<(Vec<u8>, Option<String>)> {
        match (self.data, self.path) {
            (Some(_), Some(_)) => Err(ActError::invalid_args(
                "provide either `data` or `path`, not both",
            )),
            (None, None) => Err(ActError::invalid_args(
                "provide the document as `data` (bytes) or `path` (a file on the host)",
            )),
            (Some(data), None) => Ok((data.into(), None)),
            (None, Some(path)) => match std::fs::read(&path) {
                Ok(bytes) => Ok((bytes, Some(path))),
                Err(e) => {
                    let (kind, message) = crate::error::classify_io(&e, &path);
                    Err(ActError::new(kind, message))
                }
            },
        }
    }
}

/// Document to convert.
#[cfg(target_family = "wasm")]
#[derive(Deserialize, JsonSchema)]
struct ConvertArgs {
    #[serde(flatten)]
    src: Source,
    /// Parser to use. Omit to detect from the content. Required for CSV,
    /// which carries no signature. An explicit value overrides detection.
    format: Option<Format>,
    /// Filename to fall back on for format detection when the content
    /// carries no signature. Only the extension is used; content always
    /// wins over this, and it is ignored when `format` is given.
    filename: Option<String>,
}

/// Document to identify.
#[cfg(target_family = "wasm")]
#[derive(Deserialize, JsonSchema)]
struct DetectArgs {
    #[serde(flatten)]
    src: Source,
    /// Filename to fall back on when the content carries no signature.
    /// Only the extension is used. Content always wins over this.
    filename: Option<String>,
}

/// Document to extract embedded assets from.
#[cfg(target_family = "wasm")]
#[derive(Deserialize, JsonSchema)]
struct AssetArgs {
    #[serde(flatten)]
    src: Source,
    /// Parser to use. Omit to detect from the content.
    format: Option<Format>,
    /// Filename to fall back on for format detection when the content
    /// carries no signature. Only the extension is used; content always
    /// wins over this, and it is ignored when `format` is given.
    filename: Option<String>,
    /// Asset ids to return bytes for. Omit to return all of them. The
    /// manifest always lists every asset regardless.
    ids: Option<Vec<u32>>,
}

// ── Output ───────────────────────────────────────────────────────────

#[cfg(target_family = "wasm")]
#[derive(Serialize)]
struct Converted {
    /// GitHub-Flavored Markdown.
    markdown: String,
    /// The parser actually used, so a caller relying on detection learns
    /// what it got.
    format: Format,
}

#[cfg(target_family = "wasm")]
#[derive(Serialize)]
struct Detected {
    format: Format,
    /// `content` when a signature identified it, `extension` when only the
    /// filename did. `extension` on a document that should have a signature
    /// is a sign the file is not what it claims.
    detected_from: DetectedFrom,
}

#[cfg(target_family = "wasm")]
#[derive(Serialize)]
struct AssetInfo {
    /// Index into the document's asset list; this is what `ids` selects on.
    id: u32,
    /// MIME type as the source recorded it, e.g. `image/png`.
    media_type: String,
    /// Package part or stream the asset came from, for provenance.
    origin_part: String,
    /// Size of the payload in bytes.
    byte_len: u32,
}

#[cfg(target_family = "wasm")]
#[derive(Serialize)]
struct Manifest {
    /// Every asset in the document, whether or not its bytes were returned.
    assets: Vec<AssetInfo>,
}

// ── Glue ─────────────────────────────────────────────────────────────

#[cfg(target_family = "wasm")]
fn to_act_error(e: &anydoc::ConvertError) -> ActError {
    let (kind, message) = crate::error::classify(e);
    ActError::new(kind, message)
}

/// Decide which parser to use.
///
/// An explicit `hint` is a caller assertion and short-circuits detection, so a
/// wrong assertion fails loudly at the parser rather than being silently
/// second-guessed. Otherwise content wins, and a filename extension is the
/// last resort.
#[cfg(target_family = "wasm")]
fn resolve_format(bytes: &[u8], hint: Option<Format>, filename: Option<&str>) -> ActResult<Format> {
    if let Some(f) = hint {
        return Ok(f);
    }
    crate::format::detect(bytes, filename)
        .map(|(f, _)| f)
        .ok_or_else(|| {
            ActError::invalid_args(
                "Unrecognized document: no known signature and no usable filename extension. \
                 Pass `format` explicitly — CSV in particular carries no signature and always \
                 needs it.",
            )
        })
}

// ── Tools ────────────────────────────────────────────────────────────

#[cfg(target_family = "wasm")]
#[act_component]
mod component {
    use super::*;

    #[act_tool(
        description = "Convert a document to GitHub-Flavored Markdown, preserving headings, lists, tables and footnotes. Handles Word, PowerPoint, Excel, OpenDocument, RTF, EPUB, CSV and PDF. The format is detected from the content; pass `format` for CSV, which carries no signature.",
        read_only
    )]
    fn convert(#[args] args: ConvertArgs) -> ActResult<Converted> {
        let (bytes, path) = args.src.read()?;
        let filename = args.filename.or(path);
        let format = resolve_format(&bytes, args.format, filename.as_deref())?;
        let markdown = anydoc::to_markdown_bytes(&bytes, anydoc::Format::from(format))
            .map_err(|e| to_act_error(&e))?;
        Ok(Converted { markdown, format })
    }

    #[act_tool(
        description = "Identify a document's format without converting it. Reports whether the format came from the content signature or only from the filename — an `extension` result on a format that should carry a signature means the file is not what its name claims. Much cheaper than convert.",
        read_only
    )]
    fn detect(#[args] args: DetectArgs) -> ActResult<Detected> {
        let (bytes, path) = args.src.read()?;
        let filename = args.filename.or(path);
        crate::format::detect(&bytes, filename.as_deref())
            .map(|(format, detected_from)| Detected {
                format,
                detected_from,
            })
            .ok_or_else(|| {
                ActError::invalid_args(
                    "Unrecognized document: no known signature and no usable filename extension. \
                     Pass `filename` if you know it — CSV in particular carries no signature.",
                )
            })
    }

    #[act_tool(
        description = "Extract the images and embedded objects from a document, returned as image content parts with their media type and originating package part. Emits a manifest first, so you can call again with `ids` to fetch only what you need instead of pulling every asset. Not supported for PDF.",
        read_only
    )]
    async fn extract_assets(#[args] args: AssetArgs, ctx: &mut ActContext<()>) -> ActResult<()> {
        let (bytes, path) = args.src.read()?;
        let filename = args.filename.or(path);
        let format = resolve_format(&bytes, args.format, filename.as_deref())?;

        // Upstream converts PDFs straight to Markdown without building a
        // document model, so there is no asset list to read.
        if format == Format::Pdf {
            return Err(ActError::invalid_args(
                "extract_assets does not support PDF: anydoc converts PDFs directly to Markdown \
                 with no intermediate document model. Use the `pdf-inspector` component for PDF \
                 images, or `convert` for the Markdown.",
            ));
        }

        let doc = anydoc::to_document(&bytes, anydoc::Format::from(format))
            .map_err(|e| to_act_error(&e))?;

        // The manifest lists everything, so a caller can see what is there
        // before deciding what to pull.
        let manifest = Manifest {
            assets: doc
                .assets
                .iter()
                .map(|a| AssetInfo {
                    id: a.id.0 as u32,
                    media_type: a.media_type.clone(),
                    origin_part: a.origin_part.clone(),
                    byte_len: a.bytes.len() as u32,
                })
                .collect(),
        };
        ctx.send_cbor(&manifest);

        for asset in doc.assets {
            let id = asset.id.0 as u32;
            if let Some(ids) = &args.ids
                && !ids.contains(&id)
            {
                continue;
            }
            // `send_content` takes an owned String, which is why this cannot
            // use the `Content` wrapper — that only accepts a &'static str,
            // and a media type read from a document is not static. Any
            // `image/*` part is mapped to a native MCP image block by the host.
            ctx.send_content(asset.bytes, Some(asset.media_type), vec![]);
        }
        Ok(())
    }
}
