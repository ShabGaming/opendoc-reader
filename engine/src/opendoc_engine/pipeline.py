"""The shared page pipeline behind `convert.page`: everything around an engine's `run`.

An engine turns a page image into blocks and knows nothing else. This module opens the image,
prepares the engine, renumbers the blocks so ids and reading order follow the protocol, writes
the figure crops, checks for cancellation and writes the IR `Page` file.
"""

from __future__ import annotations

import logging
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from PIL import Image

from opendoc_engine import __version__
from opendoc_engine.engines import EngineRegistry, EngineSetup
from opendoc_engine.errors import bad_input, invalid_params, map_exception
from opendoc_engine.files import write_json, write_png
from opendoc_engine.inputs import open_image
from opendoc_engine.ir import BBox, Block, BlockType, EngineInfo, Page
from opendoc_engine.rpc import CallContext, RpcError
from opendoc_engine.runtime import Settings

log = logging.getLogger(__name__)

# A figure smaller than this in either direction is noise, not something to crop.
_MIN_CROP_PIXELS = 4


@dataclass
class PageResult:
    ir_path: str
    block_count: int
    engine: EngineInfo
    elapsed_ms: int
    warnings: list[str]

    def to_dict(self) -> dict[str, Any]:
        return {
            "irPath": self.ir_path,
            "blockCount": self.block_count,
            "engine": self.engine.to_dict(),
            "elapsedMs": self.elapsed_ms,
            "warnings": self.warnings,
        }


def load_page_image(path: str) -> Image.Image:
    """The page image as RGB. Raises BAD_INPUT when it cannot be read."""
    with open_image(path) as image:
        try:
            image.load()
            return image.convert("RGB")
        except (OSError, SyntaxError, ValueError) as exc:
            raise bad_input("corrupt", f"{path}: {exc}") from exc


def _clamp(box: BBox, width: int, height: int) -> BBox:
    x1, y1, x2, y2 = box
    x1, x2 = sorted((min(max(x1, 0.0), width), min(max(x2, 0.0), width)))
    y1, y2 = sorted((min(max(y1, 0.0), height), min(max(y2, 0.0), height)))
    return (x1, y1, x2, y2)


def renumber(blocks: list[Block], page_index: int, width: int, height: int) -> list[Block]:
    """Give blocks their protocol ids (`p{page}-b{n}`, cells `-r{row}c{col}`), make `order`
    equal the position, keep boxes on the page and re-point caption links."""
    mapping: dict[str, str] = {}
    for order, block in enumerate(blocks):
        new_id = f"p{page_index}-b{order}"
        mapping[block.id] = new_id
        block.id = new_id
        block.order = order
        block.bbox = _clamp(block.bbox, width, height)
        if block.table is not None:
            for cell in block.table.cells:
                for position, inner in enumerate(cell.blocks):
                    suffix = f"-{position}" if position else ""
                    inner.id = f"{new_id}-r{cell.row}c{cell.col}{suffix}"
                    inner.order = position
                    inner.bbox = _clamp(inner.bbox, width, height)
    for block in blocks:
        if block.caption_for is not None:
            block.caption_for = mapping.get(block.caption_for)
    return blocks


def drop_tiny_figures(blocks: list[Block], warn: Any) -> list[Block]:
    """Remove figures too small to crop. The caller renumbers afterwards."""
    kept = []
    for block in blocks:
        x1, y1, x2, y2 = block.bbox
        if block.type is BlockType.FIGURE and (
            x2 - x1 < _MIN_CROP_PIXELS or y2 - y1 < _MIN_CROP_PIXELS
        ):
            warn(f"figure {block.id} is too small to crop; dropped")
            continue
        kept.append(block)
    return kept


def write_crops(blocks: list[Block], image: Image.Image, crops_dir: str, page_index: int) -> None:
    """Crop every figure block from the page image to `<cropsDir>/p<page>-b<n>.png` and record
    the path on the block."""
    for block in blocks:
        if block.type is not BlockType.FIGURE:
            continue
        x1, y1, x2, y2 = (round(v) for v in block.bbox)
        path = Path(crops_dir) / f"p{page_index}-b{block.order}.png"
        write_png(path, image.crop((x1, y1, x2, y2)))
        block.figure_crop = str(path)


def convert_page(
    engines: EngineRegistry,
    settings: Settings,
    *,
    engine_id: str,
    model_id: str,
    image_path: str,
    page_index: int,
    ir_path: str,
    crops_dir: str,
    options: dict[str, Any],
    call: CallContext,
) -> PageResult:
    """Convert one page image and write its IR. Raises `RpcError` for every failure."""
    started = time.monotonic()
    try:
        engine = engines.get(engine_id)
    except KeyError as exc:
        raise invalid_params("unknown_engine", "engineId") from exc

    call.page_index = page_index
    call.check_cancelled()
    image = load_page_image(image_path)
    width, height = image.size
    if options.get("language"):
        call.warn(f"language option {options['language']!r} is not used by {engine.id}")

    call.stage("prepare")
    try:
        engine.prepare(
            EngineSetup(
                model_id=model_id,
                models_dir=settings.models_dir,
                llama_base_url=settings.llama_server_url and settings.llama_base_url,
                cpu_threads=settings.cpu_threads,
                scratch_dir=settings.scratch_dir,
            )
        )
        blocks = engine.run(image_path, (width, height), call)
    except RpcError:
        raise
    except Exception as exc:
        log.exception("engine %s failed", engine.id)
        raise map_exception(exc) from exc

    blocks = renumber(blocks, page_index, width, height)
    kept = drop_tiny_figures(blocks, call.warn)
    if len(kept) != len(blocks):
        blocks = renumber(kept, page_index, width, height)
    call.stage("crops")
    write_crops(blocks, image, crops_dir, page_index)

    call.check_cancelled()
    page = Page(page_index, width, height, str(Path(image_path)), blocks)
    write_json(ir_path, page.to_dict())
    info = EngineInfo(engine.id, model_id, __version__)
    return PageResult(
        ir_path=ir_path,
        block_count=len(blocks),
        engine=info,
        elapsed_ms=round((time.monotonic() - started) * 1000),
        warnings=list(call.warnings),
    )
