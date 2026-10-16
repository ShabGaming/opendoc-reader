"""A deterministic stand-in engine. It is NOT an OCR engine.

`DemoEngine` ignores the page image and returns the same few blocks, laid out as fractions of
the page size. It exists so the RPC surface, the registry and the exporters can be exercised
(in tests and through `engine.capabilities`) before any real engine is wired in. Block ids are
local to the engine (`demo-b0`, ...); the pipeline is responsible for making them unique per
document if it needs that.
"""

from __future__ import annotations

from collections.abc import Mapping
from pathlib import Path
from typing import Any

from opendoc_engine.engines.base import Capabilities
from opendoc_engine.ir import Block, BlockType, Table, TableCell


def _box(size: tuple[int, int], x1: float, y1: float, x2: float, y2: float):
    w, h = size
    return (round(x1 * w, 1), round(y1 * h, 1), round(x2 * w, 1), round(y2 * h, 1))


class DemoEngine:
    id = "demo"
    display_name = "Demo (fixed output)"
    capabilities = Capabilities(
        tables=True,
        formulas=True,
        figures=False,
        reading_order=True,
        bounding_boxes=True,
        language_handling="none",
    )
    manifest_models: tuple[str, ...] = ()

    def prepare(self, options: Mapping[str, Any] | None = None) -> None:
        return None

    def run(self, page_image_path: Path | str, page_size: tuple[int, int]) -> list[Block]:
        cells = [
            TableCell(
                0, 0, col_span=2, header=True, blocks=[self._cell("demo-t0", page_size, "Totals")]
            ),
            TableCell(1, 0, blocks=[self._cell("demo-t1", page_size, "Net")]),
            TableCell(1, 1, blocks=[self._cell("demo-t2", page_size, "100")]),
        ]
        return [
            Block(
                "demo-b0",
                BlockType.HEADING,
                _box(page_size, 0.1, 0.05, 0.9, 0.1),
                0,
                level=1,
                text="Demo document",
            ),
            Block(
                "demo-b1",
                BlockType.PARAGRAPH,
                _box(page_size, 0.1, 0.12, 0.9, 0.2),
                1,
                text="This text is produced by the demo engine, not by recognition.",
            ),
            Block(
                "demo-b2",
                BlockType.TABLE,
                _box(page_size, 0.1, 0.25, 0.9, 0.4),
                2,
                table=Table(rows=2, cols=2, cells=cells),
            ),
            Block(
                "demo-b3",
                BlockType.FORMULA,
                _box(page_size, 0.3, 0.45, 0.7, 0.5),
                3,
                latex="E = mc^2",
            ),
        ]

    def release(self) -> None:
        return None

    @staticmethod
    def _cell(block_id: str, size: tuple[int, int], text: str) -> Block:
        return Block(block_id, BlockType.PARAGRAPH, _box(size, 0.1, 0.25, 0.9, 0.4), 0, text=text)
