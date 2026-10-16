"""PDF text layer -> IR blocks, for the "Use existing text" path (`text.extract`).

PDFium gives every character of a page with its box. This module groups them into lines,
lines into paragraphs and lists, and decides headings from font size, then maps the boxes to
pixels of the page image. Reading order is the order of the PDF's content stream, which is the
reading order of nearly every born-digital document.

There is no table, formula or figure detection here: text that sits in a table comes out as
lines and paragraphs, which is why the UI only offers this path for PDFs with a text layer.
Grouping is done in page space (text is horizontal there) and only the finished boxes are
rotated into image space, so pages with a /Rotate entry work.
"""

from __future__ import annotations

import ctypes
import re
import statistics
from collections import Counter
from dataclasses import dataclass, field

import pypdfium2 as pdfium
import pypdfium2.raw as pdfium_raw

from opendoc_engine.blocktext import escape_inline
from opendoc_engine.errors import bad_input
from opendoc_engine.ir import BBox, Block, BlockType, Line, ListItem

# Line grouping, in multiples of the font size.
_MAX_GAP = 4.0  # a wider horizontal gap on one baseline starts a new line (columns, tab stops)
_MIN_OVERLAP = 0.3  # share of the smaller height that two chars on one line must overlap
_SPACE_GAP = 0.25  # gap between two chars that is shown as a space when the PDF has none
# Paragraph grouping.
_MAX_PITCH = 1.6  # baseline distance of consecutive lines in one paragraph
_SIZE_TOLERANCE = 0.15
_INDENT = 0.8  # a first line indented by more than this starts a paragraph
# Headings.
_HEADING_RATIO = 1.15
_HEADING_MAX_CHARS = 160
_HEADING_MAX_LINES = 3
_BOLD_WEIGHT = 600
# Furniture zones: share of the page height at the top and bottom.
_MARGIN_ZONE = 0.05

# Bullet characters: bullet, black circle, white circle, white bullet, small black square, black
# square, middle dot, en dash, em dash.
_BULLETS = "".join(chr(code) for code in (0x2022, 0x25CF, 0x25CB, 0x25E6, 0x25AA, 0x25A0, 0xB7))
_DASHES = "".join(chr(code) for code in (0x2013, 0x2014))
_LIST_MARKER = re.compile(
    "^(?P<marker>[" + re.escape(_BULLETS + _DASHES) + r"*\-]"
    r"|\(?\d{1,3}[.)]|\(?[A-Za-z][.)]|\(?[ivxIVX]{1,5}[.)])\s+(?P<rest>\S.*)$"
)
_PAGE_NUMBER = re.compile(r"^(?:page\s+)?\d{1,4}(?:\s*(?:/|of)\s*\d{1,4})?$", re.IGNORECASE)

_COORD_SCALE = 100  # FPDF_PageToDevice works in integers; ask for hundredths of a point


@dataclass
class _Line:
    left: float = float("inf")
    bottom: float = float("inf")
    right: float = float("-inf")
    top: float = float("-inf")
    text: str = ""
    sizes: list[float] = field(default_factory=list)
    bottoms: list[float] = field(default_factory=list)
    weights: list[int] = field(default_factory=list)

    def add(self, char: str, box: tuple[float, float, float, float], size: float, weight: int):
        self.text += char
        if char.isspace():
            return
        left, bottom, right, top = box
        self.left = min(self.left, left)
        self.right = max(self.right, right)
        self.bottom = min(self.bottom, bottom)
        self.top = max(self.top, top)
        self.sizes.append(size)
        self.bottoms.append(bottom)
        self.weights.append(weight)

    @property
    def empty(self) -> bool:
        return not self.sizes

    @property
    def size(self) -> float:
        return statistics.median(self.sizes)

    @property
    def baseline(self) -> float:
        return statistics.median(self.bottoms)

    @property
    def bold(self) -> bool:
        return bool(self.weights) and min(self.weights) >= _BOLD_WEIGHT

    def continues_with(self, box: tuple[float, float, float, float], size: float) -> bool:
        left, bottom, _right, top = box
        overlap = min(top, self.top) - max(bottom, self.bottom)
        smaller = min(top - bottom, self.top - self.bottom)
        if smaller <= 0 or overlap < _MIN_OVERLAP * smaller:
            return False
        reference = max(size, self.size)
        return self.right - 0.5 * reference <= left <= self.right + _MAX_GAP * reference


