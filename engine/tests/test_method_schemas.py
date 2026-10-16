"""What the page, convert and configure methods return matches engine/schema/rpc."""

import json
from pathlib import Path

import pytest
from PIL import Image

from opendoc_engine.methods import build_server
from pdfgen import PageSpec, Text, make_pdf
from test_rpc_schema import errors, method_validator, validator


@pytest.fixture(scope="module")
def run(tmp_path_factory):
    root = tmp_path_factory.mktemp("schemas")
    server = build_server()
    pdf = root / "a.pdf"
    pdf.write_bytes(make_pdf([PageSpec([Text(72, 700, 12, "Some text on the page to read.")])]))
    tiff = root / "a.tif"
    Image.new("RGB", (50, 60), "white").save(tiff)
    counter = iter(range(1, 1000))

    def call(method: str, params: dict | None = None):
        request = {"jsonrpc": "2.0", "id": next(counter), "method": method}
        if params is not None:
            request["params"] = params
        response = json.loads(server.handle_line(json.dumps(request)))
        assert errors(validator("envelope.schema.json", "successResponse"), response) == []
        assert errors(method_validator(method, "params"), params) == []
        assert errors(method_validator(method, "result"), response["result"]) == []
        return response["result"]

    return call, root, pdf, tiff


def test_inspect_and_render_results(run):
    call, root, pdf, tiff = run
    call("pdf.inspect", {"path": str(pdf)})
    call("image.inspect", {"path": str(tiff)})
    out = root / "pages" / "page-0000.png"
    call("page.render", {"source": str(pdf), "pageIndex": 0, "outPath": str(out)})
    call(
        "page.render",
        {
            "source": str(tiff),
            "pageIndex": 0,
            "outPath": str(root / "i.png"),
            "dpi": 150,
            "rotate": 90,
            "deskew": False,
            "dewarp": False,
        },
    )


def test_text_extract_and_convert_results(run):
    call, root, pdf, _tiff = run
    plain = root / "plain.png"
    call(
        "page.render",
        {"source": str(pdf), "pageIndex": 0, "outPath": str(plain), "rotate": 0, "deskew": False},
    )
    call(
        "text.extract",
        {
            "source": str(pdf),
            "pageIndex": 0,
            "imagePath": str(plain),
            "irPath": str(root / "ir" / "t.json"),
        },
    )
    call(
        "engine.configure",
        {"llamaServerUrl": None, "workDir": str(root / "work"), "modelsDir": str(root / "models")},
    )
    call(
        "convert.page",
        {
            "engineId": "demo",
            "modelId": "x",
            "imagePath": str(plain),
            "pageIndex": 0,
            "irPath": str(root / "ir" / "c.json"),
            "cropsDir": str(root / "crops"),
            "options": {"language": "en"},
        },
    )
    call(
        "calibrate.run",
        {"engineId": "demo", "modelId": "x", "samplePath": str(plain), "runs": 1},
    )
    call("convert.cancel", {"requestId": 99})


def test_ir_files_written_by_text_extract_and_convert_validate(run):
    _call, root, _pdf, _tiff = run
    for name in ("t.json", "c.json"):
        path = Path(root / "ir" / name)
        if not path.exists():
            pytest.skip("written by the previous test")
        assert (
            errors(validator("ir.schema.json", "Page"), json.loads(path.read_text("utf-8"))) == []
        )
