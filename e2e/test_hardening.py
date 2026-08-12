"""Hostile-input suite.

This component exists to run untrusted-input parsers (legacy OLE/CFB,
ZIP+XML, RTF) under a capability ceiling, so the interesting property is not
"it converts documents" but "it refuses hostile ones cleanly". A panic inside
wasm traps and kills the instance, so every case below asserts a structured
error — proof the guest returned rather than died. An *unexpected* McpError
with no structured `_meta` kind is that failure signature. One case below
(the XML-depth safety limit) legitimately uses a custom, non-`std:` kind for
a documented, non-trap reason — see its comment.
"""

import pytest


@pytest.mark.parametrize("tool,fixture_file", [
    ("convert", "truncated.docx"),  # valid ZIP magic, archive cut in half
    ("convert", "garbage.bin"),  # ZIP header followed by 4 KiB of noise
    ("convert", "empty.bin"),  # empty input
    ("detect", "garbage.bin"),  # detect must survive the same inputs
    ("extract_assets", "truncated.docx"),
])
async def test_rejects_hostile_bytes(client, fixture_bytes, expect_error, tool, fixture_file):
    await expect_error(client, tool, {"data": fixture_bytes(fixture_file)}, "std:invalid-args")


async def test_unbalanced_rtf_groups_are_recovered_not_rejected(client, fixture_bytes):
    # note.rtf cut in half, mid font-table entry: 3 open groups, 0 closes at
    # EOF. anydoc's RTF parser treats unbalanced groups as recoverable
    # (logged, not rejected) rather than a hard error, so this is a clean
    # success with empty output, not an error — a different but equally
    # valid "did not trap" outcome. Verified against the running component
    # before asserting.
    result = await client.call_tool("convert", {"data": fixture_bytes("truncated-note.rtf")})
    data = result.structured_content
    assert data["format"] == "rtf"
    assert data["markdown"] == ""


# ── Safety limits ────────────────────────────────────────────────────

async def test_xml_depth_safety_limit_fires_with_its_own_error_kind(client, fixture_bytes):
    # word/document.xml nests a chain ~300 levels deep — past anydoc's
    # MAX_XML_DEPTH (256), well-formed XML so it is the depth cap that fires,
    # not a parse error. This is the one error kind the component's sandbox
    # claim actually rests on (the fixed caps on entry/archive size, entry
    # count, XML depth and node count), so it gets its own end-to-end case
    # rather than staying unit-tested only in src/error.rs.
    #
    # `anydoc:resource-limit` is a custom (non-`std:`) kind — ACT-SPEC
    # permits namespaced kinds and requires hosts not to reject unrecognised
    # ones, and this is genuinely a structured error, not a papered-over trap.
    result = await client.call_tool("convert", {"data": fixture_bytes("deep-nest.docx")}, raise_on_error=False)
    assert result.is_error
    assert result.meta["dev.actcore/error-kind"] == "anydoc:resource-limit"
    assert "limit" in result.content[0].text


# ── Argument validation ──────────────────────────────────────────────

async def test_rejects_when_neither_source_supplied(client):
    # Neither source supplied.
    result = await client.call_tool("convert", {}, raise_on_error=False)
    assert result.is_error
    assert result.meta["dev.actcore/error-kind"] == "std:invalid-args"
    assert "data" in result.content[0].text


async def test_rejects_when_both_sources_supplied(client, fixture_bytes):
    # Both supplied — ambiguous, so rejected rather than silently preferring one.
    result = await client.call_tool(
        "convert", {"data": fixture_bytes("report.docx"), "path": "/tmp/x.docx"}, raise_on_error=False
    )
    assert result.is_error
    assert result.meta["dev.actcore/error-kind"] == "std:invalid-args"
    assert "not both" in result.content[0].text


# ── Capability ceiling ───────────────────────────────────────────────

async def test_path_source_with_no_grant_is_denied(client, expect_error):
    # A `path` source with no filesystem grant must be denied, not served.
    # The e2e host runs headless with no grant, so ask-by-default degrades to
    # deny and the component cannot read the file even though it exists.
    await expect_error(client, "convert", {"path": "fixtures/report.docx"}, "std:capability-denied")


async def test_data_source_needs_no_grant(client, fixture_bytes):
    # The same document as `data` needs no grant and succeeds — the ceiling
    # constrains the filesystem path, not the component's core function.
    result = await client.call_tool("convert", {"data": fixture_bytes("report.docx")})
    assert result.structured_content["format"] == "docx"
