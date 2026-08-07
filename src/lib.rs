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
