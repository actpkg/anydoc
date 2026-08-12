"""convert: any document to GitHub-Flavored Markdown."""

import pytest

# (fixture, extra args, expected format, markdown substrings that must appear)
CONVERSIONS = [
    ("report.docx", {}, "docx", ("Quarterly Report", "twelve percent")),
    ("note.rtf", {}, "rtf", ("RTF note",)),
    # calamine is an entirely separate parser stack from the OOXML/docx path,
    # so this is the only case that exercises it. The sheet renders as a GFM
    # table.
    ("inventory.xlsx", {}, "excel", ("| Part | Qty |", "bolt")),
    # CSV carries no signature, so naming the format explicitly is all it needs.
    ("parts.csv", {"format": "csv"}, "csv", ("bolt",)),
    # PDF too — this is what makes the component a one-stop "any document to
    # Markdown" tool rather than one that routes by format.
    ("leaflet.pdf", {}, "pdf", ("leaflet",)),
]


@pytest.mark.parametrize("filename,extra_args,expected_format,substrings", CONVERSIONS)
async def test_converts_to_markdown(client, fixture_bytes, filename, extra_args, expected_format, substrings):
    result = await client.call_tool("convert", {"data": fixture_bytes(filename), **extra_args})
    data = result.structured_content
    assert data["format"] == expected_format
    for substring in substrings:
        assert substring in data["markdown"]


async def test_csv_with_no_hint_is_rejected(client, fixture_bytes):
    # CSV carries no signature, so with no hint there is nothing to detect.
    result = await client.call_tool("convert", {"data": fixture_bytes("parts.csv")}, raise_on_error=False)
    assert result.is_error
    assert result.meta["dev.actcore/error-kind"] == "std:invalid-args"
    assert "format" in result.content[0].text


async def test_explicit_format_overrides_detection_and_fails_at_the_parser(client, fixture_bytes, expect_error):
    # An explicit format is a caller assertion and overrides detection — so
    # asserting the wrong one fails at the parser rather than being
    # second-guessed.
    await expect_error(
        client, "convert", {"data": fixture_bytes("report.docx"), "format": "rtf"}, "std:invalid-args",
    )
