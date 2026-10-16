"""convert.page, calibrate.run and engine.configure through the RPC layer, with the demo engine
and a small figure engine, so no models are needed."""

import json
from pathlib import Path

import pytest
from jsonschema import Draft202012Validator
from PIL import Image

from opendoc_engine.engines import Capabilities, DemoEngine, EngineRegistry
from opendoc_engine.ir import Block, BlockType
from opendoc_engine.methods import build_methods
from opendoc_engine.rpc import CallContext, RpcServer, current_call

REPO = Path(__file__).resolve().parents[2]
IR_SCHEMA = json.loads((REPO / "engine" / "schema" / "ir.schema.json").read_text("utf-8"))


class FigureEngine:
    """Returns a caption, a figure, a tiny figure and a paragraph, boxes in image pixels."""

    id = "figures"
    display_name = "Figures"
    capabilities = Capabilities(figures=True)
    manifest_models: tuple[str, ...] = ()

    def __init__(self) -> None:
        self.prepared: list = []
        self.released = 0
        self.fail: Exception | None = None

    def prepare(self, setup=None) -> None:
        self.prepared.append(setup)

    def run(self, page_image_path, page_size, call: CallContext | None = None):
        if self.fail:
            raise self.fail
        call.stage("recognize")
        call.warn("noted")
        return [
            Block("x", BlockType.CAPTION, (0, 0, 50, 10), 9, text="A caption", caption_for="y"),
            Block("y", BlockType.FIGURE, (10, 20, 60, 70), 9),
            Block("t", BlockType.FIGURE, (5, 5, 6, 6), 9),  # one pixel: dropped
            Block("z", BlockType.PARAGRAPH, (0, 80, 100, 99.5), 9, text="Text"),
        ]

    def release(self) -> None:
        self.released += 1


@pytest.fixture
def engines():
    registry = EngineRegistry()
    registry.register(DemoEngine())
    registry.register(FigureEngine())
    return registry


@pytest.fixture
def server(engines):
    return RpcServer(build_methods(engines))


@pytest.fixture
def page_image(tmp_path) -> Path:
    path = tmp_path / "pages" / "page-0003.png"
    path.parent.mkdir()
    Image.new("RGB", (100, 100), "white").save(path)
    return path


def call(server, method, params=None, request_id=1):
    message = {"jsonrpc": "2.0", "id": request_id, "method": method, "params": params}
    return json.loads(server.handle_line(json.dumps(message)))


def configure(server, tmp_path, url="http://127.0.0.1:5000/", threads=None):
    params = {
        "llamaServerUrl": url,
        "workDir": str(tmp_path / "work"),
        "modelsDir": str(tmp_path / "models"),
    }
    if threads:
        params["cpuThreads"] = threads
    response = call(server, "engine.configure", params)
    assert "error" not in response, response
    return response["result"]


def convert_params(tmp_path, page_image, engine_id="demo", **extra):
    return {
        "engineId": engine_id,
        "modelId": "some-model",
        "imagePath": str(page_image),
        "pageIndex": 3,
        "irPath": str(tmp_path / "work" / "f1" / "ir" / "page-0003.json"),
        "cropsDir": str(tmp_path / "work" / "f1" / "crops"),
        **extra,
    }


# --- engine.configure -----------------------------------------------------------------------


def test_configure_returns_the_threads_in_effect(server, tmp_path):
    assert configure(server, tmp_path, threads=3) == {"cpuThreads": 3}
    assert configure(server, tmp_path)["cpuThreads"] >= 1


def test_configure_drops_loaded_engines(server, engines, tmp_path):
    configure(server, tmp_path)
    assert engines.get("figures").released == 1
    configure(server, tmp_path)
    assert engines.get("figures").released == 2


