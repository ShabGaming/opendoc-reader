"""The Paddle engines before any model is loaded: missing files, missing server, and the hooks
used for progress and cancellation. Nothing here imports Paddle."""

import subprocess
import sys

import pytest

from opendoc_engine.engines import EngineSetup, default_registry
from opendoc_engine.engines.paddle_support import wrap, wrap_stream
from opendoc_engine.rpc import RpcError


def setup(tmp_path, model_id: str, url: str | None = "http://127.0.0.1:9") -> EngineSetup:
    return EngineSetup(model_id, tmp_path / "models", url, 2, tmp_path / "work" / "_engine")


def test_fast_names_the_first_missing_model_file(tmp_path):
    engine = default_registry().get("paddleocr-structure")
    with pytest.raises(RpcError) as caught:
        engine.prepare(setup(tmp_path, "fast-ppocrv6-tiny", None))
    assert caught.value.code == -32001
    assert caught.value.data["reason"] == "files_missing"
    assert caught.value.data["detail"].endswith("inference.json")
    assert "PP-DocLayout_plus-L" in caught.value.data["detail"]


def test_fast_notices_a_partly_installed_model(tmp_path):
    folder = tmp_path / "models" / "fast-ppocrv6-tiny"
    for directory in ("PP-DocLayout_plus-L", "PP-OCRv6_tiny_det"):
        (folder / directory).mkdir(parents=True)
        for name in ("inference.json", "inference.pdiparams", "inference.yml"):
            (folder / directory / name).write_bytes(b"x")
    engine = default_registry().get("paddleocr-structure")
    with pytest.raises(RpcError) as caught:
        engine.prepare(setup(tmp_path, "fast-ppocrv6-tiny", None))
    assert "PP-OCRv6_tiny_rec" in caught.value.data["detail"]


def test_accurate_needs_the_layout_model_and_a_server(tmp_path):
    engine = default_registry().get("paddleocr-vl")
    with pytest.raises(RpcError) as no_files:
        engine.prepare(setup(tmp_path, "accurate-paddleocr-vl-1.6"))
    assert no_files.value.data["reason"] == "files_missing"
    assert "PP-DocLayoutV3" in no_files.value.data["detail"]
    with pytest.raises(RpcError) as no_server:
        engine.prepare(setup(tmp_path, "accurate-paddleocr-vl-1.6", None))
    assert (no_server.value.code, no_server.value.data["reason"]) == (-32006, "llama_server_url")


def test_running_an_unprepared_engine_is_not_configured():
    for engine_id in ("paddleocr-structure", "paddleocr-vl", "deepseek-ocr"):
        with pytest.raises(RpcError) as caught:
            default_registry().get(engine_id).run("x.png", (10, 10))
        assert caught.value.code == -32006


def test_building_the_server_does_not_import_paddle():
    code = (
        "import sys; from opendoc_engine.methods import build_server; build_server(); "
        "print([m for m in ('paddle', 'paddleocr', 'paddlex') if m in sys.modules])"
    )
    result = subprocess.run(
        [sys.executable, "-c", code], capture_output=True, text=True, check=True, timeout=60
    )
    assert result.stdout.strip() == "[]"


def test_wrap_runs_before_and_after_and_keeps_attributes():
    class Model:
        name = "layout"

        def __call__(self, value):
            yield from (value, value + 1)

    class Owner:
        model = Model()

    events = []
    assert wrap(
        Owner, "model", before=lambda: events.append("before"), after=lambda r: events.append(r)
    )
    assert list(Owner.model(1)) == [1, 2]
    assert events == ["before", [1, 2]]
    assert Owner.model.name == "layout"


def test_wrap_of_a_missing_attribute_is_a_quiet_no():
    class Owner:
        pass

    assert wrap(Owner, "nothing", before=lambda: None) is False
    assert wrap_stream(Owner, "nothing", each=lambda: None) is False


def test_wrap_stream_counts_items_and_can_stop_the_stream():
    class Owner:
        def predict(self, items):
            yield from items

    seen = []

    def each():
        seen.append(1)
        if len(seen) == 2:
            raise RuntimeError("stop")

    owner = Owner()
    wrap_stream(owner, "predict", each=each)
    produced = []
    with pytest.raises(RuntimeError):
        for item in owner.predict([10, 20, 30]):
            produced.append(item)
    assert produced == [10]
