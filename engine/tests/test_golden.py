"""Golden tests: the Fast tier on a small set of public-domain pages. Opt in with OPENDOC_GOLDEN=1.

    OPENDOC_GOLDEN=1 uv run pytest tests/test_golden.py

The page images are in tests/golden/images and their expected IR and Markdown in
tests/golden/expected; tests/golden/SOURCES.md says where each page comes from. The run
converts every page with the Fast tier through the real RPC methods and compares the result
with the checked-in files by structure, not byte for byte, because Paddle's numbers differ a
little between operating systems and CPUs:

- the page size is the same;
- the sequence of block types is at least 85 percent the same;
- the words of the page are at least 90 percent the same;
- of the blocks that line up in the type sequence, 90 percent have a box with an intersection
  over union of at least 0.85;
- the Markdown has at least 90 percent of the same words and the same number of images (give
  or take one).

Settings, all optional:

- OPENDOC_MODELS_DIR: models in the manifest layout (default G:/opendoc-dev/models, built by
  scripts/link_dev_models.py). In CI, scripts/fetch-models.py fills it with the Fast tier.
- OPENDOC_GOLDEN_UPDATE=1: write the expected files from this run instead of comparing. Read the
  new files before committing them and do not edit them by hand.

The comparison functions are checked by tests that always run.
"""

from __future__ import annotations

import json
import os
import re
from difflib import SequenceMatcher
from pathlib import Path
from typing import Any

import pytest
from jsonschema import Draft202012Validator

from opendoc_engine.exporters.markdown import to_markdown
from opendoc_engine.ir import Document, EngineInfo, Page, Source
from opendoc_engine.methods import build_server

REPO = Path(__file__).resolve().parents[2]
GOLDEN = Path(__file__).parent / "golden"
IMAGES = GOLDEN / "images"
EXPECTED = GOLDEN / "expected"
SCHEMA = json.loads((REPO / "engine" / "schema" / "ir.schema.json").read_text("utf-8"))

ENABLED = os.environ.get("OPENDOC_GOLDEN") == "1"
UPDATE = os.environ.get("OPENDOC_GOLDEN_UPDATE") == "1"
MODELS = Path(os.environ.get("OPENDOC_MODELS_DIR", "G:/opendoc-dev/models"))
ENGINE_ID = "paddleocr-structure"
MODEL_ID = "fast-ppocrv6-tiny"

TYPE_SIMILARITY = 0.85
TEXT_SIMILARITY = 0.90
BOX_IOU = 0.85
BOX_SHARE = 0.90
IMAGE_COUNT_SLACK = 1


# --- comparison ---------------------------------------------------------------------------


def normalize_page(page: dict[str, Any]) -> dict[str, Any]:
    """The page IR without machine-specific paths: image and crop paths keep only their name."""
    data = json.loads(json.dumps(page))
    data["sourceImage"] = Path(data["sourceImage"]).name
    for block in data["blocks"]:
        if "figure" in block and "crop" in block["figure"]:
            block["figure"]["crop"] = Path(block["figure"]["crop"]).name
    return data


def page_words(page: dict[str, Any]) -> list[str]:
    """Every word of the page's text blocks, lists and table cells in reading order."""
    words: list[str] = []

    def add(text: str | None) -> None:
        if text:
            words.extend(re.findall(r"\w+", text.lower()))

    def walk(block: dict[str, Any]) -> None:
        add(block.get("text"))
        add(block.get("latex"))
        for item in block.get("items", []):
            walk_item(item)
        for cell in block.get("table", {}).get("cells", []):
            for inner in cell.get("blocks", []):
                walk(inner)

    def walk_item(item: dict[str, Any]) -> None:
        add(item.get("text"))
        for child in item.get("children", []):
            walk_item(child)

    for block in page["blocks"]:
        walk(block)
    return words


def similarity(a: list[str], b: list[str]) -> float:
    if not a and not b:
        return 1.0
    return SequenceMatcher(None, a, b, autojunk=False).ratio()


def iou(a: list[float], b: list[float]) -> float:
    width = min(a[2], b[2]) - max(a[0], b[0])
    height = min(a[3], b[3]) - max(a[1], b[1])
    if width <= 0 or height <= 0:
        return 0.0
    inter = width * height
    union = (a[2] - a[0]) * (a[3] - a[1]) + (b[2] - b[0]) * (b[3] - b[1]) - inter
    return inter / union if union > 0 else 0.0


