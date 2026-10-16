"""Real models on the three public-domain sample scans. Opt in with OPENDOC_INTEGRATION=1.

    OPENDOC_INTEGRATION=1 uv run pytest tests/test_integration.py -s

Settings, all optional:

- OPENDOC_MODELS_DIR: models in the manifest layout (default G:/opendoc-dev/models, built by
  scripts/link_dev_models.py).
- OPENDOC_SAMPLES_DIR: the spike's sample folder (default G:/opendoc-spike/samples).
- OPENDOC_INTEGRATION_ENGINES: comma-separated engine ids to run (default `paddleocr-structure`,
  the only one that needs no llama-server).
- OPENDOC_LLAMA_URL: base URL of a running llama-server for `paddleocr-vl` or `deepseek-ocr`. It
  must serve the model of the engine being tested, started with the manifest's llamaServerArgs
  (so run one engine per invocation).
- OPENDOC_MODEL_ID: the manifest model id when it is not the engine's default (for example
  `experimental-deepseek-ocr-2-q8`).

Timings of each page are printed; use -s to see them.
"""

import json
import os
import threading
import time
from pathlib import Path

import pytest
from jsonschema import Draft202012Validator

from opendoc_engine.ir import BlockType
from opendoc_engine.methods import build_server
from opendoc_engine.rpc import RpcServer
from test_rpc_threading import Pipe, request

pytestmark = pytest.mark.skipif(
    os.environ.get("OPENDOC_INTEGRATION") != "1", reason="set OPENDOC_INTEGRATION=1 to run"
)

REPO = Path(__file__).resolve().parents[2]
MODELS = Path(os.environ.get("OPENDOC_MODELS_DIR", "G:/opendoc-dev/models"))
SAMPLES = Path(os.environ.get("OPENDOC_SAMPLES_DIR", "G:/opendoc-spike/samples"))
LLAMA_URL = os.environ.get("OPENDOC_LLAMA_URL")
PAGES = {
    "book": SAMPLES / "ia-tom-sawyer-1876" / "n100_2200.png",
    "table": SAMPLES / "ia-statabstract-1920" / "n120_2200.png",
    "math": SAMPLES / "ia-granville-calculus-1911" / "n100_2200.png",
}
DEFAULT_MODELS = {
    "paddleocr-structure": "fast-ppocrv6-tiny",
    "paddleocr-vl": "accurate-paddleocr-vl-1.6",
    "deepseek-ocr": "max-deepseek-ocr-1-q8",
}
ENGINES = [
    e for e in os.environ.get("OPENDOC_INTEGRATION_ENGINES", "paddleocr-structure").split(",") if e
]
SCHEMA = json.loads((REPO / "engine" / "schema" / "ir.schema.json").read_text("utf-8"))


class Session:
    """One server configured for an engine, with the converted pages of the samples."""

    def __init__(self, engine_id: str, root: Path) -> None:
        self.engine_id = engine_id
        self.model_id = os.environ.get("OPENDOC_MODEL_ID", DEFAULT_MODELS[engine_id])
        self.root = root
        self.server = build_server()
        self.ids = iter(range(1, 10_000))
        self.seconds: dict[str, float] = {}
        self.pages: dict[str, dict] = {}

    def call(self, method: str, params: dict) -> dict:
        message = {"jsonrpc": "2.0", "id": next(self.ids), "method": method, "params": params}
        return json.loads(self.server.handle_line(json.dumps(message)))

    def configure(self) -> None:
        url = LLAMA_URL if self.engine_id != "paddleocr-structure" else None
        result = self.call(
            "engine.configure",
            {
                "llamaServerUrl": url,
                "workDir": str(self.root / "work"),
                "modelsDir": str(MODELS),
            },
        )
        assert "result" in result, result

    def render(self, name: str) -> Path:
        out = self.root / "work" / "pages" / f"{name}.png"
        done = self.call(
            "page.render",
            {"source": str(PAGES[name]), "pageIndex": 0, "outPath": str(out)},
        )
        assert "result" in done, done
        return out

    def params(self, name: str) -> dict:
        return {
            "engineId": self.engine_id,
            "modelId": self.model_id,
            "imagePath": str(self.render(name)),
            "pageIndex": 0,
            "irPath": str(self.root / "work" / "ir" / f"{name}.json"),
            "cropsDir": str(self.root / "work" / "crops" / name),
        }

    def convert(self, name: str) -> None:
        started = time.monotonic()
        response = self.call("convert.page", self.params(name))
        self.seconds[name] = time.monotonic() - started
        assert "result" in response, response
        self.pages[name] = json.loads(Path(response["result"]["irPath"]).read_text("utf-8"))
        print(f"\n[{self.engine_id}] {name}: {self.seconds[name]:.1f} s, {response['result']}")


