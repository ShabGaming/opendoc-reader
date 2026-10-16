"""Parser for DeepSeek-OCR grounding output.

With `--special` on llama-server and the `<|grounding|>` prompt, the model answers with one
segment per layout block:

    <|ref|>LABEL<|/ref|><|det|>[[x1, y1, x2, y2]]<|/det|>
    content until the next <|ref|> tag

Coordinates are integers 0..999 normalized to the page image, and the output ends with an
end-of-sentence token. `parse_grounding` converts that text into IR blocks in page pixels.
Blocks keep the order the model emitted, which is the reading order.

Two leniencies, both logged: a segment whose box is missing or malformed gets the full-page
box rather than being dropped (the text is worth keeping), and text with no tag at all is
kept as a single full-page paragraph.
"""

from __future__ import annotations

import logging
import re

from opendoc_engine.htmltable import parse_html_table
from opendoc_engine.ir import BBox, Block, BlockType

log = logging.getLogger(__name__)

# <|end▁of▁sentence|> with U+FF5C (fullwidth bar) and U+2581 (lower one eighth block).
END_OF_SENTENCE = "<\uff5cend\u2581of\u2581sentence\uff5c>"

_SEGMENT_RE = re.compile(
    r"<\|ref\|>(?P<label>.*?)<\|/ref\|>\s*<\|det\|>(?P<det>.*?)<\|/det\|>", re.DOTALL
)
_NUMBER = r"(-?\d+(?:\.\d+)?)"
_BOX_RE = re.compile(r"\[\s*" + r"\s*,\s*".join([_NUMBER] * 4) + r"\s*\]")

_MAX_COORD = 999

# Labels seen in the model spike. Anything else becomes BlockType.OTHER.
_LABEL_TYPES: dict[str, tuple[BlockType, int | None]] = {
    "text": (BlockType.PARAGRAPH, None),
    "title": (BlockType.HEADING, 1),
    "sub_title": (BlockType.HEADING, 2),
    "table": (BlockType.TABLE, None),
    "image": (BlockType.FIGURE, None),
    "image_caption": (BlockType.CAPTION, None),
    "equation": (BlockType.FORMULA, None),
    "formula_number": (BlockType.OTHER, None),
}

_MATH_DELIMITERS = (("\\[", "\\]"), ("\\(", "\\)"), ("$$", "$$"), ("$", "$"))


def _scale(value: float, extent: int) -> float:
    return round(min(max(value, 0), _MAX_COORD) / _MAX_COORD * extent, 2)


def _boxes_to_bbox(det: str, page_size: tuple[int, int]) -> BBox | None:
    """Union of every well-formed box in a `<|det|>` payload, in page pixels."""
    boxes = [tuple(float(v) for v in m.groups()) for m in _BOX_RE.finditer(det)]
    if not boxes:
        return None
    width, height = page_size
    x1 = min(min(b[0], b[2]) for b in boxes)
    y1 = min(min(b[1], b[3]) for b in boxes)
    x2 = max(max(b[0], b[2]) for b in boxes)
    y2 = max(max(b[1], b[3]) for b in boxes)
    return (_scale(x1, width), _scale(y1, height), _scale(x2, width), _scale(y2, height))


def _strip_math(text: str) -> str:
    text = text.strip()
    for opening, closing in _MATH_DELIMITERS:
        if (
            len(text) >= len(opening) + len(closing)
            and text.startswith(opening)
            and text.endswith(closing)
        ):
            return text[len(opening) : -len(closing)].strip()
    return text


def _build_block(block_id: str, label: str, bbox: BBox, order: int, content: str) -> Block:
    block_type, level = _LABEL_TYPES.get(label, (BlockType.OTHER, None))
    block = Block(block_id, block_type, bbox, order, level=level)

    if block_type is BlockType.TABLE:
        table = parse_html_table(content, bbox, block_id)
        if table is not None:
            block.table = table
            return block
        log.warning("table block %s has no parsable HTML table; kept as text", block_id)
        block.type = BlockType.PARAGRAPH
    elif block_type is BlockType.FORMULA:
        block.latex = _strip_math(content)
        return block
    elif block_type is BlockType.FIGURE:
        return block  # the pipeline crops the page image and fills `figure_crop`

    if content:
        block.text = content
    return block


def _link_captions(blocks: list[Block]) -> None:
    """Point each caption at the figure or table just before it, else just after it."""
    targets = {BlockType.FIGURE, BlockType.TABLE}
    for i, block in enumerate(blocks):
        if block.type is not BlockType.CAPTION:
            continue
        for j in (i - 1, i + 1):
            if 0 <= j < len(blocks) and blocks[j].type in targets:
                block.caption_for = blocks[j].id
                break


def parse_grounding(text: str, page_size: tuple[int, int], page_index: int = 0) -> list[Block]:
    """Convert DeepSeek-OCR grounding output into IR blocks for one page.

    `page_size` is (width, height) in pixels of the image the model saw. Block ids are
    `p{page_index}-b{order}`.
    """
    text = text.replace(END_OF_SENTENCE, "")
    width, height = page_size
    full_page: BBox = (0.0, 0.0, float(width), float(height))

    # (label, bbox, content) per segment.
    segments: list[tuple[str, BBox, str]] = []
    matches = list(_SEGMENT_RE.finditer(text))

    lead = text[: matches[0].start()] if matches else text
    if lead.strip():
        log.warning("grounding output has untagged text; kept as a full-page paragraph")
        segments.append(("text", full_page, lead.strip()))

    for i, match in enumerate(matches):
        end = matches[i + 1].start() if i + 1 < len(matches) else len(text)
        content = text[match.end() : end].strip()
        bbox = _boxes_to_bbox(match.group("det"), page_size)
        if bbox is None:
            log.warning(
                "malformed box %r for %r; using the full page",
                match.group("det"),
                match.group("label"),
            )
            bbox = full_page
        segments.append((match.group("label").strip(), bbox, content))

    blocks = [
        _build_block(f"p{page_index}-b{order}", label, bbox, order, content)
        for order, (label, bbox, content) in enumerate(segments)
    ]
    _link_captions(blocks)
    return blocks
