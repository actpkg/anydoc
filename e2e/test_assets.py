"""extract_assets: images and embedded objects, manifest first."""

import json


async def test_single_image_returns_manifest_and_content_part(client, fixture_bytes):
    result = await client.call_tool("extract_assets", {"data": fixture_bytes("with-image.docx")})
    # Multi-part result: manifest + one image part, so structured_content is
    # unpopulated (measured — only a single part decoding to a JSON object
    # gets one) and the manifest is read back from content[0].text instead.
    assert result.structured_content is None
    manifest = json.loads(result.content[0].text)
    assert len(manifest["assets"]) == 1
    assert manifest["assets"][0]["media_type"] == "image/png"
    assert manifest["assets"][0]["id"] == 0
    assert "image1.png" in manifest["assets"][0]["origin_part"]
    assert result.content[1].mimeType == "image/png"


async def test_selecting_by_id_on_a_single_asset_document(client, fixture_bytes):
    # Selecting by id returns the manifest plus only the chosen asset. With a
    # single-asset document this cannot distinguish "filtered" from "ids was
    # ignored" — the two-image cases below do that.
    result = await client.call_tool("extract_assets", {"data": fixture_bytes("with-image.docx"), "ids": [0]})
    manifest = json.loads(result.content[0].text)
    assert len(manifest["assets"]) == 1
    assert result.content[1].mimeType == "image/png"


async def test_two_images_unfiltered_returns_both_in_asset_id_order(client, fixture_bytes):
    # Two embedded images of different media types, unfiltered: the manifest
    # lists both, and both content parts follow in asset-id order.
    result = await client.call_tool("extract_assets", {"data": fixture_bytes("two-images.docx")})
    assert len(result.content) == 3
    manifest = json.loads(result.content[0].text)
    assert len(manifest["assets"]) == 2
    assert result.content[1].mimeType == "image/png"
    assert result.content[2].mimeType == "image/gif"


async def test_filtering_by_id_excludes_the_other_asset(client, fixture_bytes):
    # Filtering by `ids: [1]` on the same two-image document: the manifest
    # still lists both assets (it always lists everything), but only the
    # id-1 part (the GIF) follows — asserting the total content length is
    # what actually proves the PNG was excluded rather than merely unasserted.
    result = await client.call_tool("extract_assets", {"data": fixture_bytes("two-images.docx"), "ids": [1]})
    assert len(result.content) == 2
    manifest = json.loads(result.content[0].text)
    assert len(manifest["assets"]) == 2
    assert result.content[1].mimeType == "image/gif"


async def test_document_with_no_assets_returns_an_empty_manifest(client, fixture_bytes):
    # A document with no embedded assets returns an empty manifest, not an error.
    result = await client.call_tool("extract_assets", {"data": fixture_bytes("report.docx")})
    assert result.structured_content["assets"] == []


async def test_pdf_is_refused_with_a_pointer_to_pdf_inspector(client, fixture_bytes):
    # PDF has no document model upstream, so it is refused — with a pointer
    # to the component that can do the job.
    result = await client.call_tool("extract_assets", {"data": fixture_bytes("leaflet.pdf")}, raise_on_error=False)
    assert result.is_error
    assert result.meta["dev.actcore/error-kind"] == "std:invalid-args"
    assert "pdf-inspector" in result.content[0].text
