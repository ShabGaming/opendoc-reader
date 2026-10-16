"""IR -> Markdown.

Output rules:

- Headings use `#` repeated `level` times (1 to 6).
- Tables are GFM when no cell is merged and HTML when any cell spans more than one slot.
- Display formulas are `$$` fenced blocks.
- Figures are `![](assets/<file>)`. The file name is the basename of the block's crop, or
  `<block id>.png` when it has none; the pipeline copies crops into the assets folder under
  those names.
- Pages are separated by a blank line and a `<!-- page N -->` comment (N is 1-based).
- Page furniture (header, footer, page number) is omitted.
"""

from __future__ import annotations

import html
from pathlib import PureWindowsPath

from opendoc_engine.ir import Block, BlockType, Document, ListItem, Table

ASSETS_DIR = "assets"

_OMITTED = {BlockType.HEADER, BlockType.FOOTER, BlockType.PAGE_NUMBER}


def to_markdown(document: Document) -> str:
    parts: list[str] = []
    for i, page in enumerate(document.pages):
        if i > 0:
            parts.append(f"<!-- page {page.index + 1} -->")
        for block in sorted(page.blocks, key=lambda b: b.order):
            rendered = _render_block(block)
            if rendered:
                parts.append(rendered)
    return "\n\n".join(parts) + "\n" if parts else ""


def _render_block(block: Block) -> str:
    kind = block.type
    if kind in _OMITTED:
        return ""
    if kind is BlockType.HEADING:
        level = min(max(block.level or 1, 1), 6)
        return f"{'#' * level} {block.text}" if block.text else ""
    if kind is BlockType.LIST:
        return "\n".join(_list_lines(block.items or []))
    if kind is BlockType.TABLE:
        return _render_table(block.table) if block.table else ""
    if kind is BlockType.FORMULA:
        body = block.latex if block.latex is not None else block.text
        return f"$$\n{body}\n$$" if body else ""
    if kind is BlockType.FIGURE:
        return f"![]({ASSETS_DIR}/{_figure_name(block)})"
    if kind is BlockType.CAPTION:
        return f"*{block.text}*" if block.text else ""
    return block.text or ""  # paragraph, footnote, other


def _figure_name(block: Block) -> str:
    if block.figure_crop:
        # PureWindowsPath splits on both / and \, so it handles either style of path.
        return PureWindowsPath(block.figure_crop).name
    return f"{block.id}.png"


def _list_lines(items: list[ListItem], depth: int = 0) -> list[str]:
    lines: list[str] = []
    for number, item in enumerate(items, start=1):
        marker = f"{number}." if item.ordered else "-"
        lines.append(f"{'   ' * depth}{marker} {item.text}")
        lines.extend(_list_lines(item.children, depth + 1))
    return lines


def _cell_texts(blocks: list[Block]) -> list[str]:
    return [b.text for b in blocks if b.text]


def _render_table(table: Table) -> str:
    if table.rows == 0 or table.cols == 0:
        return ""
    return _html_table(table) if table.has_merged_cells else _gfm_table(table)


def _gfm_table(table: Table) -> str:
    grid = [[""] * table.cols for _ in range(table.rows)]
    for cell in table.cells:
        if cell.row < table.rows and cell.col < table.cols:
            text = "<br>".join(t.replace("\n", "<br>") for t in _cell_texts(cell.blocks))
            grid[cell.row][cell.col] = text.replace("|", "\\|")
    lines = ["| " + " | ".join(row) + " |" for row in grid]
    lines.insert(1, "| " + " | ".join(["---"] * table.cols) + " |")
    return "\n".join(lines)


def _html_table(table: Table) -> str:
    rows: list[list[str]] = [[] for _ in range(table.rows)]
    for cell in sorted(table.cells, key=lambda c: (c.row, c.col)):
        if cell.row >= table.rows:
            continue
        tag = "th" if cell.header else "td"
        attrs = ""
        if cell.row_span > 1:
            attrs += f' rowspan="{cell.row_span}"'
        if cell.col_span > 1:
            attrs += f' colspan="{cell.col_span}"'
        text = "<br>".join(
            html.escape(t, quote=False).replace("\n", "<br>") for t in _cell_texts(cell.blocks)
        )
        rows[cell.row].append(f"<{tag}{attrs}>{text}</{tag}>")
    lines = ["<table>"]
    lines.extend(f"<tr>{''.join(cells)}</tr>" for cells in rows)
    lines.append("</table>")
    return "\n".join(lines)