def _read_lines(page: pdfium.PdfPage) -> list[_Line]:
    textpage = page.get_textpage()
    try:
        lines: list[_Line] = []
        current = _Line()

        def flush() -> None:
            nonlocal current
            if not current.empty:
                current.text = " ".join(current.text.split())
                lines.append(current)
            current = _Line()

        for index in range(textpage.count_chars()):
            char = textpage.get_text_range(index, 1)
            if not char or 0xD800 <= ord(char[0]) <= 0xDFFF:
                continue
            if char in "\r\n" or (
                pdfium_raw.FPDFText_IsGenerated(textpage, index) and char.isspace()
            ):
                flush()
                continue
            box = textpage.get_charbox(index)
            size = pdfium_raw.FPDFText_GetFontSize(textpage, index)
            if char.isspace():
                current.add(" ", box, size, 0)
                continue
            if size <= 0 or box[2] <= box[0] or box[3] <= box[1]:
                continue
            if not current.empty and not current.continues_with(box, size):
                flush()
            elif (
                not current.empty
                and not current.text.endswith(" ")
                and box[0] - current.right > _SPACE_GAP * size
            ):
                current.text += " "
            weight = pdfium_raw.FPDFText_GetFontWeight(textpage, index)
            current.add(char, box, size, weight)
        flush()
        return lines
    finally:
        textpage.close()


# --- grouping -------------------------------------------------------------------------------


@dataclass
class _Group:
    kind: str  # "paragraph" or "list"
    lines: list[_Line] = field(default_factory=list)
    items: list[list[_Line]] = field(default_factory=list)  # lists only
    ordered: bool = False


def _marker(line: _Line) -> tuple[str, str] | None:
    match = _LIST_MARKER.match(line.text)
    if not match:
        return None
    marker = match.group("marker")
    # A lone capital letter with a period at the start of a sentence ("A. Smith") is not a list.
    letter = marker.strip("().")
    if marker[-1] in ".)" and len(letter) == 1 and letter.isupper():
        return None
    return marker, match.group("rest")


def _same_paragraph(previous: _Line, line: _Line) -> bool:
    size = max(previous.size, line.size)
    if abs(previous.size - line.size) > _SIZE_TOLERANCE * size:
        return False
    pitch = previous.baseline - line.baseline
    if not 0 < pitch <= _MAX_PITCH * size:
        return False
    return line.left - previous.left <= _INDENT * size


def _group(lines: list[_Line]) -> list[_Group]:
    groups: list[_Group] = []
    for line in lines:
        marker = _marker(line)
        current = groups[-1] if groups else None
        previous = current.lines[-1] if current and current.lines else None
        if marker:
            if current and current.kind == "list" and previous is not None:
                size = max(previous.size, line.size)
                if abs(line.left - current.items[0][0].left) < size:
                    current.items.append([line])
                    current.lines.append(line)
                    continue
            group = _Group(
                "list", [line], [[line]], ordered=marker[0][0].isalnum() or marker[0][0] == "("
            )
            groups.append(group)
            continue
        if current and previous is not None:
            if current.kind == "list":
                size = max(previous.size, line.size)
                pitch = previous.baseline - line.baseline
                if (
                    0 < pitch <= 1.5 * size
                    and line.left >= current.items[0][0].left - 0.2 * size
                    and abs(previous.size - line.size) <= _SIZE_TOLERANCE * size
                ):
                    current.items[-1].append(line)
                    current.lines.append(line)
                    continue
            elif _same_paragraph(previous, line):
                current.lines.append(line)
                continue
        groups.append(_Group("paragraph", [line]))
    return groups


def _join(lines: list[_Line], strip_marker: bool = False) -> str:
    texts = [line.text for line in lines]
    if strip_marker and texts:
        marker = _marker(lines[0])
        if marker:
            texts[0] = marker[1]
    out = texts[0] if texts else ""
    for text in texts[1:]:
        if len(out) > 1 and out.endswith("-") and out[-2].isalpha() and text[:1].islower():
            out = out[:-1] + text
        else:
            out = f"{out} {text}"
    return out


def _body_size(lines: list[_Line]) -> float:
    counts: Counter[float] = Counter()
    for line in lines:
        for size in line.sizes:
            counts[round(size, 1)] += 1
    return counts.most_common(1)[0][0] if counts else 0.0


# --- geometry -------------------------------------------------------------------------------


