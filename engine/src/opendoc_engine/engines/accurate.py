"""Accurate tier: PaddleOCR-VL 1.6.

Layout detection (PP-DocLayoutV3) runs in this process on the CPU. Each layout region is then
recognized by the PaddleOCR-VL model behind llama-server, through PaddleOCR's `llama-cpp-server`
backend. The pipeline hands back a list of labelled, boxed blocks with text, HTML tables and
LaTeX, which `paddle_blocks` turns into IR.

The layout model is read from `<modelsDir>/<modelId>/PP-DocLayoutV3/`. Paddle never downloads
anything; the VL weights are llama-server's business and are not touched here.
"""

from __future__ import annotations

import logging
from pathlib import Path
from typing import Any

from opendoc_engine.engines.base import Capabilities, EngineSetup
from opendoc_engine.engines.paddle_support import (
    close_pipeline,
    inner_pipeline,
    wrap,
    wrap_stream,
)
from opendoc_engine.errors import map_exception, model_not_ready, not_configured
from opendoc_engine.ir import Block
from opendoc_engine.llama import LlamaClient
from opendoc_engine.paddle_blocks import blocks_from_layout, layout_items
from opendoc_engine.paddle_env import (
    model_file,
    paddle_dir_files,
    prepare_paddle_env,
    require_files,
)
from opendoc_engine.rpc import CallContext, RpcError

log = logging.getLogger(__name__)

LAYOUT_DIR = "PP-DocLayoutV3"
# Layout labels that are not sent to the recognizer.
_NOT_RECOGNIZED = {"image", "header_image", "footer_image", "seal", "chart"}


class AccurateEngine:
    id = "paddleocr-vl"
    display_name = "PaddleOCR-VL"
    capabilities = Capabilities(
        tables=True,
        formulas=True,
        figures=True,
        reading_order=True,
        bounding_boxes=True,
        language_handling="auto",
    )
    manifest_models: tuple[str, ...] = ("accurate-paddleocr-vl-1.6",)

    def __init__(self) -> None:
        self._pipeline: Any = None
        self._key: tuple[Any, ...] | None = None
        self._setup: EngineSetup | None = None
        self._call: CallContext | None = None
        self._expected = 0
        self._done = 0

    def prepare(self, setup: EngineSetup | None = None) -> None:
        if setup is None:
            raise not_configured()
        if not setup.llama_base_url:
            raise not_configured("llama_server_url")
        key = (setup.model_id, str(setup.models_dir), setup.llama_base_url, setup.cpu_threads)
        if self._pipeline is not None and key == self._key:
            return
        self.release()
        require_files(setup.models_dir, setup.model_id, paddle_dir_files(LAYOUT_DIR))
        try:
            self._pipeline = self._load(setup)
        except Exception as exc:
            raise model_not_ready("load_failed", f"{type(exc).__name__}: {exc}") from exc
        self._key = key
        self._setup = setup

    def _load(self, setup: EngineSetup) -> Any:
        prepare_paddle_env(setup.scratch_dir)
        from paddleocr import PaddleOCRVL

        pipeline = PaddleOCRVL(
            pipeline_version="v1.6",
            layout_detection_model_name=LAYOUT_DIR,
            layout_detection_model_dir=str(
                model_file(setup.models_dir, setup.model_id, LAYOUT_DIR)
            ),
            vl_rec_backend="llama-cpp-server",
            vl_rec_server_url=f"{setup.llama_base_url}/v1",
            use_doc_orientation_classify=False,
            use_doc_unwarping=False,
            use_chart_recognition=False,
            use_seal_recognition=False,
            device="cpu",
            cpu_threads=setup.cpu_threads,
        )
        self._hook(pipeline)
        return pipeline

    def _hook(self, pipeline: Any) -> None:
        """Progress and cancellation: the layout model reports how many regions will be
        recognized, and every recognized region counts as one step and is a place to stop."""
        inner = inner_pipeline(pipeline)
        if inner is None:
            return
        wrap(inner, "layout_det_model", after=self._layout_done)
        recognizer = getattr(inner, "vl_rec_model", None)
        if recognizer is not None:
            wrap_stream(recognizer, "predict", each=self._region_done)

    def _layout_done(self, results: Any) -> None:
        try:
            boxes = results[0]["boxes"]
        except (IndexError, KeyError, TypeError):
            boxes = []
        self._expected = sum(1 for box in boxes if box.get("label") not in _NOT_RECOGNIZED)
        self._done = 0
        if self._call is not None:
            self._call.stage("recognize", 0, self._expected)

    def _region_done(self) -> None:
        self._done += 1
        if self._call is not None:
            self._call.stage("recognize", self._done, max(self._expected, self._done))

    def run(
        self,
        page_image_path: Path | str,
        page_size: tuple[int, int],
        call: CallContext | None = None,
    ) -> list[Block]:
        if self._pipeline is None or self._setup is None or self._setup.llama_base_url is None:
            raise not_configured()
        self._call = call
        self._expected = self._done = 0
        try:
            LlamaClient(self._setup.llama_base_url).check_health()
            if call:
                call.stage("layout")
            try:
                results = self._pipeline.predict(str(page_image_path), use_queues=False)
                result = next(iter(results))
            except RpcError:
                raise
            except Exception as exc:
                raise map_exception(exc) from exc
            if call:
                call.stage("normalize")
            return blocks_from_layout(
                layout_items(result["parsing_res_list"]),
                "vl",
                warn=call.warn if call else log.warning,
            )
        finally:
            self._call = None

    def release(self) -> None:
        pipeline, self._pipeline, self._key, self._setup = self._pipeline, None, None, None
        if pipeline is not None:
            close_pipeline(pipeline)
