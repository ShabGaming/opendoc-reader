"""A tiny PDF writer for tests: pages of positioned Helvetica text, nothing else.

Hand-written so the tests need no fixture files and no extra dependency. Coordinates are PDF
points with the origin at the bottom left, as in the PDF format itself.
"""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class Text:
    x: float
    y: float
    size: float
    text: str
    bold: bool = False


@dataclass
class PageSpec:
    texts: list[Text] = field(default_factory=list)
    width: int = 612
    height: int = 792
    rotate: int = 0


def _escape(text: str) -> str:
    return text.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)")


def _content(texts: list[Text]) -> bytes:
    lines = []
    for item in texts:
        font = "F2" if item.bold else "F1"
        lines.append(f"BT /{font} {item.size} Tf {item.x} {item.y} Td ({_escape(item.text)}) Tj ET")
    return "\n".join(lines).encode("latin-1")


def make_pdf(pages: list[PageSpec]) -> bytes:
    """Serialize `pages` as a PDF file."""
    objects: dict[int, bytes] = {}
    objects[3] = b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"
    objects[4] = b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>"
    page_ids = []
    next_id = 5
    for spec in pages:
        page_id, content_id = next_id, next_id + 1
        next_id += 2
        page_ids.append(page_id)
        content = _content(spec.texts)
        objects[content_id] = (
            b"<< /Length %d >>\nstream\n" % len(content) + content + b"\nendstream"
        )
        rotate = f" /Rotate {spec.rotate}" if spec.rotate else ""
        objects[page_id] = (
            f"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {spec.width} {spec.height}]{rotate} "
            f"/Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents {content_id} 0 R >>"
        ).encode("latin-1")
    kids = " ".join(f"{i} 0 R" for i in page_ids)
    objects[2] = f"<< /Type /Pages /Kids [{kids}] /Count {len(page_ids)} >>".encode("latin-1")
    objects[1] = b"<< /Type /Catalog /Pages 2 0 R >>"

    out = bytearray(b"%PDF-1.4\n")
    offsets: dict[int, int] = {}
    for number in sorted(objects):
        offsets[number] = len(out)
        out += b"%d 0 obj\n" % number + objects[number] + b"\nendobj\n"
    xref = len(out)
    out += b"xref\n0 %d\n" % (len(objects) + 1)
    out += b"0000000000 65535 f \n"
    for number in sorted(objects):
        out += b"%010d 00000 n \n" % offsets[number]
    out += b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (
        len(objects) + 1,
        xref,
    )
    return bytes(out)
