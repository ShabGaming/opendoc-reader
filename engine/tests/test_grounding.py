from opendoc_engine.grounding import END_OF_SENTENCE, parse_grounding
from opendoc_engine.ir import BlockType

PAGE = (1000, 2000)

SAMPLE = (
    "<|ref|>title<|/ref|><|det|>[[100, 50, 900, 100]]<|/det|>\n"
    "Quarterly Report\n\n"
    "<|ref|>sub_title<|/ref|><|det|>[[100, 120, 500, 150]]<|/det|>\n"
    "1. Summary\n\n"
    "<|ref|>text<|/ref|><|det|>[[100, 160, 900, 300]]<|/det|>\n"
    "Revenue grew by 12% over the previous quarter.\n"
    "A second line of the same paragraph.\n\n"
    "<|ref|>table<|/ref|><|det|>[[100, 320, 900, 500]]<|/det|>\n"
    '<table><tr><td rowspan="2">Region</td><td colspan="2">Sales</td></tr>'
    "<tr><td>Q1</td><td>Q2</td></tr>"
    "<tr><td>North</td><td>10</td><td>12</td></tr></table>\n\n"
    "<|ref|>image_caption<|/ref|><|det|>[[100, 510, 900, 530]]<|/det|>\n"
    "Table 1: Sales by region\n\n"
    "<|ref|>equation<|/ref|><|det|>[[300, 600, 700, 650]]<|/det|>\n"
    "\\[ E = mc^2 \\]\n\n"
    "<|ref|>formula_number<|/ref|><|det|>[[950, 600, 999, 650]]<|/det|>\n"
    "(1)\n\n"
    "<|ref|>image<|/ref|><|det|>[[100, 700, 900, 900]]<|/det|>\n\n"
    "<|ref|>footnote_x<|/ref|><|det|>[[0, 950, 999, 999]]<|/det|>\n"
    "Unknown label text" + END_OF_SENTENCE
)


def test_end_token_is_the_documented_literal():
    assert END_OF_SENTENCE == "<\uff5cend\u2581of\u2581sentence\uff5c>"
    assert END_OF_SENTENCE.startswith("<") and END_OF_SENTENCE.endswith(">")


def test_block_types_levels_and_order():
    blocks = parse_grounding(SAMPLE, PAGE)
    assert [b.type for b in blocks] == [
        BlockType.HEADING,
        BlockType.HEADING,
        BlockType.PARAGRAPH,
        BlockType.TABLE,
        BlockType.CAPTION,
        BlockType.FORMULA,
        BlockType.FIGURE,
        BlockType.OTHER,
    ]
    assert [b.level for b in blocks[:2]] == [1, 2]
    assert [b.order for b in blocks] == list(range(8))
    assert blocks[0].id == "p0-b0"
    assert parse_grounding(SAMPLE, PAGE, page_index=3)[0].id == "p3-b0"


def test_coordinates_are_scaled_to_page_pixels():
    blocks = parse_grounding(SAMPLE, PAGE)
    assert blocks[0].bbox == (100.1, 100.1, 900.9, 200.2)
    last = blocks[-1]
    assert last.bbox == (0.0, 1901.9, 1000.0, 2000.0)


def test_end_token_is_stripped_from_content():
    blocks = parse_grounding(SAMPLE, PAGE)
    assert blocks[-1].text == "Unknown label text"
    assert "end" not in (blocks[-1].text or "")


def test_paragraph_text_kept_verbatim():
    blocks = parse_grounding(SAMPLE, PAGE)
    assert blocks[2].text == (
        "Revenue grew by 12% over the previous quarter.\nA second line of the same paragraph."
    )


def test_table_html_becomes_cells_with_spans():
    table = parse_grounding(SAMPLE, PAGE)[3].table
    assert table is not None
    assert (table.rows, table.cols) == (3, 3)
    anchors = {(c.row, c.col): c for c in table.cells}
    assert anchors[(0, 0)].row_span == 2
    assert anchors[(0, 1)].col_span == 2
    # The cell under the rowspan starts at column 1, and "North" is back at column 0.
    assert anchors[(1, 1)].blocks[0].text == "Q1"
    assert anchors[(1, 2)].blocks[0].text == "Q2"
    assert anchors[(2, 0)].blocks[0].text == "North"
    assert table.has_merged_cells


