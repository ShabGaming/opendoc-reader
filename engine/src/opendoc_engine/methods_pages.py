"""RPC methods for input files: `pdf.inspect`, `image.inspect`, `page.render` and `text.extract`
(docs/protocol.md 1.5)."""

from __future__ import annotations

import time
from typing import Any

import cv2
from PIL import Image

from opendoc_engine import __version__, preprocess
from opendoc_engine.errors import bad_input, map_exception, model_not_ready, out_of_memory
from opendoc_engine.files import write_json, write_png
from opendoc_engine.inputs import inspect_image, inspect_pdf, open_pdf, render_source_page
from opendoc_engine.ir import EngineInfo, Page
from opendoc_engine.params import Field, parse_params
from opendoc_engine.pipeline import PageResult, renumber
from opendoc_engine.rpc import MethodRegistry, RpcError
from opendoc_engine.runtime import Runtime
from opendoc_engine.text_extract import extract_blocks

_PATH = {"path": Field("path")}
_RENDER = {
    "source": Field("path"),
    "pageIndex": Field("int", minimum=0),
    "outPath": Field("path"),
    "dpi": Field("int", required=False, default=200, minimum=72, maximum=600),
    "rotate": Field("any", required=False, default="auto", choices=("auto", 0, 90, 180, 270)),
    "deskew": Field("bool", required=False, default=True),
    "dewarp": Field("bool", required=False, default=False),
}
_EXTRACT = {
    "source": Field("path"),
    "pageIndex": Field("int", minimum=0),
    "imagePath": Field("path"),
    "irPath": Field("path"),
}


def register(methods: MethodRegistry, runtime: Runtime) -> None:
    @methods.method("pdf.inspect")
    def pdf_inspect(params: Any) -> dict[str, Any]:
        return inspect_pdf(parse_params(params, _PATH)["path"])

    @methods.method("image.inspect")
    def image_inspect(params: Any) -> dict[str, Any]:
        return inspect_image(parse_params(params, _PATH)["path"])

    @methods.method("page.render")
    def page_render(params: Any) -> dict[str, Any]:
        p = parse_params(params, _RENDER)
        try:
            return render_page(runtime, p)
        except (RpcError, MemoryError):
            raise
        except cv2.error as exc:
            raise map_exception(exc) from exc

    @methods.method("text.extract")
    def text_extract(params: Any) -> dict[str, Any]:
        p = parse_params(params, _EXTRACT)
        return extract_text(p).to_dict()


def render_page(runtime: Runtime, p: dict[str, Any]) -> dict[str, Any]:
    settings = runtime.settings_or_none()
    try:
        image, dpi = render_source_page(p["source"], p["pageIndex"], p["dpi"])
        rotation = p["rotate"]
        if rotation == "auto":
            rotation = (
                preprocess.detect_rotation(image, settings.models_dir, settings.scratch_dir)
                if settings
                else 0
            )
        image = preprocess.rotate_clockwise(image, rotation)
        if p["dewarp"]:
            if settings is None:
                raise model_not_ready(
                    "files_missing", f"{preprocess.UNWARP_MODEL} (not configured)"
                )
            image = preprocess.dewarp(image, settings.models_dir, settings.scratch_dir)
        skew = 0.0
        if p["deskew"]:
            image, skew = preprocess.deskew(image)
    except MemoryError as exc:
        raise out_of_memory("ram", p["source"]) from exc
    write_png(p["outPath"], image)
    return {
        "path": p["outPath"],
        "width": image.width,
        "height": image.height,
        "dpi": dpi,
        "rotation": rotation,
        "skewAngle": skew,
    }


def extract_text(p: dict[str, Any]) -> PageResult:
    started = time.monotonic()
    page_index = p["pageIndex"]
    try:
        with Image.open(p["imagePath"]) as image:
            width, height = image.size
    except FileNotFoundError as exc:
        raise bad_input("file_not_found", p["imagePath"]) from exc
    except OSError as exc:
        raise bad_input("unreadable", f"{p['imagePath']}: {exc}") from exc

    with open_pdf(p["source"]) as pdf:
        if not 0 <= page_index < len(pdf):
            raise bad_input("page_out_of_range", f"{p['source']}: page {page_index}")
        blocks = extract_blocks(pdf, page_index, (width, height), p["source"])
    blocks = renumber(blocks, page_index, width, height)
    write_json(p["irPath"], Page(page_index, width, height, p["imagePath"], blocks).to_dict())
    return PageResult(
        ir_path=p["irPath"],
        block_count=len(blocks),
        engine=EngineInfo("pdf-text", "text-layer", __version__),
        elapsed_ms=round((time.monotonic() - started) * 1000),
        warnings=[],
    )
