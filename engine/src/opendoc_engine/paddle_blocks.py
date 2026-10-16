"""PaddleOCR layout results -> IR blocks.

Both Paddle pipelines (PP-StructureV3 for Fast, PaddleOCR-VL for Accurate) end in a list of
layout blocks with a label, a box in page pixels and the recognized content. This module turns
that list into IR blocks. It works on plain dicts (`{"label", "bbox", "content"}`) so it needs
no Paddle import and can be tested with recorded output.

The two pipelines differ in what they recognized. In Fast mode tables and formulas were not
recognized, so those blocks become figures and the pipeline crops them from the page image. In
VL mode tables arrive as HTML and formulas as LaTeX.
"""

from __future__ import annotations

import logging
import re
from collections.abc import Callable, Iterable
from dataclasses import dataclass
from typing import Any, Literal

from opendoc_engine.blocktext import escape_inline, normalize_blocks, strip_heading_markers
from opendoc_engine.htmltable import parse_html_table
from opendoc_engine.ir import BBox, Block, BlockType, Line
from opendoc_engine.mathtext import strip_math

log = logging.getLogger(__name__)

Mode = Literal["fast", "vl"]

# Layout labels of PP-DocLayout_plus-L and PP-DocLayoutV3 that become headings, with the level.
_HEADINGS = {"doc_title": 1, "paragraph_title": 2}
_TEXT = {
    "text",
    "abstract",
    "content",
    "reference",
    "reference_content",
    "algorithm",
    "vertical_text",
    "inline_formula",
}
_FIGURES = {"image", "chart", "seal", "header_image", "footer_image"}
_FORMULAS = {"formula", "display_formula"}
_FURNITURE = {
    "header": BlockType.HEADER,
    "footer": BlockType.FOOTER,
    "number": BlockType.PAGE_NUMBER,
    "footnote": BlockType.FOOTNOTE,
    "vision_footnote": BlockType.FOOTNOTE,
}
_CAPTION = "figure_title"
_FORMULA_NUMBER = "formula_number"

_SECTION_NUMBER = re.compile(r"^(\d+(?:\.\d+)*)[.)]?\s")


@dataclass
class LayoutItem:
    label: str
    bbox: BBox
    content: str


def layout_items(raw_blocks: Iterable[Any]) -> list[LayoutItem]:
    """Plain items from Paddle's `parsing_res_list`. Each entry may be a block object
    (`label`, `bbox`, `content`) or the dict that `save_to_json` writes (`block_label`,
    `block_bbox`, `block_content`)."""
    items = []
    for raw in raw_blocks:
        if isinstance(raw, dict):
            label = raw.get("block_label", raw.get("label", ""))
            bbox = raw.get("block_bbox", raw.get("bbox"))
            content = raw.get("block_content", raw.get("content", ""))
        else:
            label, bbox, content = raw.label, raw.bbox, raw.content
        if bbox is None or len(bbox) != 4:
            log.warning("layout block %r has no usable box; dropped", label)
            continue
        x1, y1, x2, y2 = (float(v) for v in bbox)
        items.append(
            LayoutItem(
                str(label), (min(x1, x2), min(y1, y2), max(x1, x2), max(y1, y2)), content or ""
            )
        )
    return items


def heading_level(label: str, text: str) -> int:
    """Level from the label, deepened by a section number such as `2.1`."""
    base = _HEADINGS[label]
    if label == "paragraph_title":
        match = _SECTION_NUMBER.match(text)
        if match:
            return min(base + match.group(1).count("."), 6)
    return base


def _clean(text: str) -> str:
    return " ".join(text.split())


def _overlap(a: BBox, b: BBox) -> float:
    """Share of the shorter vertical extent that `a` and `b` have in common."""
    shared = min(a[3], b[3]) - max(a[1], b[1])
    shorter = min(a[3] - a[1], b[3] - b[1])
    return shared / shorter if shorter > 0 else 0.0


def _horizontal_gap(a: BBox, b: BBox) -> float:
    return max(a[0] - b[2], b[0] - a[2], 0.0)


def _attach_numbers(blocks: list[Block], numbers: list[tuple[BBox, str]]) -> list[tuple[BBox, str]]:
    """Give each equation number to the formula it stands beside: same rows, on either side
    (books print the number at the left or at the right), the nearest one. Returns the numbers
    that matched no formula."""
    left_over = []
    for box, text in numbers:
        candidates = [
            b
            for b in blocks
            if b.type is BlockType.FORMULA and b.number is None and _overlap(b.bbox, box) >= 0.5
        ]
        if candidates:
            min(candidates, key=lambda b: _horizontal_gap(b.bbox, box)).number = text
        else:
            left_over.append((box, text))
    return left_over


