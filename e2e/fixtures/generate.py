#!/usr/bin/env python3
"""Generate the e2e document fixtures.

Written by hand rather than copied from upstream's test corpus: upstream's
fixtures are real-world documents whose redistribution provenance is unclear.
Everything here is ours, and small enough to review.

Run from this directory:  python3 generate.py
"""

import base64
import json
import pathlib
import struct
import zipfile
import zlib

HERE = pathlib.Path(__file__).parent
ARGS = HERE / "args"

CT = "http://schemas.openxmlformats.org/package/2006/content-types"
WML = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"


def content_types(overrides: dict[str, str]) -> bytes:
    parts = "".join(
        f'<Override PartName="{n}" ContentType="{t}"/>' for n, t in overrides.items()
    )
    defaults = (
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="png" ContentType="image/png"/>'
    )
    return f'<?xml version="1.0" encoding="UTF-8"?><Types xmlns="{CT}">{defaults}{parts}</Types>'.encode()


def docx(
    body: str, extra: dict[str, bytes] | None = None, content_type_overrides: dict[str, str] | None = None
) -> bytes:
    """A WordprocessingML package with the given <w:body> content."""
    doc = (
        f'<?xml version="1.0" encoding="UTF-8"?>'
        f'<w:document xmlns:w="{WML}" xmlns:r="{REL}"><w:body>{body}</w:body></w:document>'
    ).encode()
    files = {
        "[Content_Types].xml": content_types(
            {
                "/word/document.xml": "application/vnd.openxmlformats-officedocument."
                "wordprocessingml.document.main+xml",
                **(content_type_overrides or {}),
            }
        ),
        "_rels/.rels": f'<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/officeDocument" Target="word/document.xml"/></Relationships>'.encode(),
        "word/document.xml": doc,
    }
    files.update(extra or {})
    return zip_bytes(files)


def zip_bytes(files: dict[str, bytes]) -> bytes:
    import io

    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
        for name, data in files.items():
            # A fixed date_time keeps the generator deterministic: without
            # it, `writestr` stamps each entry with the current wall-clock
            # time, so merely re-running the script perturbs every existing
            # fixture's bytes even when nothing about its content changed.
            zinfo = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            zinfo.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(zinfo, data)
    return buf.getvalue()


def para(text: str, style: str | None = None) -> str:
    pr = f'<w:pPr><w:pStyle w:val="{style}"/></w:pPr>' if style else ""
    return f"<w:p>{pr}<w:r><w:t>{text}</w:t></w:r></w:p>"


def minimal_pdf() -> bytes:
    """A one-page text-based PDF with a correct xref table."""
    stream = b"BT /F1 12 Tf 72 720 Td (A short leaflet.) Tj ET\n"
    objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
        b"/Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
        b"<< /Length %d >>\nstream\n" % len(stream) + stream + b"endstream",
    ]
    out = bytearray(b"%PDF-1.4\n")
    offsets = []
    for i, body in enumerate(objects, start=1):
        offsets.append(len(out))
        out += b"%d 0 obj\n" % i + body + b"\nendobj\n"
    xref_at = len(out)
    n = len(objects) + 1
    out += b"xref\n0 %d\n" % n + b"0000000000 65535 f \n"
    for off in offsets:
        out += b"%010d 00000 n \n" % off
    out += b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (n, xref_at)
    return bytes(out)


DRAWING_NS = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
A_NS = "http://schemas.openxmlformats.org/drawingml/2006/main"
PIC_NS = "http://schemas.openxmlformats.org/drawingml/2006/picture"


def drawing(rel_id: str) -> str:
    """An inline <w:drawing> referencing an image relationship."""
    return (
        "<w:p><w:r><w:drawing>"
        f'<wp:inline xmlns:wp="{DRAWING_NS}">'
        '<wp:extent cx="914400" cy="914400"/><wp:docPr id="1" name="Picture 1"/>'
        f'<a:graphic xmlns:a="{A_NS}"><a:graphicData uri="{PIC_NS}">'
        f'<pic:pic xmlns:pic="{PIC_NS}">'
        '<pic:nvPicPr><pic:cNvPr id="0" name="image1.png"/><pic:cNvPicPr/></pic:nvPicPr>'
        f'<pic:blipFill><a:blip r:embed="{rel_id}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>'
        '<pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="914400" cy="914400"/></a:xfrm>'
        '<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr>'
        "</pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"
    )


