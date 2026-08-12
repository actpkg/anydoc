"""detect: identify a document's format without converting it."""

import pytest

# (fixture, extra args, expected format, expected detected_from)
DETECTIONS = [
    ("report.docx", {}, "docx", "content"),
    # The same bytes under a lying filename. Content wins — this is the case
    # the tool exists for.
    ("report.docx", {"filename": "innocent.txt"}, "docx", "content"),
    # CSV has no signature, so the extension is all there is.
    ("parts.csv", {"filename": "parts.csv"}, "csv", "extension"),
]


@pytest.mark.parametrize("filename,extra_args,expected_format,expected_from", DETECTIONS)
async def test_detects_format(client, fixture_bytes, filename, extra_args, expected_format, expected_from):
    result = await client.call_tool("detect", {"data": fixture_bytes(filename), **extra_args})
    data = result.structured_content
    assert data["format"] == expected_format
    assert data["detected_from"] == expected_from


async def test_no_signature_and_no_filename_is_rejected(client, fixture_bytes, expect_error):
    # No signature and no filename: nothing to go on.
    await expect_error(client, "detect", {"data": fixture_bytes("parts.csv")}, "std:invalid-args")


async def test_path_source_reads_and_detects_under_a_grant(granted_client, fixtures_dir):
    # The same property again, but through `path` on a real file sitting on
    # disk under a lying .txt name, against a host granted read access to
    # e2e/fixtures/. This is the only e2e case that reads via `path`
    # successfully — every other `path` case (hardening) asserts the
    # capability-denied ceiling instead, since a denied call never reaches
    # format detection at all.
    result = await granted_client.call_tool("detect", {"path": str(fixtures_dir / "mislabeled.txt")})
    data = result.structured_content
    assert data["format"] == "docx"
    assert data["detected_from"] == "content"
