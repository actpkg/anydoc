//! The fixtures must actually convert. If this fails the e2e suite would fail
//! too, but far more slowly and with a much worse error message.

use std::path::Path;

fn read(name: &str) -> Vec<u8> {
    std::fs::read(Path::new("e2e/fixtures").join(name)).expect("fixture must exist")
}

#[test]
fn report_docx_converts_and_keeps_its_text() {
    let md = anydoc::to_markdown_bytes(&read("report.docx"), None).expect("should convert");
    assert!(md.contains("Quarterly Report"), "got: {md}");
    assert!(md.contains("twelve percent"), "got: {md}");
}

#[test]
fn csv_converts_when_the_format_is_named() {
    let md =
        anydoc::to_markdown_bytes(&read("parts.csv"), anydoc::Format::Csv).expect("should convert");
    assert!(md.contains("bolt"), "got: {md}");
}

#[test]
fn pdf_converts_through_the_same_entry_point() {
    let md = anydoc::to_markdown_bytes(&read("leaflet.pdf"), None).expect("should convert");
    assert!(md.contains("leaflet"), "got: {md}");
}

#[test]
fn rtf_converts() {
    let md = anydoc::to_markdown_bytes(&read("note.rtf"), None).expect("should convert");
    assert!(md.contains("RTF note"), "got: {md}");
}

#[test]
fn with_image_docx_carries_an_asset() {
    let doc = anydoc::to_document(&read("with-image.docx"), None).expect("should parse");
    assert!(
        !doc.assets.is_empty(),
        "fixture must embed at least one asset"
    );
    assert_eq!(doc.assets[0].media_type, "image/png");
}

#[test]
fn hostile_fixtures_error_rather_than_panic() {
    for name in ["truncated.docx", "garbage.bin", "empty.bin"] {
        let r = anydoc::to_markdown_bytes(&read(name), None);
        assert!(r.is_err(), "{name} should be refused, got Ok");
    }
}
