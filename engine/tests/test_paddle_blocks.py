"""Paddle layout results -> IR blocks, with output recorded from the real models.

The fixtures in tests/fixtures are the layout blocks (label, box, content) that PP-StructureV3
(`fast-*`) and PaddleOCR-VL (`vl-*`) produced for the three public-domain sample scans of the
model spike; the table is cut to its first rows to keep the files small.
"""

import json
from pathlib import Path

import pytest

from opendoc_engine.engines.fast import lines_within, text_lines
from opendoc_engine.ir import BlockType
from opendoc_engine.paddle_blocks import (
    LayoutItem,
    blocks_from_layout,
    heading_level,
    layout_items,
)

FIXTURES = Path(__file__).parent / "fixtures"


def load(name: str) -> dict:
    return json.loads((FIXTURES / f"{name}.json").read_text(encoding="utf-8"))


def convert(name: str, mode: str):
    return blocks_from_layout(layout_items(load(name)["blocks"]), mode)


def types(blocks):
    return [b.type for b in blocks]


def test_ids_and_order_are_contiguous():
    blocks = convert("vl-math", "vl")
    assert [b.id for b in blocks] == [f"b{i}" for i in range(len(blocks))]
    assert [b.order for b in blocks] == list(range(len(blocks)))


def test_fast_book_page_has_text_a_figure_and_its_caption():
    blocks = convert("fast-book", "fast")
    assert BlockType.FIGURE in types(blocks)
    assert types(blocks).count(BlockType.PARAGRAPH) == 4
    caption = next(b for b in blocks if b.type is BlockType.CAPTION)
    figure = next(b for b in blocks if b.type is BlockType.FIGURE)
    assert caption.caption_for == figure.id
    assert caption.text == "MUFF POTTER OUTWITTED."
    assert blocks[0].type is BlockType.HEADER
    assert blocks[-1].type is BlockType.PAGE_NUMBER


def test_fast_keeps_a_table_as_a_figure():
    blocks = convert("fast-table", "fast")
    assert BlockType.TABLE not in types(blocks)
    figure = next(b for b in blocks if b.type is BlockType.FIGURE)
    assert figure.figure_crop is None  # the pipeline crops it
    assert figure.bbox[3] - figure.bbox[1] > 1000  # the whole table


def test_fast_keeps_display_formulas_as_figures():
    blocks = convert("fast-math", "fast")
    assert BlockType.FORMULA not in types(blocks)
    assert types(blocks).count(BlockType.FIGURE) == 5


def test_fast_attaches_ocr_lines_to_text_blocks():
    data = load("fast-book")
    lines = text_lines(
        {
            "rec_boxes": [item["bbox"] for item in data["lines"]],
            "rec_texts": [item["text"] for item in data["lines"]],
        }
    )
    blocks = blocks_from_layout(
        layout_items(data["blocks"]), "fast", lines_for=lambda box: lines_within(lines, box)
    )
    paragraph = next(b for b in blocks if b.type is BlockType.PARAGRAPH)
    assert paragraph.lines
    x1, y1, x2, y2 = paragraph.bbox
    for line in paragraph.lines:
        assert x1 - 15 <= line.bbox[0] and line.bbox[2] <= x2 + 15
        assert y1 - 15 <= line.bbox[1] and line.bbox[3] <= y2 + 15
    assert [x.bbox[1] for x in paragraph.lines] == sorted(x.bbox[1] for x in paragraph.lines)
    figure = next(b for b in blocks if b.type is BlockType.FIGURE)
    assert figure.lines is None


def test_vl_table_has_header_cells_with_spans():
    blocks = convert("vl-table", "vl")
    table = next(b for b in blocks if b.type is BlockType.TABLE).table
    assert (table.rows, table.cols) == (7, 10)
    spans = {(c.row, c.col): (c.row_span, c.col_span) for c in table.cells}
    assert spans[(0, 0)] == (2, 1)  # "Race or people." spans both header rows
    assert spans[(0, 1)] == (1, 3)  # the year headings span three columns
    assert table.has_merged_cells
    first = next(c for c in table.cells if (c.row, c.col) == (0, 0))
    assert first.blocks[0].text == "Race or people."


def test_vl_table_caption_points_at_the_table():
    blocks = convert("vl-table", "vl")
    table = next(b for b in blocks if b.type is BlockType.TABLE)
    assert any(b.caption_for == table.id for b in blocks if b.type is BlockType.CAPTION)


def test_vl_formulas_are_latex_without_delimiters_and_carry_their_numbers():
    blocks = convert("vl-math", "vl")
    formulas = [b for b in blocks if b.type is BlockType.FORMULA]
    assert len(formulas) == 6
    assert all(b.latex and "$" not in b.latex[:1] for b in formulas)
    assert [b.number for b in formulas] == [None, "(2)", "(3)", "(4)", "(5)", "(6)"]
    # the numbers are not blocks of their own
    assert BlockType.OTHER not in types(blocks)


def test_vl_footnotes_and_inline_math_are_kept():
    blocks = convert("vl-math", "vl")
    assert types(blocks).count(BlockType.FOOTNOTE) == 2
    assert any("$TP_{1}M$" in (b.text or "") for b in blocks)


def test_number_with_no_formula_beside_it_stays_as_text():
    items = [
        LayoutItem("text", (0, 0, 100, 20), "intro"),
        LayoutItem("formula_number", (500, 300, 540, 320), "(9)"),
        LayoutItem("text", (0, 400, 100, 420), "outro"),
    ]
    blocks = blocks_from_layout(items, "vl")
    assert [b.text for b in blocks] == ["intro", "(9)", "outro"]
    assert blocks[1].type is BlockType.OTHER


def test_number_is_matched_to_the_nearest_formula_on_its_rows():
    items = [
        LayoutItem("display_formula", (100, 100, 400, 160), "$$a=b$$"),
        LayoutItem("display_formula", (100, 300, 400, 360), "$$c=d$$"),
        LayoutItem("formula_number", (420, 310, 470, 350), "(2)"),
    ]
    blocks = blocks_from_layout(items, "vl")
    assert [b.number for b in blocks] == [None, "(2)"]


def test_empty_text_and_unknown_labels():
    items = [
        LayoutItem("text", (0, 0, 10, 10), "  "),
        LayoutItem("mystery", (0, 20, 10, 30), "kept"),
        LayoutItem("table", (0, 40, 10, 50), "not html"),
    ]
    warnings = []
    blocks = blocks_from_layout(items, "vl", warn=warnings.append)
    assert [b.type for b in blocks] == [BlockType.OTHER, BlockType.FIGURE]
    assert warnings


@pytest.mark.parametrize(
    ("label", "text", "level"),
    [
        ("doc_title", "Title", 1),
        ("paragraph_title", "Introduction", 2),
        ("paragraph_title", "1 Introduction", 2),
        ("paragraph_title", "2.1 Method", 3),
        ("paragraph_title", "3.2.4. Detail", 4),
    ],
)
def test_heading_levels(label, text, level):
    assert heading_level(label, text) == level


def test_items_accept_objects_and_json_dicts():
    class Block:
        label, bbox, content = "text", [5, 6, 1, 2], "hello"

    items = layout_items(
        [
            Block(),
            {"block_label": "title", "block_bbox": [0, 0, 3, 4], "block_content": "x"},
            {"label": "text", "bbox": None, "content": "dropped"},
        ]
    )
    assert [i.label for i in items] == ["text", "title"]
    assert items[0].bbox == (1.0, 2.0, 5.0, 6.0)