def test_configure_accepts_a_null_server_url_and_checks_its_parameters(server, tmp_path):
    configure(server, tmp_path, url=None)
    assert call(server, "engine.configure", {"workDir": "x"})["error"]["data"] == {
        "reason": "missing_param",
        "field": "llamaServerUrl",
    }
    bad = call(
        server,
        "engine.configure",
        {"llamaServerUrl": None, "workDir": "a", "modelsDir": "b", "cpuThreads": 0},
    )
    assert bad["error"]["data"] == {"reason": "bad_type", "field": "cpuThreads"}


# --- convert.page ---------------------------------------------------------------------------


def test_convert_before_configure_is_not_configured(server, tmp_path, page_image):
    response = call(server, "convert.page", convert_params(tmp_path, page_image))
    assert response["error"]["code"] == -32006
    assert response["error"]["data"] == {"reason": "not_configured"}


def test_demo_page_is_written_and_valid(server, tmp_path, page_image):
    configure(server, tmp_path)
    params = convert_params(tmp_path, page_image)
    response = call(server, "convert.page", params)
    result = response["result"]
    assert result["irPath"] == params["irPath"]
    assert result["blockCount"] == 4
    assert result["engine"] == {"id": "demo", "model": "some-model", "version": "0.1.0"}
    assert isinstance(result["elapsedMs"], int)
    assert result["warnings"] == []

    page = json.loads(Path(params["irPath"]).read_text("utf-8"))
    assert (page["index"], page["width"], page["height"]) == (3, 100, 100)
    assert page["sourceImage"] == str(page_image)
    document = {
        "schemaVersion": "1",
        "source": {"path": "x.pdf", "kind": "pdf", "pageCount": 4},
        "engine": result["engine"],
        "pages": [page],
    }
    errors = list(Draft202012Validator(IR_SCHEMA).iter_errors(document))
    assert not errors, errors[0].message
    assert [b["id"] for b in page["blocks"]] == ["p3-b0", "p3-b1", "p3-b2", "p3-b3"]
    table = page["blocks"][2]
    assert [c["blocks"][0]["id"] for c in table["table"]["cells"]] == [
        "p3-b2-r0c0",
        "p3-b2-r1c0",
        "p3-b2-r1c1",
    ]
    assert not list(Path(params["irPath"]).parent.glob(".*.tmp"))


def test_blocks_are_renumbered_cropped_and_linked(server, engines, tmp_path, page_image):
    configure(server, tmp_path)
    params = convert_params(tmp_path, page_image, engine_id="figures", options={"language": "de"})
    result = call(server, "convert.page", params)["result"]
    assert result["blockCount"] == 3  # the one-pixel figure is gone
    assert result["warnings"] == [
        "language option 'de' is not used by figures",
        "noted",
        "figure p3-b2 is too small to crop; dropped",
    ]

    page = json.loads(Path(params["irPath"]).read_text("utf-8"))
    caption, figure, paragraph = page["blocks"]
    assert [b["order"] for b in page["blocks"]] == [0, 1, 2]
    assert caption["caption"] == {"for": "p3-b1"}
    crop = Path(figure["figure"]["crop"])
    assert crop == tmp_path / "work" / "f1" / "crops" / "p3-b1.png"
    with Image.open(crop) as image:
        assert image.size == (50, 50)
    assert paragraph["bbox"][3] == 99.5
    assert len(list(crop.parent.glob("*.png"))) == 1


def test_the_engine_gets_the_configured_setup(server, engines, tmp_path, page_image):
    configure(server, tmp_path, url="http://127.0.0.1:5000/v1/", threads=2)
    call(server, "convert.page", convert_params(tmp_path, page_image, engine_id="figures"))
    setup = engines.get("figures").prepared[0]
    assert setup.model_id == "some-model"
    assert setup.llama_base_url == "http://127.0.0.1:5000"
    assert setup.cpu_threads == 2
    assert setup.models_dir == tmp_path / "models"
    assert setup.scratch_dir == tmp_path / "work" / "_engine"


