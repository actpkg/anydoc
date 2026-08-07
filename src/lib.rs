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
///
/// Kept as two optional fields rather than an untagged enum because a
/// flattened `#[serde(untagged)]` enum contributes no properties to the
/// generated JSON Schema — the source fields would be invisible to any agent
/// reading the tool catalogue. The "exactly one" invariant is enforced in
/// [`Source::read`].
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

#[cfg(target_family = "wasm")]
#[derive(Deserialize, JsonSchema)]
struct ConvertArgs {
    #[serde(flatten)]
    src: Source,
    /// Parser to use. Omit to detect from the content. Required for CSV,
    /// which carries no signature. An explicit value overrides detection.
    format: Option<Format>,
}

// Not wired to a tool yet — the `detect` tool lands in Task 6, which will
// construct this. `#[allow(dead_code)]` keeps clippy's `-D warnings` gate
// green in the interim; remove it once `detect` uses this.
#[cfg(target_family = "wasm")]
#[allow(dead_code)]
#[derive(Deserialize, JsonSchema)]
struct DetectArgs {
    #[serde(flatten)]
    src: Source,
    /// Filename to fall back on when the content carries no signature.
    /// Only the extension is used. Content always wins over this.
    filename: Option<String>,
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

// Same as `DetectArgs`: unused until Task 6 wires up the `detect` tool.
#[cfg(target_family = "wasm")]
#[allow(dead_code)]
#[derive(Serialize)]
struct Detected {
    format: Format,
    /// `content` when a signature identified it, `extension` when only the
    /// filename did. `extension` on a document that should have a signature
    /// is a sign the file is not what it claims.
    detected_from: DetectedFrom,
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
        let format = resolve_format(&bytes, args.format, path.as_deref())?;
        let markdown = anydoc::to_markdown_bytes(&bytes, anydoc::Format::from(format))
            .map_err(|e| to_act_error(&e))?;
        Ok(Converted { markdown, format })
    }
}
