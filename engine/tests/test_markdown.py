from opendoc_engine.exporters import to_markdown
from opendoc_engine.exporters.markdown import figure_files
from opendoc_engine.ir import (
    Block,
    BlockType,
    Document,
    EngineInfo,
    ListItem,
    Page,
    Source,
    Table,
    TableCell,
)

BOX = (0.0, 0.0, 1.0, 1.0)


def para(block_id: str, text: str) -> Block:
    return Block(block_id, BlockType.PARAGRAPH, BOX, 0, text=text)


def cell(row: int, col: int, text: str, **kwargs) -> TableCell:
    return TableCell(row, col, blocks=[para(f"c{row}{col}", text)], **kwargs)


def document(*pages: list[Block]) -> Document:
    return Document(
        source=Source("a.pdf", "pdf", len(pages)),
        engine=EngineInfo("demo", "none", "0.1.0"),
        pages=[Page(i, 100, 100, f"{i}.png", blocks) for i, blocks in enumerate(pages)],
    )


def ordered(blocks: list[Block]) -> list[Block]:
    for i, block in enumerate(blocks):
        block.order = i
    return blocks


def test_golden_two_pages():
    gfm = Table(
        2,
        2,
        [
            cell(0, 0, "Item", header=True),
            cell(0, 1, "Qty", header=True),
            cell(1, 0, "a|b"),
            cell(1, 1, "3"),
        ],
    )
    merged = Table(
        2,
        2,
        [
            cell(0, 0, "Totals & more", header=True, col_span=2),
            cell(1, 0, "Net"),
            cell(1, 1, "100"),
        ],
    )
    page1 = ordered(
        [
            Block("h", BlockType.HEADER, BOX, 0, text="Company header"),
            Block("t", BlockType.HEADING, BOX, 0, level=1, text="Report"),
            Block("s", BlockType.HEADING, BOX, 0, level=2, text="Summary"),
            para("p", "Hello world."),
            Block(
                "l",
                BlockType.LIST,
                BOX,
                0,
                items=[ListItem("one", True, [ListItem("nested")]), ListItem("two", True)],
            ),
            Block("tb1", BlockType.TABLE, BOX, 0, table=gfm),
            Block("f", BlockType.FORMULA, BOX, 0, latex="E = mc^2"),
            Block("pn", BlockType.PAGE_NUMBER, BOX, 0, text="1"),
        ]
    )
    page2 = ordered(
        [
            Block("fig", BlockType.FIGURE, BOX, 0, figure_crop="C:\\tmp\\crops\\p1-b0.png"),
            Block("cap", BlockType.CAPTION, BOX, 0, text="Figure 1", caption_for="fig"),
            Block("tb2", BlockType.TABLE, BOX, 0, table=merged),
            Block("ft", BlockType.FOOTER, BOX, 0, text="Footer"),
        ]
    )

    assert to_markdown(document(page1, page2)) == (
        "# Report\n"
        "\n"
        "## Summary\n"
        "\n"
        "Hello world.\n"
        "\n"
        "1. one\n"
        "   - nested\n"
        "2. two\n"
        "\n"
        "| Item | Qty |\n"
        "| --- | --- |\n"
        "| a\\|b | 3 |\n"
        "\n"
        "$$\n"
        "E = mc^2\n"
        "$$\n"
        "\n"
        "<!-- page 2 -->\n"
        "\n"
        "![](assets/p1-b0.png)\n"
        "\n"
        "*Figure 1*\n"
        "\n"
        "<table>\n"
        '<tr><th colspan="2">Totals &amp; more</th></tr>\n'
        "<tr><td>Net</td><td>100</td></tr>\n"
        "</table>\n"
    )


def test_figure_without_crop_uses_block_id():
    doc = document([Block("p0-b3", BlockType.FIGURE, BOX, 0)])
    assert to_markdown(doc) == "![](assets/p0-b3.png)\n"


def test_blocks_render_in_order_field_order():
    first, second = para("a", "first"), para("b", "second")
    first.order, second.order = 1, 0
    assert to_markdown(document([first, second])) == "second\n\nfirst\n"


def test_empty_document_is_empty_string():
    assert to_markdown(document()) == ""


def test_assets_folder_name_and_links_are_relative_and_encoded():
    doc = document([Block("f", BlockType.FIGURE, BOX, 0, figure_crop="C:\\w\\crops\\p0-b1.png")])
    assert to_markdown(doc, assets_dir="my doc (2)_assets") == (
        "![](my%20doc%20%282%29_assets/p0-b1.png)\n"
    )


def test_figures_sharing_a_file_name_get_distinct_names():
    doc = document(
        ordered(
            [
                Block("a", BlockType.FIGURE, BOX, 0, figure_crop="C:\\one\\fig.png"),
                Block("b", BlockType.FIGURE, BOX, 0, figure_crop="C:\\two\\fig.png"),
            ]
        )
    )
    assert figure_files(doc) == {
        "a": ("fig.png", "C:\\one\\fig.png"),
        "b": ("fig-2.png", "C:\\two\\fig.png"),
    }
    assert to_markdown(doc) == "![](assets/fig.png)\n\n![](assets/fig-2.png)\n"


def test_footnotes_stay_at_the_end_of_their_page():
    note = Block("fn", BlockType.FOOTNOTE, BOX, 0, text="1 note")
    page1 = ordered([para("a", "first"), note, para("b", "second")])
    page2 = ordered([para("c", "third")])
    assert to_markdown(document(page1, page2)) == (
        "first\n\nsecond\n\n1 note\n\n<!-- page 2 -->\n\nthird\n"
    )


def test_formula_number_becomes_a_tag_unless_the_latex_has_one():
    numbered = Block("f", BlockType.FORMULA, BOX, 0, latex="a=b", number="(3)")
    assert to_markdown(document([numbered])) == "$$\na=b \\tag{3}\n$$\n"
    tagged = Block("g", BlockType.FORMULA, BOX, 0, latex="a=b \\tag{9}", number="3")
    assert to_markdown(document([tagged])) == "$$\na=b \\tag{9}\n$$\n"


def test_inline_math_padding_is_removed():
    doc = document([para("p", "at $ P_1 $ and $5 and $6")])
    assert to_markdown(doc) == "at $P_1$ and $5 and $6\n"