def compare_pages(expected: dict[str, Any], actual: dict[str, Any]) -> list[str]:
    """What differs beyond the tolerances, as readable lines. Empty when the pages match."""
    problems: list[str] = []
    if (expected["width"], expected["height"]) != (actual["width"], actual["height"]):
        problems.append(
            f"page size {actual['width']}x{actual['height']}, "
            f"expected {expected['width']}x{expected['height']}"
        )
    want = [b["type"] for b in expected["blocks"]]
    got = [b["type"] for b in actual["blocks"]]
    matcher = SequenceMatcher(None, want, got, autojunk=False)
    ratio = matcher.ratio()
    if ratio < TYPE_SIMILARITY:
        problems.append(f"block types are {ratio:.2f} similar (need {TYPE_SIMILARITY}): {got}")
    text = similarity(page_words(expected), page_words(actual))
    if text < TEXT_SIMILARITY:
        problems.append(f"page text is {text:.2f} similar (need {TEXT_SIMILARITY})")
    aligned = [
        (expected["blocks"][m.a + k], actual["blocks"][m.b + k])
        for m in matcher.get_matching_blocks()
        for k in range(m.size)
    ]
    if aligned:
        close = sum(iou(a["bbox"], b["bbox"]) >= BOX_IOU for a, b in aligned)
        if close / len(aligned) < BOX_SHARE:
            problems.append(
                f"only {close} of {len(aligned)} aligned blocks have a box within IoU {BOX_IOU}"
            )
    return problems


def markdown_words(text: str) -> list[str]:
    return re.findall(r"\w+", re.sub(r"!\[\]\([^)]*\)", " ", text).lower())


def compare_markdown(expected: str, actual: str) -> list[str]:
    problems: list[str] = []
    text = similarity(markdown_words(expected), markdown_words(actual))
    if text < TEXT_SIMILARITY:
        problems.append(f"Markdown text is {text:.2f} similar (need {TEXT_SIMILARITY})")
    images = (len(re.findall(r"!\[\]\(", expected)), len(re.findall(r"!\[\]\(", actual)))
    if abs(images[0] - images[1]) > IMAGE_COUNT_SLACK:
        problems.append(f"{images[1]} images in the Markdown, expected {images[0]}")
    return problems


# --- the comparison functions themselves (always run) -------------------------------------


def _block(kind: str, box: list[float], text: str = "") -> dict[str, Any]:
    return {"type": kind, "bbox": box, "text": text}


def _page(*blocks: dict[str, Any]) -> dict[str, Any]:
    return {"width": 1000, "height": 1400, "sourceImage": "x.jpg", "blocks": list(blocks)}


BASE = _page(
    _block("heading", [100, 100, 900, 160], "The first chapter begins here"),
    _block(
        "paragraph", [100, 200, 900, 600], "Some words in a paragraph about nothing much at all"
    ),
    _block("figure", [100, 650, 900, 1000]),
    _block("paragraph", [100, 1050, 900, 1300], "and the page ends with another short paragraph"),
)


def test_a_page_matches_itself():
    assert compare_pages(BASE, BASE) == []


def test_small_box_and_text_drift_is_tolerated():
    moved = json.loads(json.dumps(BASE))
    for block in moved["blocks"]:
        block["bbox"] = [v + 2 for v in block["bbox"]]
    moved["blocks"][1]["text"] = moved["blocks"][1]["text"].replace("nothing", "nothin")
    assert compare_pages(BASE, moved) == []


def test_a_missing_block_type_is_reported():
    changed = json.loads(json.dumps(BASE))
    del changed["blocks"][2:]
    problems = compare_pages(BASE, changed)
    assert any("block types" in p for p in problems)


def test_changed_text_is_reported():
    changed = json.loads(json.dumps(BASE))
    changed["blocks"][1]["text"] = "completely different words appear in this paragraph now"
    changed["blocks"][3]["text"] = "nothing like the original last paragraph here"
    assert any("page text" in p for p in compare_pages(BASE, changed))


def test_moved_boxes_are_reported():
    changed = json.loads(json.dumps(BASE))
    for block in changed["blocks"]:
        block["bbox"] = [v + 200 for v in block["bbox"]]
    assert any("box" in p for p in compare_pages(BASE, changed))


def test_a_different_page_size_is_reported():
    changed = json.loads(json.dumps(BASE))
    changed["width"] = 900
    assert any("page size" in p for p in compare_pages(BASE, changed))


def test_markdown_comparison():
    expected = "# Title\n\nSome text of the page.\n\n![](a_assets/p0-b2.png)\n"
    assert compare_markdown(expected, expected) == []
    assert compare_markdown(expected, "# Title\n\nSome text of the page.\n") == []
    assert compare_markdown(
        expected,
        "# Title\n\nSome text of the page.\n\n![](a/x.png)\n\n![](a/y.png)\n\n![](a/z.png)\n",
    )
    assert compare_markdown(expected, "Nothing\n\nalike\n")