def png_1x1() -> bytes:
    """A valid 1x1 opaque red PNG, assembled chunk by chunk."""

    def chunk(tag: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    ihdr = struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0)
    raw = b"\x00\xff\x00\x00"  # filter byte + RGB
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", ihdr)
        + chunk(b"IDAT", zlib.compress(raw))
        + chunk(b"IEND", b"")
    )


def gif_1x1() -> bytes:
    """A valid 1x1 GIF89a image: a 2-color global color table, no Graphic
    Control Extension (transparency isn't needed for a format-detection
    fixture). This is the well-known 35-byte minimal GIF shape."""
    header = b"GIF89a"
    width, height = 1, 1
    packed_lsd = 0b1000_0000  # global color table present, 2^(0+1) = 2 entries
    lsd = struct.pack("<HHBBB", width, height, packed_lsd, 0, 0)
    color_table = b"\xff\xff\xff" + b"\x00\x00\x00"  # white, black
    image_descriptor = struct.pack("<BHHHHB", 0x2C, 0, 0, width, height, 0)
    # LZW min code size 2, one 2-byte sub-block (Clear, pixel 0, End packed
    # LSB-first into 3-bit codes), then the block terminator.
    image_data = bytes([0x02, 0x02, 0x44, 0x01, 0x00])
    trailer = b"\x3b"
    return header + lsd + color_table + image_descriptor + image_data + trailer


