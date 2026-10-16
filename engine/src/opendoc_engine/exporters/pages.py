"""Block selection and ordering shared by the exporters."""

from __future__ import annotations

from opendoc_engine.ir import Block, BlockType, Page

OMITTED = {BlockType.HEADER, BlockType.FOOTER, BlockType.PAGE_NUMBER}


def page_blocks(page: Page) -> list[Block]:
    """Blocks of a page in reading order, page furniture removed, footnotes last."""
    blocks = [b for b in sorted(page.blocks, key=lambda b: b.order) if b.type not in OMITTED]
    return [b for b in blocks if b.type is not BlockType.FOOTNOTE] + [
        b for b in blocks if b.type is BlockType.FOOTNOTE
    ]
