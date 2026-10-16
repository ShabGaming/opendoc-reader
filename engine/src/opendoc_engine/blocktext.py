"""Keeps block-level Markdown out of IR text.

IR `text` is inline Markdown: emphasis, links, inline code and `$inline math$`, nothing more.
Recognition models answer in full Markdown, so a heading arrives as `## Title`, a list as
`- a` lines, a centred caption as `<div style="text-align: center;">...</div>`. This module
turns each of those into the IR structure it stands for (a heading block with a level, a list
block, a formula block, a table) or into plain text, before the blocks leave the engine.

`normalize_blocks` is for text a model wrote as Markdown. `escape_inline` is for text that is
literal (OCR lines, a PDF text layer), where `*` or `$` must not be read as markup.
`strip_heading_markers` and `strip_html` are the small defensive pieces the exporters reuse.
"""

from __future__ import annotations

import re
from copy import copy
from dataclasses import dataclass, field

from opendoc_engine.equations import dollar_math
from opendoc_engine.ir import Block, BlockType, ListItem, Table, TableCell

_HEADING_LINE = re.compile(r"^\s{0,3}(#{1,6})(?:[ \t]+(.*?))?(?:[ \t]+#+)?\s*$")
_HEADING_PREFIX = re.compile(r"^\s{0,3}#{1,6}(?:[ \t]+|$)")
_BULLET_LINE = re.compile(r"^(\s*)[-*+•·]\s+(\S.*)$")
_ORDERED_LINE = re.compile(r"^(\s*)(\d{1,3})[.)]\s+(\S.*)$")
_QUOTE_LINE = re.compile(r"^\s{0,3}>\s?(.*)$")
_TABLE_ROW = re.compile(r"^\s*\|.*\|\s*$")
_TABLE_RULE = re.compile(r"^\s*\|?\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)*\|?\s*$")
_FENCE = re.compile(r"^\s*(```|~~~)")
_DISPLAY_MATH = re.compile(r"^\s*(?:\$\$(.+?)\$\$|\\\[(.+?)\\\])\s*$", re.DOTALL)
_DISPLAY_INLINE = re.compile(r"\$\$(.+?)\$\$", re.DOTALL)

_BREAK = re.compile(r"<br\s*/?>", re.IGNORECASE)
_STRONG = re.compile(r"</?(?:b|strong)\b[^>]*>", re.IGNORECASE)
_EMPHASIS = re.compile(r"</?(?:i|em)\b[^>]*>", re.IGNORECASE)
_WRAPPER = re.compile(
    r"</?(?:div|p|span|center|font|section|article|u|small|big|mark)\b[^>]*>", re.IGNORECASE
)

_STRUCTURED = {BlockType.PARAGRAPH, BlockType.HEADING, BlockType.OTHER}
_FLAT = {
    BlockType.CAPTION,
    BlockType.FOOTNOTE,
    BlockType.HEADER,
    BlockType.FOOTER,
    BlockType.PAGE_NUMBER,
}

_INLINE_SPECIALS = re.compile(r"([\\*_$`\[\]])")


def escape_inline(text: str) -> str:
    """Backslash-escape the characters inline Markdown would read as markup."""
    return _INLINE_SPECIALS.sub(r"\\\1", text)


def strip_html(text: str) -> str:
    """Remove the HTML models put around text: line breaks become spaces, bold and italic tags
    become `**` and `*`, layout wrappers (`div`, `center`, `span`, ...) disappear."""
    if "<" not in text:
        return text
    text = _BREAK.sub(" ", text)
    text = _STRONG.sub("**", text)
    text = _EMPHASIS.sub("*", text)
    return _WRAPPER.sub(" ", text)


def strip_heading_markers(text: str) -> str:
    """The text without leading `#` markers, however many times they were stacked."""
    while True:
        match = _HEADING_PREFIX.match(text)
        if not match:
            return text.strip()
        text = text[match.end() :]


def inline_text(text: str) -> str:
    """One line of inline Markdown: HTML removed, `\\( \\)` and display `$$` math written as
    `$...$`, whitespace collapsed."""
    text = dollar_math(strip_html(text))
    text = _DISPLAY_INLINE.sub(lambda m: f"${m.group(1).strip()}$", text)
    return " ".join(text.split())


_LEADING_MARKER = re.compile(r"^\s{0,3}#{1,6}(?:[ \t]+|$)")
_LEADING_BULLET = re.compile(r"^[-*+•·][ \t]+")
_LEADING_NUMBER = re.compile(r"^\d{1,3}[.)][ \t]+")
_LITERAL_LEAD = re.compile(r"^(\s{0,3})(#{1,6}\s|[-*+]\s|>|\|)")
_LITERAL_NUMBER = re.compile(r"^(\s{0,3}\d{1,3})([.)]\s)")


