//! Format naming and detection precedence.
//!
//! Host-compilable: no SDK, no wit-bindgen, so `cargo test --target
//! x86_64-unknown-linux-gnu` covers everything here. Keep it that way — the
//! moment this file mentions `ActError` or `Bytes` the host tests stop
//! building.

use anydoc::Format as Upstream;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The 12 parsers anydoc exposes.
///
/// Fewer variants than the 14 advertised formats because several extensions
/// share one parser: `Docx` covers `.docx`/`.docm`, and `Excel` covers
/// `.xlsx`/`.xlsm`/`.xlsb`/`.xls`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    /// Binary Word 97-2003 (`.doc`).
    Doc,
    /// WordprocessingML (`.docx`, `.docm`).
    Docx,
    /// OpenDocument Text (`.odt`).
    Odt,
    /// Portable Document Format (`.pdf`).
    Pdf,
    /// Binary PowerPoint 97-2003 (`.ppt`, `.pps`, `.pot`).
    Ppt,
    /// PresentationML (`.pptx`, `.pptm`, `.ppsx`, `.ppsm`).
    Pptx,
    /// Rich Text Format (`.rtf`).
    Rtf,
    /// EPUB 2 and 3 (`.epub`).
    Epub,
    /// Excel workbooks: `.xlsx`, `.xlsm`, `.xlsb`, `.xls`.
    Excel,
    /// OpenDocument Spreadsheet (`.ods`).
    Ods,
    /// OpenDocument Presentation (`.odp`).
    Odp,
    /// Delimiter-separated text (`.csv`). Carries no signature.
    Csv,
}

impl Format {
    /// Stable lowercase name. This is what callers see in tool results and
    /// what they pass back as the `format` argument, so it must not drift.
    pub fn as_str(self) -> &'static str {
        match self {
            Format::Doc => "doc",
            Format::Docx => "docx",
            Format::Odt => "odt",
            Format::Pdf => "pdf",
            Format::Ppt => "ppt",
            Format::Pptx => "pptx",
            Format::Rtf => "rtf",
            Format::Epub => "epub",
            Format::Excel => "excel",
            Format::Ods => "ods",
            Format::Odp => "odp",
            Format::Csv => "csv",
        }
    }
}

impl From<Format> for Upstream {
    fn from(f: Format) -> Upstream {
        match f {
            Format::Doc => Upstream::Doc,
            Format::Docx => Upstream::Docx,
            Format::Odt => Upstream::Odt,
            Format::Pdf => Upstream::Pdf,
            Format::Ppt => Upstream::Ppt,
            Format::Pptx => Upstream::Pptx,
            Format::Rtf => Upstream::Rtf,
            Format::Epub => Upstream::Epub,
            Format::Excel => Upstream::Excel,
            Format::Ods => Upstream::Ods,
            Format::Odp => Upstream::Odp,
            Format::Csv => Upstream::Csv,
        }
    }
}

impl From<Upstream> for Format {
    fn from(f: Upstream) -> Format {
        match f {
            Upstream::Doc => Format::Doc,
            Upstream::Docx => Format::Docx,
            Upstream::Odt => Format::Odt,
            Upstream::Pdf => Format::Pdf,
            Upstream::Ppt => Format::Ppt,
            Upstream::Pptx => Format::Pptx,
            Upstream::Rtf => Format::Rtf,
            Upstream::Epub => Format::Epub,
            Upstream::Excel => Format::Excel,
            Upstream::Ods => Format::Ods,
            Upstream::Odp => Format::Odp,
            Upstream::Csv => Format::Csv,
        }
    }
}

/// How a format was arrived at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum DetectedFrom {
    /// Identified by the signature the container specification designates.
    Content,
    /// Identified only by the filename extension — the content carried no
    /// signature, or carried none we recognise.
    Extension,
}

/// The bare extension of a path, with no leading dot. `None` when there is none.
pub fn extension_of(path: &str) -> Option<&str> {
    let name = path.rsplit(['/', '\\']).next()?;
    let (stem, ext) = name.rsplit_once('.')?;
    if stem.is_empty() || ext.is_empty() {
        return None;
    }
    Some(ext)
}

