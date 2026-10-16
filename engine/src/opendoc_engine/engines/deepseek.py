"""Max tier: DeepSeek-OCR over llama-server's chat endpoint.

One request per page with the grounding prompt. The answer is a stream of
`<|ref|>label<|/ref|><|det|>[[x1, y1, x2, y2]]<|/det|>` segments that `grounding.parse_grounding`
turns into IR blocks. llama-server must run with `--special`, or the tags are stripped (the
manifest's `llamaServerArgs` take care of that).

The experimental DeepSeek-OCR 2 model loops on some pages without repetition control, so for
that model id the request also carries DRY sampling parameters, the same ones its manifest entry
passes on the server command line.
"""

from __future__ import annotations

import io
import logging
import re
from pathlib import Path

from PIL import Image

from opendoc_engine.engines.base import Capabilities, EngineSetup
from opendoc_engine.errors import not_configured, recognition_failed
from opendoc_engine.grounding import parse_grounding
from opendoc_engine.ir import Block
from opendoc_engine.llama import LlamaClient
from opendoc_engine.rpc import CallContext

log = logging.getLogger(__name__)

PROMPT = "<|grounding|>Convert the document to markdown."
# Dense pages take about 1,900 tokens; a loop runs to this limit and is reported as one.
MAX_TOKENS = 4096

_DRY_PARAMS = {
    "dry_multiplier": 0.8,
    "dry_base": 1.75,
    "dry_allowed_length": 2,
    "dry_penalty_last_n": 8192,
}
_CENTER_TAGS = re.compile(r"</?center>", re.IGNORECASE)


def uses_dry_sampling(model_id: str) -> bool:
    return "deepseek-ocr-2" in model_id


def _png_bytes(path: Path | str) -> bytes:
    data = Path(path).read_bytes()
    if data.startswith(b"\x89PNG"):
        return data
    buffer = io.BytesIO()
    Image.open(io.BytesIO(data)).convert("RGB").save(buffer, format="PNG")
    return buffer.getvalue()


def _tidy(blocks: list[Block]) -> None:
    """Drop the `<center>` wrappers the model puts around captions."""
    for block in blocks:
        if block.text and "<" in block.text:
            block.text = " ".join(_CENTER_TAGS.sub(" ", block.text).split())


class DeepseekEngine:
    id = "deepseek-ocr"
    display_name = "DeepSeek-OCR"
    capabilities = Capabilities(
        tables=True,
        formulas=True,
        figures=True,
        reading_order=True,
        bounding_boxes=True,
        language_handling="auto",
    )
    manifest_models: tuple[str, ...] = ("max-deepseek-ocr-1-q8", "experimental-deepseek-ocr-2-q8")

    def __init__(self) -> None:
        self._setup: EngineSetup | None = None

    def prepare(self, setup: EngineSetup | None = None) -> None:
        if setup is None:
            raise not_configured()
        if not setup.llama_base_url:
            raise not_configured("llama_server_url")
        self._setup = setup

    def run(
        self,
        page_image_path: Path | str,
        page_size: tuple[int, int],
        call: CallContext | None = None,
    ) -> list[Block]:
        setup = self._setup
        if setup is None or setup.llama_base_url is None:
            raise not_configured()
        call = call or CallContext()
        png = _png_bytes(page_image_path)
        call.stage("recognize")
        completion = LlamaClient(setup.llama_base_url).complete_image(
            png=png,
            prompt=PROMPT,
            max_tokens=MAX_TOKENS,
            call=call,
            extra=_DRY_PARAMS if uses_dry_sampling(setup.model_id) else None,
        )
        if completion.finish_reason == "length":
            raise recognition_failed("max_tokens", f"stopped after {completion.chunks} chunks")
        if not completion.text.strip():
            raise recognition_failed("output_unusable", "empty answer")
        call.stage("normalize")
        blocks = parse_grounding(completion.text, page_size)
        if not blocks:
            raise recognition_failed("output_unusable", "no blocks in the answer")
        _tidy(blocks)
        return blocks

    def release(self) -> None:
        self._setup = None