def export_text(text: str, *, heading: bool = False, item: str | None = None) -> str:
    """What an exporter should print for IR text: the text with HTML and any block marker that
    still got through removed. `item` is "bullet" or "ordered" for list entries, whose own
    marker would otherwise print twice. A numbered paragraph ("3. Text") and a quote mark stay."""
    text = strip_html(text)
    if heading:
        return strip_heading_markers(text)
    while _LEADING_MARKER.match(text):
        text = _LEADING_MARKER.sub("", text, count=1)
    if item == "bullet":
        text = _LEADING_BULLET.sub("", text, count=1)
    elif item == "ordered":
        text = _LEADING_NUMBER.sub("", text, count=1)
    return " ".join(text.split()) if "\n" in text else text.strip()


def literal_lead(text: str) -> str:
    """Escape a leading `#`, `-`, `>`, `|` or `1.` so Markdown shows paragraph text as written."""
    match = _LITERAL_NUMBER.match(text)
    if match:
        return f"{match.group(1)}\\{text[len(match.group(1)) :]}"
    match = _LITERAL_LEAD.match(text)
    if match:
        pos = len(match.group(1))
        return f"{text[:pos]}\\{text[pos:]}"
    return text


@dataclass
class _Part:
    kind: str  # heading, para, list, table, formula
    text: str = ""
    level: int = 0
    items: list[ListItem] = field(default_factory=list)
    rows: list[list[str]] = field(default_factory=list)


@dataclass
class _Entry:
    indent: int
    ordered: bool
    text: str
    raw: str


def _split_cells(line: str) -> list[str]:
    cells = re.split(r"(?<!\\)\|", line.strip())
    if cells and not cells[0].strip():
        cells = cells[1:]
    if cells and not cells[-1].strip():
        cells = cells[:-1]
    return [c.strip().replace("\\|", "|") for c in cells]


def _item_tree(entries: list[_Entry]) -> list[ListItem]:
    """Nest items by indent: a deeper indent than the item before makes a child."""
    root: list[ListItem] = []
    stack: list[tuple[int, list[ListItem]]] = [(-1, root)]
    for entry in entries:
        while len(stack) > 1 and entry.indent <= stack[-1][0]:
            stack.pop()
        item = ListItem(entry.text, entry.ordered)
        stack[-1][1].append(item)
        stack.append((entry.indent, item.children))
    return root


class _Splitter:
    """Reads Markdown text line by line into parts."""

    def __init__(self, *, structure: bool) -> None:
        self.structure = structure
        self.parts: list[_Part] = []
        self.para: list[str] = []
        self.entries: list[_Entry] = []
        self.rows: list[str] = []
        self.fence: list[str] | None = None

    def run(self, text: str) -> list[_Part]:
        for line in text.split("\n"):
            self.line(line)
        if self.fence is not None:
            self.close_fence()
        self.flush()
        return self.parts

    def close_fence(self) -> None:
        body = " ".join(self.fence or []).strip()
        if body:
            self.parts.append(_Part("para", "`" + body.replace("`", "'") + "`"))
        self.fence = None

    def line(self, line: str) -> None:
        if self.fence is not None:
            if _FENCE.match(line):
                self.close_fence()
            else:
                self.fence.append(line.strip())
            return
        if _FENCE.match(line):
            self.flush()
            self.fence = []
        elif not line.strip():
            self.flush()
        elif _TABLE_ROW.match(line):
            self.flush_list()
            self.flush_para()
            self.rows.append(line)
        else:
            self.flush_rows()
            self.text_line(line)

    def text_line(self, line: str) -> None:
        heading = _HEADING_LINE.match(line) if self.structure else None
        bullet = _BULLET_LINE.match(line) if self.structure else None
        ordered = _ORDERED_LINE.match(line) if self.structure else None
        quote = _QUOTE_LINE.match(line)
        if heading:
            self.flush()
            text = inline_text(strip_heading_markers(heading.group(2) or ""))
            if text:
                self.parts.append(_Part("heading", text, len(heading.group(1))))
        elif bullet:
            self.flush_para()
            indent = len(bullet.group(1).expandtabs(4))
            self.entries.append(_Entry(indent, False, inline_text(bullet.group(2)), line))
        elif ordered and (self.entries or int(ordered.group(2)) == 1):
            # Numbered lines count as a list from 1; a lone "3. text" stays text, so a numbered
            # paragraph keeps its number.
            self.flush_para()
            indent = len(ordered.group(1).expandtabs(4))
            self.entries.append(_Entry(indent, True, inline_text(ordered.group(3)), line))
        elif quote:
            self.flush_list()
            self.para.append(quote.group(1))
        else:
            self.flush_list()
            self.para.append(line)

    def flush_para(self) -> None:
        if self.para:
            text = inline_text(" ".join(self.para))
            if text:
                self.parts.append(_Part("para", text))
            self.para.clear()

    def flush_list(self) -> None:
        entries, self.entries = self.entries, []
        if len(entries) == 1 and entries[0].ordered:
            self.para.append(entries[0].raw.strip())
        elif entries:
            self.flush_para()
            self.parts.append(_Part("list", items=_item_tree(entries)))

    def flush_rows(self) -> None:
        rows, self.rows = self.rows, []
        if not rows:
            return
        grid = [_split_cells(r) for r in rows if not _TABLE_RULE.match(r)]
        if self.structure and len(grid) >= 2:
            self.parts.append(_Part("table", rows=grid))
        else:
            self.para.extend(" ".join(c for c in row if c) for row in grid)

    def flush(self) -> None:
        self.flush_rows()
        self.flush_list()
        self.flush_para()


