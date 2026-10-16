"""The engine seam: what a recognition engine provides, and where engines are registered.

An engine turns one preprocessed page image into IR blocks. It does not render PDFs, rotate
pages, write files or know about exporters; the shared pipeline does all of that. All
model-specific parsing (for example DeepSeek-OCR grounding output, see `grounding.py`) lives
behind `run`.

Requirements for the hardware suggester are data in `models/manifest.json`, not code. An
engine only names the manifest model entries it needs through `manifest_models`.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any, Literal, Protocol, runtime_checkable

from opendoc_engine.ir import Block
from opendoc_engine.rpc import CallContext


@dataclass(frozen=True)
class Capabilities:
    """What an engine can recognize. `language_handling` is how the engine deals with the
    page language: detected by the model, chosen by the user, or not applicable."""

    tables: bool = False
    formulas: bool = False
    figures: bool = False
    reading_order: bool = False
    bounding_boxes: bool = False
    language_handling: Literal["auto", "selectable", "none"] = "none"

    def to_dict(self) -> dict[str, Any]:
        return {
            "tables": self.tables,
            "formulas": self.formulas,
            "figures": self.figures,
            "readingOrder": self.reading_order,
            "boundingBoxes": self.bounding_boxes,
            "languageHandling": self.language_handling,
        }


@dataclass(frozen=True)
class EngineSetup:
    """The runtime facts an engine needs, taken from `engine.configure` and the call.
    `llama_base_url` is the server's base URL without `/v1`, or None when none is running."""

    model_id: str
    models_dir: Path
    llama_base_url: str | None
    cpu_threads: int
    scratch_dir: Path


@runtime_checkable
class Engine(Protocol):
    id: str
    display_name: str
    capabilities: Capabilities
    # Keys of the `models` object in models/manifest.json that must be downloaded first.
    manifest_models: tuple[str, ...]

    def prepare(self, setup: EngineSetup | None = None) -> None:
        """Load or connect to whatever the engine needs. Called before every page and cheap
        when `setup` is the same as last time; a different setup replaces what was loaded.
        Raises the protocol's MODEL_NOT_READY or LLAMA_SERVER_UNREACHABLE errors."""

    def run(
        self,
        page_image_path: Path | str,
        page_size: tuple[int, int],
        call: CallContext | None = None,
    ) -> list[Block]:
        """Recognize one page. `page_size` is (width, height) in pixels of the image, and the
        returned boxes are in the same pixel space, listed in reading order. Figure blocks come
        back without a crop; the pipeline writes it. Block ids are local to the call, the
        pipeline renumbers them. `call` carries progress reporting and cancellation: engines
        call `call.stage(...)` as they go, which raises CANCELLED after `convert.cancel`.
        Non-fatal problems are added to `call.warnings`."""

    def release(self) -> None:
        """Free memory so the queue can run the next job (and on `engine.configure`)."""


def describe(engine: Engine) -> dict[str, Any]:
    """The JSON form of an engine's declared metadata, as served by `engine.capabilities`."""
    return {
        "id": engine.id,
        "displayName": engine.display_name,
        "capabilities": engine.capabilities.to_dict(),
        "manifestModels": list(engine.manifest_models),
    }


class EngineRegistry:
    def __init__(self) -> None:
        self._engines: dict[str, Engine] = {}

    def register(self, engine: Engine) -> None:
        if engine.id in self._engines:
            raise ValueError(f"engine already registered: {engine.id}")
        self._engines[engine.id] = engine

    def get(self, engine_id: str) -> Engine:
        try:
            return self._engines[engine_id]
        except KeyError:
            raise KeyError(f"unknown engine: {engine_id}") from None

    def all(self) -> list[Engine]:
        return list(self._engines.values())

    def release_all(self) -> None:
        for engine in self._engines.values():
            engine.release()