def test_equation_delimiters_are_stripped():
    assert parse_grounding(SAMPLE, PAGE)[5].latex == "E = mc^2"


def test_formula_number_is_attached_to_the_preceding_formula():
    formula = parse_grounding(SAMPLE, PAGE)[5]
    assert formula.type is BlockType.FORMULA
    assert formula.latex == "E = mc^2"
    assert formula.number == "(1)"
    assert formula.to_dict()["number"] == "(1)"
    # The number is not a block of its own, and block ids stay contiguous.
    ids = [b.id for b in parse_grounding(SAMPLE, PAGE)]
    assert ids == [f"p0-b{i}" for i in range(8)]


def test_formula_without_a_number_has_none():
    text = "<|ref|>equation<|/ref|><|det|>[[0, 0, 999, 999]]<|/det|>\n$$ a + b $$"
    (block,) = parse_grounding(text, PAGE)
    assert block.number is None
    assert "number" not in block.to_dict()


def segment(label: str, box: str, content: str) -> str:
    return f"<|ref|>{label}<|/ref|><|det|>[[{box}]]<|/det|>\n{content}\n\n"


def test_formula_number_without_a_formula_before_it_is_kept_as_other():
    text = segment("text", "0, 0, 999, 100", "Intro") + segment(
        "formula_number", "900, 200, 999, 250", "(7)"
    )
    blocks = parse_grounding(text, PAGE)
    assert [b.type for b in blocks] == [BlockType.PARAGRAPH, BlockType.OTHER]
    assert blocks[1].text == "(7)"


def test_second_formula_number_after_one_equation_is_kept_as_other():
    text = (
        segment("equation", "0, 0, 800, 100", "x")
        + segment("formula_number", "900, 0, 999, 100", "(1)")
        + segment("formula_number", "900, 0, 999, 100", "(2)")
    )
    blocks = parse_grounding(text, PAGE)
    assert [b.type for b in blocks] == [BlockType.FORMULA, BlockType.OTHER]
    assert blocks[0].number == "(1)"
    assert blocks[1].text == "(2)"


def test_empty_formula_number_is_not_attached():
    text = segment("equation", "0, 0, 800, 100", "x") + segment(
        "formula_number", "900, 0, 999, 100", ""
    )
    blocks = parse_grounding(text, PAGE)
    assert blocks[0].number is None


def test_caption_links_to_preceding_table():
    blocks = parse_grounding(SAMPLE, PAGE)
    assert blocks[4].caption_for == blocks[3].id


def test_figure_has_no_crop_yet():
    figure = parse_grounding(SAMPLE, PAGE)[6]
    assert figure.type is BlockType.FIGURE
    assert figure.figure_crop is None


def test_malformed_box_falls_back_to_full_page_and_keeps_text():
    text = "<|ref|>text<|/ref|><|det|>[[12, 40, 5]]<|/det|>\nStill useful text"
    (block,) = parse_grounding(text, PAGE)
    assert block.bbox == (0.0, 0.0, 1000.0, 2000.0)
    assert block.text == "Still useful text"


def test_multiple_boxes_are_merged_into_their_union():
    text = "<|ref|>text<|/ref|><|det|>[[0, 0, 500, 100], [400, 50, 999, 300]]<|/det|>\nSplit"
    (block,) = parse_grounding(text, (999, 999))
    assert block.bbox == (0.0, 0.0, 999.0, 300.0)


def test_out_of_range_coordinates_are_clamped():
    text = "<|ref|>text<|/ref|><|det|>[[-5, 10, 1500, 20]]<|/det|>\nx"
    (block,) = parse_grounding(text, (999, 999))
    assert block.bbox == (0.0, 10.0, 999.0, 20.0)


def test_table_without_html_degrades_to_paragraph():
    text = "<|ref|>table<|/ref|><|det|>[[0, 0, 999, 999]]<|/det|>\n| a | b |"
    (block,) = parse_grounding(text, PAGE)
    assert block.type is BlockType.PARAGRAPH
    assert block.text == "| a | b |"


def test_untagged_text_is_kept_and_empty_input_is_empty():
    (block,) = parse_grounding("just text" + END_OF_SENTENCE, PAGE)
    assert block.type is BlockType.PARAGRAPH and block.text == "just text"
    assert parse_grounding(END_OF_SENTENCE, PAGE) == []
    assert parse_grounding("", PAGE) == []