def _split(text: str, *, structure: bool) -> list[_Part]:
    whole = _DISPLAY_MATH.match(text)
    if structure and whole and "$$" not in (whole.group(1) or ""):
        return [_Part("formula", (whole.group(1) or whole.group(2)).strip())]
    return _Splitter(structure=structure).run(text)


def _table(rows: list[list[str]], box: tuple[float, float, float, float], prefix: str) -> Table:
    cols = max(len(r) for r in rows)
    cells = []
    for r, row in enumerate(rows):
        for c in range(cols):
            text = inline_text(row[c]) if c < len(row) else ""
            blocks = [Block(f"{prefix}-r{r}c{c}", BlockType.PARAGRAPH, box, 0, text=text)]
            cells.append(TableCell(r, c, header=r == 0, blocks=blocks if text else []))
    return Table(len(rows), cols, cells)


def _block_for(part: _Part, source: Block, number: int) -> Block:
    """A block for `part`, carrying the source block's box and id (suffixed after the first)."""
    block_id = source.id if number == 0 else f"{source.id}-s{number}"
    base = Block(block_id, BlockType.PARAGRAPH, source.bbox, source.order)
    if number == 0:
        base.lines, base.confidence = source.lines, source.confidence
    if part.kind == "heading":
        base.type, base.text = BlockType.HEADING, part.text
        base.level = part.level
    elif part.kind == "list":
        base.type, base.items = BlockType.LIST, part.items
    elif part.kind == "table":
        base.type, base.table = BlockType.TABLE, _table(part.rows, source.bbox, block_id)
    elif part.kind == "formula":
        base.type, base.latex = BlockType.FORMULA, part.text
    else:
        base.type = source.type if source.type is not BlockType.HEADING else BlockType.PARAGRAPH
        base.text = part.text
    return base


def _structured(block: Block) -> list[Block]:
    heading = block.type is BlockType.HEADING
    parts = _split(block.text or "", structure=True)
    if not parts:
        return []
    if heading and parts[0].kind == "para":
        # A title block's own text, with no marker: it stays the heading.
        parts[0] = _Part("heading", parts[0].text, 0)
    out = []
    for number, part in enumerate(parts):
        new = _block_for(part, block, number)
        if new.type is BlockType.HEADING:
            if heading and number == 0:
                new.level = block.level if block.level else (part.level or 1)
            else:
                new.level = part.level or block.level or 2
        out.append(new)
    return out


def _flat(block: Block) -> Block:
    """Captions, footnotes and page furniture keep their type; markers become plain text."""
    parts = _split(block.text or "", structure=False)
    text = " ".join(p.text for p in parts if p.kind == "para")
    block.text = strip_heading_markers(text) if text else None
    return block


def normalize_blocks(blocks: list[Block]) -> list[Block]:
    """Blocks whose text is clean inline Markdown. A block may become several (a paragraph
    holding a heading line and a list); the first keeps the original id and the rest get
    `-s1`, `-s2`, ... Table cells are cleaned in place."""
    out: list[Block] = []
    for block in blocks:
        if block.table is not None:
            for cell in block.table.cells:
                for inner in cell.blocks:
                    if inner.text:
                        inner.text = inline_text(inner.text)
        if block.items:
            _clean_items(block.items)
        if block.text and block.type in _STRUCTURED:
            out.extend(_structured(copy(block)))
        elif block.text and block.type in _FLAT:
            _flat(block)
            if block.text:
                out.append(block)
        else:
            out.append(block)
    return out


def _clean_items(items: list[ListItem]) -> None:
    for item in items:
        item.text = inline_text(item.text)
        _clean_items(item.children)
