"""HTML table -> IR `Table`.

DeepSeek-OCR and PaddleOCR-VL both emit tables as HTML with `rowspan` and `colspan`. This
turns one `<table>` into grid-anchored IR cells. HTML carries no per-cell geometry, so every
cell's paragraph block reuses the table's own bounding box.
"""

from __future__ import annotations

from dataclasses import dataclass
from html.parser import HTMLParser

from opendoc_engine.ir import BBox, Block, BlockType, Table, TableCell


@dataclass
class _RawCell:
    header: bool
    row_span: int
    col_span: int
    text: str = ""


def _span(attrs: list[tuple[str, str | None]], name: str) -> int:
    for key, value in attrs:
        if key == name and value and value.strip().isdigit():
            return max(1, int(value))
    return 1


class _TableParser(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.rows: list[list[_RawCell]] = []
        self._cell: _RawCell | None = None
        self._depth = 0

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        if tag == "table":
            self._depth += 1
        elif self._depth != 1:
            return  # content of a nested table is flattened into its parent cell
        elif tag == "tr":
            self.rows.append([])
        elif tag in ("td", "th"):
            if not self.rows:
                self.rows.append([])
            self._cell = _RawCell(tag == "th", _span(attrs, "rowspan"), _span(attrs, "colspan"))
            self.rows[-1].append(self._cell)
        elif tag == "br" and self._cell is not None:
            self._cell.text += "\n"

    def handle_endtag(self, tag: str) -> None:
        if tag == "table":
            self._depth -= 1
        elif tag in ("td", "th") and self._depth == 1:
            self._cell = None

    def handle_data(self, data: str) -> None:
        if self._cell is not None:
            self._cell.text += data


def _clean(text: str) -> str:
    lines = (" ".join(line.split()) for line in text.split("\n"))
    return "\n".join(line for line in lines if line)


def parse_html_table(html: str, bbox: BBox, id_prefix: str) -> Table | None:
    """Parse the first `<table>` in `html`. Returns None when there is no table with cells."""
    parser = _TableParser()
    parser.feed(html)
    parser.close()
    if not any(parser.rows):
        return None

    row_count = len(parser.rows)
    occupied: set[tuple[int, int]] = set()
    cells: list[TableCell] = []
    col_count = 0
    for r, raw_row in enumerate(parser.rows):
        c = 0
        for raw in raw_row:
            while (r, c) in occupied:
                c += 1
            row_span = min(raw.row_span, row_count - r)
            for dr in range(row_span):
                for dc in range(raw.col_span):
                    occupied.add((r + dr, c + dc))
            text = _clean(raw.text)
            blocks = (
                [Block(f"{id_prefix}-r{r}c{c}", BlockType.PARAGRAPH, bbox, 0, text=text)]
                if text
                else []
            )
            cells.append(TableCell(r, c, row_span, raw.col_span, raw.header, blocks))
            c += raw.col_span
            col_count = max(col_count, c)
    return Table(rows=row_count, cols=col_count, cells=cells)