def test_without_a_server_url_the_setup_has_none(server, engines, tmp_path, page_image):
    configure(server, tmp_path, url=None)
    call(server, "convert.page", convert_params(tmp_path, page_image, engine_id="figures"))
    assert engines.get("figures").prepared[0].llama_base_url is None


def test_unknown_engine_and_bad_parameters(server, tmp_path, page_image):
    configure(server, tmp_path)
    unknown = call(server, "convert.page", convert_params(tmp_path, page_image, engine_id="nope"))
    assert unknown["error"]["code"] == -32602
    assert unknown["error"]["data"] == {"reason": "unknown_engine", "field": "engineId"}
    typo = call(server, "convert.page", convert_params(tmp_path, page_image, imagePth="x"))
    assert typo["error"]["data"]["reason"] == "unknown_param"
    options = call(server, "convert.page", convert_params(tmp_path, page_image, options={"x": 1}))
    assert options["error"]["data"] == {"reason": "unknown_param", "field": "x"}
    negative = call(server, "convert.page", convert_params(tmp_path, page_image, pageIndex=-1))
    assert negative["error"]["data"] == {"reason": "bad_type", "field": "pageIndex"}


def test_bad_images(server, tmp_path):
    configure(server, tmp_path)
    missing = call(server, "convert.page", convert_params(tmp_path, tmp_path / "gone.png"))
    assert missing["error"]["data"]["reason"] == "file_not_found"
    junk = tmp_path / "junk.png"
    junk.write_bytes(b"not an image")
    assert (
        call(server, "convert.page", convert_params(tmp_path, junk))["error"]["data"]["reason"]
        == "unsupported_format"
    )


def test_engine_failures_map_to_protocol_errors(server, engines, tmp_path, page_image):
    configure(server, tmp_path)
    params = convert_params(tmp_path, page_image, engine_id="figures")
    figures = engines.get("figures")
    cases = [
        (MemoryError("x"), -32003, "ram"),
        (RuntimeError("CUDA error: out of memory"), -32003, "gpu"),
        (ConnectionRefusedError("refused"), -32002, "connection_refused"),
        (ValueError("weird"), -32008, "output_unusable"),
    ]
    for exc, code, why in cases:
        figures.fail = exc
        error = call(server, "convert.page", params)["error"]
        assert (error["code"], error["data"]["reason"]) == (code, why)
    assert not Path(params["irPath"]).exists()


def test_a_cancelled_page_writes_no_ir(server, engines, tmp_path, page_image):
    configure(server, tmp_path)
    params = convert_params(tmp_path, page_image, engine_id="figures")
    figures = engines.get("figures")

    def cancel_during_run(*_args, **_kwargs):
        current_call().cancel()
        current_call().check_cancelled()

    figures.run = cancel_during_run
    error = call(server, "convert.page", params)["error"]
    assert error["code"] == -32005
    assert not Path(params["irPath"]).exists()


# --- calibrate.run --------------------------------------------------------------------------


def test_calibrate_times_a_warmup_and_the_requested_runs(server, tmp_path, page_image):
    configure(server, tmp_path)
    params = {
        "engineId": "demo",
        "modelId": "m",
        "samplePath": str(page_image),
        "runs": 3,
    }
    result = call(server, "calibrate.run", params)["result"]
    assert len(result["runSeconds"]) == 3
    assert result["warmupSeconds"] >= 0
    assert result["secondsPerPage"] == pytest.approx(sorted(result["runSeconds"])[1], abs=0.011)
    assert not (tmp_path / "work" / "_engine" / "calibration").exists()


def test_calibrate_defaults_to_two_runs_and_validates_runs(server, tmp_path, page_image):
    configure(server, tmp_path)
    params = {"engineId": "demo", "modelId": "m", "samplePath": str(page_image)}
    assert len(call(server, "calibrate.run", params)["result"]["runSeconds"]) == 2
    assert call(server, "calibrate.run", {**params, "runs": 9})["error"]["code"] == -32602