def main() -> None:
    # ── Good inputs ──────────────────────────────────────────────────
    report = docx(
        para("Quarterly Report", "Heading1")
        + para("Revenue grew by twelve percent.")
        + para("Costs held flat.")
    )
    (HERE / "report.docx").write_bytes(report)

    # The same bytes under a lying name, for the detection test.
    (HERE / "mislabeled.txt").write_bytes(report)

    # A docx carrying an embedded PNG, for extract_assets.
    #
    # The <w:drawing> reference is load-bearing: anydoc only retains assets the
    # document body actually references through a relationship. A package that
    # merely contains word/media/image1.png yields an EMPTY asset list.
    with_image = docx(
        para("Diagram") + drawing("rId9") + para("See the figure."),
        extra={
            "word/media/image1.png": png_1x1(),
            "word/_rels/document.xml.rels": f'<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId9" Type="{REL}/image" Target="media/image1.png"/></Relationships>'.encode(),
        },
    )
    (HERE / "with-image.docx").write_bytes(with_image)

    # A docx carrying two embedded images of *different* media types, so a
    # request that filters `extract_assets` by `ids` can be proven to have
    # actually filtered: any response part self-identifies by its MIME type.
    two_images = docx(
        para("Two figures") + drawing("rId9") + drawing("rId10") + para("End."),
        extra={
            "word/media/image1.png": png_1x1(),
            "word/media/image2.gif": gif_1x1(),
            "word/_rels/document.xml.rels": (
                f'<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
                f'<Relationship Id="rId9" Type="{REL}/image" Target="media/image1.png"/>'
                f'<Relationship Id="rId10" Type="{REL}/image" Target="media/image2.gif"/>'
                f"</Relationships>"
            ).encode(),
        },
        # A scoped Override rather than a new shared `Default`: anydoc reads
        # a docx image's media type from the part's file extension, not from
        # `[Content_Types].xml` (see `media_type_for`), so this is not
        # load-bearing for the test — but it keeps the package
        # self-describing per OPC without perturbing every other docx
        # fixture's bytes, which a shared `Default Extension="gif"` would.
        content_type_overrides={"/word/media/image2.gif": "image/gif"},
    )
    (HERE / "two-images.docx").write_bytes(two_images)

    (HERE / "parts.csv").write_bytes(b"part,qty\nbolt,4\nnut,8\n")

    # A minimal one-page PDF. anydoc converts PDFs straight to Markdown with
    # no document model, so this is what proves `extract_assets` refuses them.
    (HERE / "leaflet.pdf").write_bytes(minimal_pdf())
    note_rtf = rb"{\rtf1\ansi\deff0 {\fonttbl{\f0 Helvetica;}}\f0\fs24 A short RTF note.\par}" b"\n"
    (HERE / "note.rtf").write_bytes(note_rtf)

    # ── Hostile inputs ───────────────────────────────────────────────
    # Each must produce a clean structured error, never a trap. A panic inside
    # wasm kills the instance, which is exactly what these tests catch.

    # Valid ZIP magic, body cut mid-archive.
    (HERE / "truncated.docx").write_bytes(report[: len(report) // 2])

    # Deterministic noise behind a ZIP header.
    noise = bytes((i * 37 + 11) % 256 for i in range(4096))
    (HERE / "garbage.bin").write_bytes(b"PK\x03\x04" + noise)

    (HERE / "empty.bin").write_bytes(b"")

    # A WordprocessingML document whose word/document.xml nests a chain of
    # elements ~300 levels deep — past MAX_XML_DEPTH (256). Each level is
    # opened and closed correctly, so this is well-formed XML: the point is
    # to trip the *depth* limit while parsing, not a malformed-XML error. The
    # element name is arbitrary (`<w:x>` is not a real WordprocessingML
    # element) — the generic XML parser enforces the depth cap during raw
    # parsing, before anything tries to interpret the tree as a document.
    # 300 rather than exactly 256 leaves margin against the `<w:document>`/
    # `<w:body>` wrapper also counting toward the same stack.
    deep_depth = 300
    (HERE / "deep-nest.docx").write_bytes(
        docx(("<w:x>" * deep_depth) + "leaf" + ("</w:x>" * deep_depth))
    )

    # note.rtf, cut in half: mid-way through the font table (inside
    # `\fonttbl`, mid-entry), leaving 3 open groups and 0 closes. Same "cut
    # the file in half" shape as truncated.docx, for consistency. Trailing
    # `\n` is not part of the truncation — it's here so the committed file
    # already satisfies pre-commit's end-of-file-fixer; without it the hook
    # appends one on first commit, silently desyncing this file from the
    # args/truncated-note.json baked from the newline-less bytes above.
    (HERE / "truncated-note.rtf").write_bytes(note_rtf[: len(note_rtf) // 2] + b"\n")

    # ── Request bodies ───────────────────────────────────────────────
    # Ready-made, so the hurl tests stay a single source of truth with the
    # fixtures instead of carrying pasted base64 that drifts.
    ARGS.mkdir(exist_ok=True)
    for p in sorted(HERE.iterdir()):
        if p.suffix in {".py", ".json"} or p.is_dir():
            continue
        write_args(f"{p.stem}.json", {"data": b64(p.read_bytes())})

    csv_b64 = b64((HERE / "parts.csv").read_bytes())
    write_args("parts-typed.json", {"data": csv_b64, "format": "csv"})
    write_args("parts-named.json", {"data": csv_b64, "filename": "parts.csv"})
    write_args("mislabeled-named.json", {"data": b64(report), "filename": "innocent.txt"})
    write_args("report-wrong-format.json", {"data": b64(report), "format": "rtf"})
    write_args("with-image-id0.json", {"data": b64(with_image), "ids": [0]})
    write_args("two-images-id1.json", {"data": b64(two_images), "ids": [1]})
    write_args("no-source.json", {})
    write_args("both-sources.json", {"data": b64(report), "path": "/tmp/x.docx"})

    for p in sorted(HERE.iterdir()):
        if p.is_file() and p.suffix not in {".py", ".json"}:
            print(f"{p.name}: {p.stat().st_size} bytes")
    print(f"{len(list(ARGS.glob('*.json')))} request bodies in {ARGS.name}/")


def b64(data: bytes) -> dict:
    return {"$bytes": base64.b64encode(data).decode()}


def write_args(name: str, arguments: dict) -> None:
    (ARGS / name).write_text(json.dumps({"arguments": arguments}) + "\n")


if __name__ == "__main__":
    main()