class _Mapper:
    """Page space (points, origin bottom left, unrotated) to image pixels."""

    def __init__(self, page: pdfium.PdfPage, image_size: tuple[int, int], path: str) -> None:
        self._page = page
        width_pt, height_pt = page.get_size()
        self.width_pt, self.height_pt = width_pt, height_pt
        self.sx = image_size[0] / width_pt
        self.sy = image_size[1] / height_pt
        if abs(self.sx - self.sy) > 0.02 * max(self.sx, self.sy):
            raise bad_input(
                "image_mismatch",
                f"{path}: image {image_size[0]}x{image_size[1]} does not fit a "
                f"{width_pt:.0f}x{height_pt:.0f} pt page",
            )
        self._size = (int(width_pt * _COORD_SCALE), int(height_pt * _COORD_SCALE))

    def point(self, x: float, y: float) -> tuple[float, float]:
        dx, dy = ctypes.c_int(), ctypes.c_int()
        pdfium_raw.FPDF_PageToDevice(
            self._page,
            0,
            0,
            self._size[0],
            self._size[1],
            0,
            x,
            y,
            ctypes.byref(dx),
            ctypes.byref(dy),
        )
        return dx.value / _COORD_SCALE, dy.value / _COORD_SCALE

    def bbox(self, left: float, bottom: float, right: float, top: float) -> BBox:
        x1, y1 = self.point(left, top)
        x2, y2 = self.point(right, bottom)
        x1, x2 = sorted((x1 * self.sx, x2 * self.sx))
        y1, y2 = sorted((y1 * self.sy, y2 * self.sy))
        return (round(x1, 1), round(y1, 1), round(x2, 1), round(y2, 1))


def _union(lines: list[_Line]) -> tuple[float, float, float, float]:
    return (
        min(line.left for line in lines),
        min(line.bottom for line in lines),
        max(line.right for line in lines),
        max(line.top for line in lines),
    )


# --- blocks ---------------------------------------------------------------------------------


def extract_blocks(
    pdf: pdfium.PdfDocument, page_index: int, image_size: tuple[int, int], path: str
) -> list[Block]:
    """IR blocks (ids `b0`, `b1`, ...) for one PDF page, boxes in pixels of an image of
    `image_size` that shows the whole page unrotated."""
    page = pdf[page_index]
    try:
        mapper = _Mapper(page, image_size, path)
        lines = _read_lines(page)
        if not lines:
            return []
        body = _body_size(lines)
        blocks: list[Block] = []
        for group in _group(lines):
            block = _make_block(group, body, mapper, len(blocks))
            if block is not None:
                blocks.append(block)
        return blocks
    finally:
        page.close()


def _zone(group: _Group, mapper: _Mapper) -> str | None:
    """`top` or `bottom` when the group lies in the page margin zone. Page space has its origin
    at the bottom left, so a high `bottom` is near the top of the page."""
    _left, bottom, _right, top = _union(group.lines)
    if bottom >= mapper.height_pt * (1 - _MARGIN_ZONE):
        return "top"
    if top <= mapper.height_pt * _MARGIN_ZONE:
        return "bottom"
    return None


def _make_block(group: _Group, body: float, mapper: _Mapper, order: int) -> Block | None:
    block_id = f"b{order}"
    box = mapper.bbox(*_union(group.lines))
    if group.kind == "list":
        items = [
            ListItem(text=escape_inline(_join(item, strip_marker=True)), ordered=group.ordered)
            for item in group.items
        ]
        return Block(block_id, BlockType.LIST, box, order, items=items)

    text = escape_inline(_join(group.lines))  # PDF text is literal, IR text is Markdown
    if not text:
        return None
    lines = [Line(mapper.bbox(*_union([line])), line.text) for line in group.lines]
    size = statistics.median(s for line in group.lines for s in line.sizes)

    zone = _zone(group, mapper)
    if len(group.lines) == 1 and zone is not None:
        if _PAGE_NUMBER.match(text):
            return Block(block_id, BlockType.PAGE_NUMBER, box, order, text=text, lines=lines)
        kind = BlockType.HEADER if zone == "top" else BlockType.FOOTER
        return Block(block_id, kind, box, order, text=text, lines=lines)

    short = len(text) <= _HEADING_MAX_CHARS and len(group.lines) <= _HEADING_MAX_LINES
    if short and body and size >= _HEADING_RATIO * body:
        ratio = size / body
        level = 1 if ratio >= 1.7 else 2 if ratio >= 1.35 else 3
        return Block(block_id, BlockType.HEADING, box, order, level=level, text=text, lines=lines)
    bold_title = (
        len(group.lines) == 1
        and group.lines[0].bold
        and size >= body
        and len(text) <= 100
        and not text.endswith((".", ",", ";"))
    )
    if short and bold_title:
        return Block(block_id, BlockType.HEADING, box, order, level=3, text=text, lines=lines)
    return Block(block_id, BlockType.PARAGRAPH, box, order, text=text, lines=lines)