/// Resolve a format from the bytes, falling back to a filename extension.
///
/// **Content wins over the extension**, deliberately: a mislabeled file is
/// exactly the case worth catching, and the signature is evidence where the
/// name is only a claim. This is *not* the same rule as an explicit `format`
/// argument, which is a caller assertion and short-circuits detection
/// entirely (see `anydoc::to_markdown_bytes`).
pub fn detect(bytes: &[u8], filename: Option<&str>) -> Option<(Format, DetectedFrom)> {
    if let Some(f) = Upstream::from_bytes(bytes) {
        return Some((f.into(), DetectedFrom::Content));
    }
    let ext = extension_of(filename?)?;
    Upstream::from_extension(ext).map(|f| (f.into(), DetectedFrom::Extension))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal ZIP whose first entry is an OOXML content-types part — enough
    /// for anydoc's container sniffing. Built inline so the unit tests need no
    /// fixture files on disk.
    fn docx_bytes() -> Vec<u8> {
        crate::format::tests_support::minimal_docx()
    }

    #[test]
    fn every_variant_round_trips_through_upstream() {
        use Format::*;
        for f in [
            Doc, Docx, Odt, Pdf, Ppt, Pptx, Rtf, Epub, Excel, Ods, Odp, Csv,
        ] {
            let upstream: anydoc::Format = f.into();
            assert_eq!(Format::from(upstream), f, "round trip failed for {f:?}");
        }
    }

    #[test]
    fn names_are_lowercase_and_stable() {
        assert_eq!(Format::Docx.as_str(), "docx");
        assert_eq!(Format::Excel.as_str(), "excel");
        assert_eq!(Format::Csv.as_str(), "csv");
    }

    #[test]
    fn content_signature_wins_over_a_lying_extension() {
        // The security-relevant case: a real document with a .txt name.
        let bytes = docx_bytes();
        let (format, from) = detect(&bytes, Some("innocent.txt")).expect("should detect");
        assert_eq!(format, Format::Docx);
        assert_eq!(from, DetectedFrom::Content);
    }

    #[test]
    fn csv_has_no_signature_and_falls_back_to_the_extension() {
        let bytes = b"name,qty\nbolt,4\n";
        assert!(
            anydoc::Format::from_bytes(bytes).is_none(),
            "csv must not be content-detectable"
        );
        let (format, from) = detect(bytes, Some("parts.csv")).expect("should detect via extension");
        assert_eq!(format, Format::Csv);
        assert_eq!(from, DetectedFrom::Extension);
    }

    #[test]
    fn unknown_content_with_no_filename_is_none() {
        assert!(detect(b"just some prose", None).is_none());
    }

    #[test]
    fn unknown_content_with_an_unknown_extension_is_none() {
        assert!(detect(b"just some prose", Some("notes.xyz")).is_none());
    }

    #[test]
    fn extension_is_taken_from_the_last_dot_and_is_case_insensitive() {
        assert_eq!(extension_of("/a/b/report.final.DOCX"), Some("DOCX"));
        assert_eq!(extension_of("noextension"), None);
        let (format, _) = detect(b"x", Some("R.CSV")).expect("uppercase extension should match");
        assert_eq!(format, Format::Csv);
    }
}

#[cfg(test)]
pub(crate) mod tests_support {
    /// A minimal `.docx`: a ZIP whose `[Content_Types].xml` names the
    /// WordprocessingML main-document content type, plus the document part
    /// itself. Stored (not deflated) so this needs no compressor.
    pub fn minimal_docx() -> Vec<u8> {
        let content_types = br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
        let document = br#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Hello</w:t></w:r></w:p></w:body></w:document>"#;
        crate::testzip::store(&[
            ("[Content_Types].xml", content_types.as_slice()),
            ("word/document.xml", document.as_slice()),
        ])
    }
}
