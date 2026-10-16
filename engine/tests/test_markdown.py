from opendoc_engine.exporters import to_markdown
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
