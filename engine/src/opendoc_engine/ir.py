"""Document IR: the one model every engine produces and every exporter reads.

The shapes follow the sketch in docs/architecture.md. JSON keys are camelCase, Python
attributes are snake_case, and optional fields are omitted from JSON when unset. All
coordinates are in pixels of the rendered page image, as `(x1, y1, x2, y2)`.

`from_dict` is strict about structure (missing required keys and unknown schema versions
raise `ValueError`) so that a corrupted or newer file fails loudly instead of being half read.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import StrEnum
from typing import Any, Literal

IR_SCHEMA_VERSION = "1"

BBox = tuple[float, float, float, float]


class BlockType(StrEnum):
    HEADING = "heading"
    PARAGRAPH = "paragraph"
    LIST = "list"
    TABLE = "table"
    FORMULA = "formula"
    FIGURE = "figure"
    CAPTION = "caption"
    HEADER = "header"
    FOOTER = "footer"
    FOOTNOTE = "footnote"
    PAGE_NUMBER = "page_number"
    OTHER = "other"


def _get(data: dict[str, Any], key: str, where: str) -> Any:
    try:
        return data[key]
    except (KeyError, TypeError):
        raise ValueError(f"{where}: missing required field {key!r}") from None


def _bbox(value: Any, where: str) -> BBox:
    if not isinstance(value, (list, tuple)) or len(value) != 4:
        raise ValueError(f"{where}: bbox must be a list of 4 numbers")
    x1, y1, x2, y2 = (float(v) for v in value)
    return (x1, y1, x2, y2)


@dataclass
class ListItem:
    """One list entry. Nesting is expressed through `children`."""

    text: str
    ordered: bool = False
    children: list[ListItem] = field(default_factory=list)

    def to_dict(self) -> dict[str, Any]:
        data: dict[str, Any] = {"text": self.text, "ordered": self.ordered}
        if self.children:
            data["children"] = [c.to_dict() for c in self.children]
        return data

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> ListItem:
        return cls(
            text=_get(data, "text", "list item"),
            ordered=bool(data.get("ordered", False)),
            children=[cls.from_dict(c) for c in data.get("children", [])],
        )


@dataclass
class TableCell:
    """A cell anchored at (`row`, `col`) and spanning `row_span` x `col_span` grid slots."""

    row: int
    col: int
    row_span: int = 1
    col_span: int = 1
    header: bool = False
    blocks: list[Block] = field(default_factory=list)

    def to_dict(self) -> dict[str, Any]:
        data: dict[str, Any] = {
            "row": self.row,
            "col": self.col,
            "rowSpan": self.row_span,
            "colSpan": self.col_span,
        }
        if self.header:
            data["header"] = True
        data["blocks"] = [b.to_dict() for b in self.blocks]
        return data

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> TableCell:
        return cls(
            row=int(_get(data, "row", "table cell")),
            col=int(_get(data, "col", "table cell")),
            row_span=int(data.get("rowSpan", 1)),
            col_span=int(data.get("colSpan", 1)),
            header=bool(data.get("header", False)),
            blocks=[Block.from_dict(b) for b in data.get("blocks", [])],
        )


@dataclass
class Table:
    rows: int
    cols: int
    cells: list[TableCell] = field(default_factory=list)

    @property
    def has_merged_cells(self) -> bool:
        return any(c.row_span > 1 or c.col_span > 1 for c in self.cells)

    def to_dict(self) -> dict[str, Any]:
        return {
            "rows": self.rows,
            "cols": self.cols,
            "cells": [c.to_dict() for c in self.cells],
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> Table:
        return cls(
            rows=int(_get(data, "rows", "table")),
            cols=int(_get(data, "cols", "table")),
            cells=[TableCell.from_dict(c) for c in data.get("cells", [])],
        )


@dataclass
class Block:
    """A layout element on a page. `figure_crop` and `caption_for` flatten the nested
    `figure: {crop}` and `caption: {for}` objects of the JSON form."""

    id: str
    type: BlockType
    bbox: BBox
    order: int
    level: int | None = None
    text: str | None = None
    items: list[ListItem] | None = None
    table: Table | None = None
    latex: str | None = None
    figure_crop: str | None = None
    caption_for: str | None = None
    confidence: float | None = None

    def to_dict(self) -> dict[str, Any]:
        data: dict[str, Any] = {
            "id": self.id,
            "type": self.type.value,
            "bbox": list(self.bbox),
            "order": self.order,
        }
        if self.level is not None:
            data["level"] = self.level
        if self.text is not None:
            data["text"] = self.text
        if self.items is not None:
            data["items"] = [i.to_dict() for i in self.items]
        if self.table is not None:
            data["table"] = self.table.to_dict()
        if self.latex is not None:
            data["latex"] = self.latex
        if self.figure_crop is not None:
            data["figure"] = {"crop": self.figure_crop}
        if self.caption_for is not None:
            data["caption"] = {"for": self.caption_for}
        if self.confidence is not None:
            data["confidence"] = self.confidence
        return data

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> Block:
        where = f"block {data.get('id', '?')}"
        table = data.get("table")
        items = data.get("items")
        return cls(
            id=_get(data, "id", where),
            type=BlockType(_get(data, "type", where)),
            bbox=_bbox(_get(data, "bbox", where), where),
            order=int(_get(data, "order", where)),
            level=data.get("level"),
            text=data.get("text"),
            items=None if items is None else [ListItem.from_dict(i) for i in items],
            table=None if table is None else Table.from_dict(table),
            latex=data.get("latex"),
            figure_crop=data.get("figure", {}).get("crop"),
            caption_for=data.get("caption", {}).get("for"),
            confidence=data.get("confidence"),
        )


@dataclass
class Page:
    """One rendered page. `blocks` are stored in reading order."""

    index: int
    width: int
    height: int
    source_image: str
    blocks: list[Block] = field(default_factory=list)

    def to_dict(self) -> dict[str, Any]:
        return {
            "index": self.index,
            "width": self.width,
            "height": self.height,
            "sourceImage": self.source_image,
            "blocks": [b.to_dict() for b in self.blocks],
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> Page:
        return cls(
            index=int(_get(data, "index", "page")),
            width=int(_get(data, "width", "page")),
            height=int(_get(data, "height", "page")),
            source_image=_get(data, "sourceImage", "page"),
            blocks=[Block.from_dict(b) for b in data.get("blocks", [])],
        )


@dataclass
class Source:
    path: str
    kind: Literal["pdf", "image"]
    page_count: int

    def to_dict(self) -> dict[str, Any]:
        return {"path": self.path, "kind": self.kind, "pageCount": self.page_count}

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> Source:
        kind = _get(data, "kind", "source")
        if kind not in ("pdf", "image"):
            raise ValueError(f"source: unknown kind {kind!r}")
        return cls(
            path=_get(data, "path", "source"),
            kind=kind,
            page_count=int(_get(data, "pageCount", "source")),
        )


@dataclass
class EngineInfo:
    """Which engine and model produced the document."""

    id: str
    model: str
    version: str

    def to_dict(self) -> dict[str, Any]:
        return {"id": self.id, "model": self.model, "version": self.version}

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> EngineInfo:
        return cls(
            id=_get(data, "id", "engine"),
            model=_get(data, "model", "engine"),
            version=_get(data, "version", "engine"),
        )


@dataclass
class Document:
    source: Source
    engine: EngineInfo
    pages: list[Page] = field(default_factory=list)
    schema_version: str = IR_SCHEMA_VERSION

    def to_dict(self) -> dict[str, Any]:
        return {
            "schemaVersion": self.schema_version,
            "source": self.source.to_dict(),
            "engine": self.engine.to_dict(),
            "pages": [p.to_dict() for p in self.pages],
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> Document:
        version = _get(data, "schemaVersion", "document")
        if version != IR_SCHEMA_VERSION:
            raise ValueError(
                f"unsupported IR schemaVersion {version!r}; this engine reads {IR_SCHEMA_VERSION!r}"
            )
        return cls(
            schema_version=version,
            source=Source.from_dict(_get(data, "source", "document")),
            engine=EngineInfo.from_dict(_get(data, "engine", "document")),
            pages=[Page.from_dict(p) for p in data.get("pages", [])],
        )