@pytest.fixture(scope="module", params=ENGINES)
def session(request, tmp_path_factory) -> Session:
    engine_id = request.param
    if engine_id != "paddleocr-structure" and not LLAMA_URL:
        pytest.skip("set OPENDOC_LLAMA_URL to a llama-server serving this engine's model")
    current = Session(engine_id, tmp_path_factory.mktemp(engine_id))
    current.configure()
    # Every page twice: the first run includes loading the models, the second is the warm time.
    for name in PAGES:
        current.convert(name)
    for name in PAGES:
        current.convert(name)
    return current


def kinds(page: dict) -> list[str]:
    return [b["type"] for b in page["blocks"]]


def test_every_page_validates_against_the_ir_schema(session):
    for name, page in session.pages.items():
        document = {
            "schemaVersion": "1",
            "source": {"path": str(PAGES[name]), "kind": "image", "pageCount": 1},
            "engine": {"id": session.engine_id, "model": session.model_id, "version": "0.1.0"},
            "pages": [page],
        }
        errors = list(Draft202012Validator(SCHEMA).iter_errors(document))
        assert not errors, f"{name}: {errors[0].message}"
        assert [b["order"] for b in page["blocks"]] == list(range(len(page["blocks"])))


def test_book_page_has_text_and_a_cropped_figure(session):
    page = session.pages["book"]
    assert kinds(page).count("paragraph") >= 3
    figures = [b for b in page["blocks"] if b["type"] == "figure"]
    assert figures
    for figure in figures:
        assert Path(figure["figure"]["crop"]).stat().st_size > 1000
    words = " ".join(b.get("text", "") for b in page["blocks"]).lower()
    assert "sober" in words


def test_table_page(session):
    page = session.pages["table"]
    if session.engine_id == "paddleocr-structure":
        # The Fast tier keeps tables as images.
        assert "table" not in kinds(page)
        biggest = max(
            (b for b in page["blocks"] if b["type"] == "figure"),
            key=lambda b: b["bbox"][3] - b["bbox"][1],
        )
        assert biggest["bbox"][3] - biggest["bbox"][1] > 1000
        return
    tables = [b["table"] for b in page["blocks"] if b["type"] == "table"]
    assert len(tables) == 1
    table = tables[0]
    assert table["rows"] > 30 and table["cols"] >= 9
    spans = [(c["rowSpan"], c["colSpan"]) for c in table["cells"]]
    assert any(row > 1 for row, _ in spans), "no cell with a rowspan"
    assert any(col > 1 for _, col in spans), "no cell with a colspan"
    texts = {c["blocks"][0]["text"] for c in table["cells"] if c["blocks"]}
    assert any("Race" in t for t in texts)


def test_math_page(session):
    page = session.pages["math"]
    formulas = [b for b in page["blocks"] if b["type"] == "formula"]
    if session.engine_id == "paddleocr-structure":
        assert not formulas
        assert kinds(page).count("figure") >= 3  # display equations are cropped
        return
    assert len(formulas) >= 3
    assert all("frac" in f["latex"] or "=" in f["latex"] for f in formulas)
    assert any(f.get("number") for f in formulas), "no equation number was attached"
    assert any("$" in b.get("text", "") for b in page["blocks"] if b["type"] == "paragraph")


def test_a_warm_page_is_not_slower_than_the_first(session):
    # Not a benchmark: it only checks that models stay loaded between pages.
    assert all(seconds < 600 for seconds in session.seconds.values())


def test_cancel_in_the_middle_of_a_page(session):
    """A page cancelled while the model works answers CANCELLED quickly and writes no IR."""
    if session.engine_id == "paddleocr-structure":
        pytest.skip("the Fast tier is cancelled between stages; covered by the stage hook")
    params = session.params("table")
    Path(params["irPath"]).unlink(missing_ok=True)
    pipe = Pipe()
    server: RpcServer = session.server
    thread = threading.Thread(target=server.serve, args=(pipe, pipe), daemon=True)
    thread.start()
    pipe.send(request(500, "convert.page", params))
    seen = []
    deadline = time.monotonic() + 120
    while time.monotonic() < deadline:  # wait until recognition has started
        message = pipe.next(timeout=120)
        seen.append(message)
        if message.get("params", {}).get("stage") == "recognize":
            break
    pipe.send(request(501, "convert.cancel", {"requestId": 500}))
    cancelled_at = time.monotonic()
    answers = {}
    while 500 not in answers:
        message = pipe.next(timeout=60)
        if "id" in message:
            answers[message["id"]] = message
    assert time.monotonic() - cancelled_at < 20
    assert answers[500]["error"]["code"] == -32005
    assert not Path(params["irPath"]).exists()
    pipe.close()
    thread.join(10)
    # the engine still works afterwards
    again = session.call("convert.page", session.params("book"))
    assert "result" in again, again
    assert BlockType.PARAGRAPH.value in kinds(session.pages["book"])