def blocks_from_layout(
    items: list[LayoutItem],
    mode: Mode,
    *,
    lines_for: Callable[[BBox], list[Line]] | None = None,
    warn: Callable[[str], None] = log.warning,
) -> list[Block]:
    """IR blocks for one page, in the order of `items` (Paddle's reading order). Block ids are
    local (`b0`, ...). `lines_for` supplies the text lines inside a box, when known."""
    blocks: list[Block] = []
    numbers: list[tuple[int, BBox, str]] = []  # reading position, box and text

    for item in items:
        label, box = item.label, item.bbox
        # Fast text is literal OCR output, so markup characters are escaped. VL text is Markdown
        # written by the model: it keeps its line structure until `normalize_blocks` has
        # turned markers into IR structure.
        text = escape_inline(_clean(item.content)) if mode == "fast" else item.content.strip()
        block_id = f"b{len(blocks)}"
        order = len(blocks)
        block: Block | None = None

        if label in _FIGURES:
            block = Block(block_id, BlockType.FIGURE, box, order)
        elif label == "table":
            block = _table_block(block_id, item, mode, warn)
        elif label in _FORMULAS:
            block = _formula_block(block_id, item, mode)
        elif label == _FORMULA_NUMBER:
            if text:
                numbers.append((order, box, _clean(item.content)))
        elif not text:
            pass  # an empty text block carries nothing
        elif label in _HEADINGS:
            level = heading_level(label, strip_heading_markers(text))
            block = Block(block_id, BlockType.HEADING, box, order, level, text)
        elif label == _CAPTION:
            block = Block(block_id, BlockType.CAPTION, box, order, text=text)
        elif label in _FURNITURE:
            block = Block(block_id, _FURNITURE[label], box, order, text=text)
        else:
            if label not in _TEXT:
                log.info("layout label %r kept as an 'other' block", label)
            kind = BlockType.PARAGRAPH if label in _TEXT else BlockType.OTHER
            block = Block(block_id, kind, box, order, text=text)

        if block is None:
            continue
        if lines_for is not None and block.type in _LINE_TYPES:
            block.lines = lines_for(box) or None
        blocks.extend(normalize_blocks([block]) if mode == "vl" else [block])

    unmatched = _attach_numbers(blocks, [(box, text) for _, box, text in numbers])
    inserted = 0
    for position, box, text in numbers:
        if (box, text) in unmatched:
            # A number with no formula beside it keeps its text where it was read.
            blocks.insert(position + inserted, Block("", BlockType.OTHER, box, 0, text=text))
            inserted += 1

    for order, block in enumerate(blocks):
        block.id = f"b{order}"
        block.order = order
    _link_captions(blocks)
    return blocks


_LINE_TYPES = {
    BlockType.HEADING,
    BlockType.PARAGRAPH,
    BlockType.CAPTION,
    BlockType.FOOTNOTE,
    BlockType.HEADER,
    BlockType.FOOTER,
    BlockType.OTHER,
}


def _table_block(block_id: str, item: LayoutItem, mode: Mode, warn: Callable[[str], None]) -> Block:
    order = int(block_id[1:])
    if mode == "vl":
        table = parse_html_table(item.content, item.bbox, block_id) if item.content else None
        if table is not None:
            return Block(block_id, BlockType.TABLE, item.bbox, order, table=table)
        warn(f"table {block_id} has no parsable HTML; kept as a figure")
    return Block(block_id, BlockType.FIGURE, item.bbox, order)


def _formula_block(block_id: str, item: LayoutItem, mode: Mode) -> Block:
    order = int(block_id[1:])
    latex = strip_math(item.content) if mode == "vl" else ""
    if latex:
        return Block(block_id, BlockType.FORMULA, item.bbox, order, latex=latex)
    return Block(block_id, BlockType.FIGURE, item.bbox, order)


def _gap(a: BBox, b: BBox) -> float:
    return max(a[1] - b[3], b[1] - a[3], 0.0)


def _link_captions(blocks: list[Block]) -> None:
    """Point each caption at the figure or table next to it: whichever of the block before and
    the block after is nearer (the one before on a tie)."""
    targets = {BlockType.FIGURE, BlockType.TABLE}
    for i, block in enumerate(blocks):
        if block.type is not BlockType.CAPTION:
            continue
        neighbours = [blocks[j] for j in (i - 1, i + 1) if 0 <= j < len(blocks)]
        near = [n for n in neighbours if n.type in targets]
        if near:
            block.caption_for = min(near, key=lambda n: _gap(block.bbox, n.bbox)).id