def test_normalize_page_keeps_only_file_names():
    page = {
        "sourceImage": "C:\\tmp\\work\\page.png",
        "blocks": [{"figure": {"crop": "/tmp/crops/p0-b2.png"}}, {}],
    }
    result = normalize_page(page)
    assert result["sourceImage"] == "page.png"
    assert result["blocks"][0]["figure"]["crop"] == "p0-b2.png"


# --- the golden pages (opt in) ------------------------------------------------------------

PAGES = sorted(p.stem for p in IMAGES.glob("*.jpg"))

needs_golden = pytest.mark.skipif(not ENABLED, reason="set OPENDOC_GOLDEN=1 to run")


def test_the_golden_set_is_five_to_eight_pages_under_400_kb():
    assert 5 <= len(PAGES) <= 8
    for name in PAGES:
        assert (IMAGES / f"{name}.jpg").stat().st_size < 400 * 1024, name
    if not UPDATE:
        for name in PAGES:
            assert (EXPECTED / f"{name}.ir.json").is_file(), f"no expected IR for {name}"
            assert (EXPECTED / f"{name}.md").is_file(), f"no expected Markdown for {name}"


class Run:
    """The Fast tier on every golden page, converted once."""

    def __init__(self, root: Path) -> None:
        self.root = root
        self.server = build_server()
        self.ids = iter(range(1, 10_000))
        self.pages: dict[str, dict[str, Any]] = {}
        self.engine: dict[str, Any] = {}

    def call(self, method: str, params: dict[str, Any]) -> dict[str, Any]:
        message = {"jsonrpc": "2.0", "id": next(self.ids), "method": method, "params": params}
        response = json.loads(self.server.handle_line(json.dumps(message)))
        assert "result" in response, response
        return response["result"]

    def convert_all(self) -> None:
        self.call(
            "engine.configure",
            {
                "llamaServerUrl": None,
                "workDir": str(self.root / "work"),
                "modelsDir": str(MODELS),
            },
        )
        for name in PAGES:
            result = self.call(
                "convert.page",
                {
                    "engineId": ENGINE_ID,
                    "modelId": MODEL_ID,
                    "imagePath": str(IMAGES / f"{name}.jpg"),
                    "pageIndex": 0,
                    "irPath": str(self.root / "ir" / f"{name}.json"),
                    "cropsDir": str(self.root / "crops" / name),
                },
            )
            self.engine = result["engine"]
            self.pages[name] = json.loads(Path(result["irPath"]).read_text("utf-8"))

    def document(self, name: str) -> Document:
        return Document(
            source=Source(path=f"{name}.jpg", kind="image", page_count=1),
            engine=EngineInfo.from_dict(self.engine),
            pages=[Page.from_dict(self.pages[name])],
        )

    def markdown(self, name: str) -> str:
        return to_markdown(self.document(name), assets_dir=f"{name}_assets")


@pytest.fixture(scope="module")
def run(tmp_path_factory) -> Run:
    if not (MODELS / MODEL_ID).is_dir():
        pytest.fail(
            f"the Fast tier models are not in {MODELS / MODEL_ID}; set OPENDOC_MODELS_DIR "
            "or run scripts/fetch-models.py"
        )
    current = Run(tmp_path_factory.mktemp("golden"))
    current.convert_all()
    if UPDATE:
        EXPECTED.mkdir(exist_ok=True)
        for name in PAGES:
            (EXPECTED / f"{name}.ir.json").write_text(
                json.dumps(normalize_page(current.pages[name]), ensure_ascii=False, indent=1)
                + "\n",
                encoding="utf-8",
            )
            (EXPECTED / f"{name}.md").write_text(current.markdown(name), encoding="utf-8")
    return current


@needs_golden
@pytest.mark.parametrize("name", PAGES)
def test_the_ir_matches_the_golden_page(run: Run, name: str):
    actual = normalize_page(run.pages[name])
    document = {
        "schemaVersion": "1",
        "source": {"path": f"{name}.jpg", "kind": "image", "pageCount": 1},
        "engine": run.engine,
        "pages": [run.pages[name]],
    }
    errors = list(Draft202012Validator(SCHEMA).iter_errors(document))
    assert not errors, errors[0].message
    expected = json.loads((EXPECTED / f"{name}.ir.json").read_text("utf-8"))
    assert compare_pages(expected, actual) == []


@needs_golden
@pytest.mark.parametrize("name", PAGES)
def test_the_markdown_matches_the_golden_page(run: Run, name: str):
    expected = (EXPECTED / f"{name}.md").read_text("utf-8")
    assert compare_markdown(expected, run.markdown(name)) == []
