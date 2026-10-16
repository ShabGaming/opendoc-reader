"""Fast tier: PP-StructureV3 with only layout detection and text OCR, on the CPU.

This is the configuration the model spike settled on (docs/spike, section 5): PP-OCRv6 tiny
detection and recognition, a detection limit of 960 pixels, oneDNN off (it fails on
paddlepaddle 3.3.1 CPU). Orientation, unwarping, region detection, text line orientation and
the table, formula, seal and chart recognizers are off, so tables and formulas come out as
figures and the shared pipeline crops them from the page image.

The three models are read from `<modelsDir>/<modelId>/<dir>/` and Paddle is never allowed to
download anything.
"""

from __future__ import annotations

import logging
from pathlib import Path
from typing import Any

from opendoc_engine.engines.base import Capabilities, EngineSetup
from opendoc_engine.engines.paddle_support import close_pipeline, inner_pipeline, wrap
from opendoc_engine.errors import map_exception, model_not_ready, not_configured
from opendoc_engine.ir import BBox, Block, Line
from opendoc_engine.paddle_blocks import blocks_from_layout, layout_items
from opendoc_engine.paddle_env import (
    model_file,
    paddle_dir_files,
    prepare_paddle_env,
    require_files,
)
from opendoc_engine.rpc import CallContext, RpcError

log = logging.getLogger(__name__)

LAYOUT_DIR = "PP-DocLayout_plus-L"
DETECTION_DIR = "PP-OCRv6_tiny_det"
RECOGNITION_DIR = "PP-OCRv6_tiny_rec"
DETECTION_LIMIT = 960


def text_lines(ocr: Any) -> list[tuple[BBox, str]]:
    """The recognized text lines of Paddle's `overall_ocr_res`, as (box, text) pairs."""
    try:
        boxes, texts = ocr["rec_boxes"], ocr["rec_texts"]
    except (KeyError, TypeError):
        return []
    lines = []
    for box, text in zip(boxes, texts, strict=False):
        x1, y1, x2, y2 = (float(v) for v in box)
        if text and text.strip():
            lines.append(((x1, y1, x2, y2), text.strip()))
    return lines


def lines_within(lines: list[tuple[BBox, str]], area: BBox) -> list[Line]:
    """The lines whose centre lies inside `area`, top to bottom then left to right."""
    inside = []
    for box, text in lines:
        cx, cy = (box[0] + box[2]) / 2, (box[1] + box[3]) / 2
        if area[0] <= cx <= area[2] and area[1] <= cy <= area[3]:
            inside.append((box, text))
    inside.sort(key=lambda item: (round(item[0][1] / 10), item[0][0]))
    return [Line(box, text) for box, text in inside]


class FastEngine:
    id = "paddleocr-structure"
    display_name = "PP-StructureV3 (text and layout)"
    capabilities = Capabilities(
        tables=False,
        formulas=False,
        figures=True,
        reading_order=True,
        bounding_boxes=True,
        language_handling="selectable",
    )
    manifest_models: tuple[str, ...] = ("fast-ppocrv6-tiny",)

    def __init__(self) -> None:
        self._pipeline: Any = None
        self._key: tuple[Any, ...] | None = None
        self._call: CallContext | None = None

    def prepare(self, setup: EngineSetup | None = None) -> None:
        if setup is None:
            raise not_configured()
        key = (setup.model_id, str(setup.models_dir), setup.cpu_threads)
        if self._pipeline is not None and key == self._key:
            return
        self.release()
        require_files(
            setup.models_dir,
            setup.model_id,
            [
                *paddle_dir_files(LAYOUT_DIR),
                *paddle_dir_files(DETECTION_DIR),
                *paddle_dir_files(RECOGNITION_DIR),
            ],
        )
        try:
            self._pipeline = self._load(setup)
        except Exception as exc:
            raise model_not_ready("load_failed", f"{type(exc).__name__}: {exc}") from exc
        self._key = key

    def _load(self, setup: EngineSetup) -> Any:
        prepare_paddle_env(setup.scratch_dir)
        from paddleocr import PPStructureV3

        def directory(name: str) -> str:
            return str(Path(model_file(setup.models_dir, setup.model_id, name)))

        pipeline = PPStructureV3(
            layout_detection_model_name=LAYOUT_DIR,
            layout_detection_model_dir=directory(LAYOUT_DIR),
            text_detection_model_name=DETECTION_DIR,
            text_detection_model_dir=directory(DETECTION_DIR),
            text_recognition_model_name=RECOGNITION_DIR,
            text_recognition_model_dir=directory(RECOGNITION_DIR),
            text_det_limit_type="max",
            text_det_limit_side_len=DETECTION_LIMIT,
            use_doc_orientation_classify=False,
            use_doc_unwarping=False,
            use_region_detection=False,
            use_textline_orientation=False,
            use_table_recognition=False,
            use_formula_recognition=False,
            use_seal_recognition=False,
            use_chart_recognition=False,
            device="cpu",
            enable_mkldnn=False,
            cpu_threads=setup.cpu_threads,
        )
        self._hook(pipeline)
        return pipeline

    def _hook(self, pipeline: Any) -> None:
        """After layout detection the page moves to text recognition: report that, and stop
        there if the call was cancelled."""
        inner = inner_pipeline(pipeline)
        if inner is None:
            return
        wrap(inner, "layout_det_model", after=lambda _result: self._stage("recognize"))

    def _stage(self, stage: str) -> None:
        if self._call is not None:
            self._call.stage(stage)

    def run(
        self,
        page_image_path: Path | str,
        page_size: tuple[int, int],
        call: CallContext | None = None,
    ) -> list[Block]:
        if self._pipeline is None:
            raise not_configured()
        self._call = call
        try:
            self._stage("layout")
            try:
                result = next(iter(self._pipeline.predict(str(page_image_path))))
            except RpcError:
                raise
            except Exception as exc:
                raise map_exception(exc) from exc
            self._stage("normalize")
            ocr_lines = text_lines(result.get("overall_ocr_res"))
            return blocks_from_layout(
                layout_items(result["parsing_res_list"]),
                "fast",
                lines_for=lambda box: lines_within(ocr_lines, box),
                warn=call.warn if call else log.warning,
            )
        finally:
            self._call = None

    def release(self) -> None:
        pipeline, self._pipeline, self._key = self._pipeline, None, None
        if pipeline is not None:
            close_pipeline(pipeline)
