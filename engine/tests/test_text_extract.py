"""text.extract on generated PDFs: blocks, boxes and the IR file it writes."""

import json
from pathlib import Path

import pytest
from jsonschema import Draft202012Validator
from PIL import Image

from opendoc_engine.methods import build_server
from pdfgen import PageSpec, Text, make_pdf

REPO = Path(__file__).resolve().parents[2]
PAGE_SCHEMA = json.loads((REPO / "engine" / "schema" / "ir.schema.json").read_text("utf-8"))
SCALE = 200 / 72

BODY = [
    "Text layers hold the words of a document exactly as it was typed,",
    "so converting them needs no recognition at all and is very fast.",
]


def page_spec() -> PageSpec:
    texts = [
        Text(72, 720, 24, "Annual Report", bold=True),
        Text(72, 680, 12, BODY[0]),
        Text(72, 666, 12, BODY[1]),
        # a second paragraph after a larger gap
        Text(72, 630, 12, "A second paragraph starts here and stays on a single line."),
        Text(72, 590, 18, "1. Scope"),
        Text(90, 560, 12, "- first point of the list"),
        Text(90, 546, 12, "- second point of the list"),
        Text(300, 30, 10, "3"),
    ]
    return PageSpec(texts)


def extract(tmp_path: Path, spec: PageSpec | None = None, **overrides) -> dict:
    pdf = tmp_path / "in.pdf"
    pdf.write_bytes(make_pdf([spec or page_spec()]))
    server = build_server()
    request = {"jsonrpc": "2.0", "id": 1, "method": "page.render"}
    request["params"] = {
        "source": str(pdf),
        "pageIndex": 0,
        "outPath": str(tmp_path / "page.png"),
        "rotate": 0,
        "deskew": False,
    }
    rendered = json.loads(server.handle_line(json.dumps(request)))["result"]
    params = {
        "source": str(pdf),
        "pageIndex": 0,
        "imagePath": str(tmp_path / "page.png"),
        "irPath": str(tmp_path / "ir" / "page-0000.json"),
        **overrides,
    }
    response = json.loads(
        server.handle_line(
            json.dumps({"jsonrpc": "2.0", "id": 2, "method": "text.extract", "params": params})
        )
    )
    response["rendered"] = rendered
    return response


@pytest.fixture
def result(tmp_path):
    response = extract(tmp_path)
    assert "error" not in response, response
    return response["result"], json.loads(Path(response["result"]["irPath"]).read_text("utf-8"))


def test_result_shape_and_engine(result):
    summary, page = result
    assert summary["engine"]["id"] == "pdf-text"
    assert summary["engine"]["model"] == "text-layer"
    assert summary["blockCount"] == len(page["blocks"])
    assert summary["warnings"] == []


def test_page_validates_against_the_ir_schema(result, tmp_path):
    _, page = result
    document = {
        "schemaVersion": "1",
        "source": {"path": "in.pdf", "kind": "pdf", "pageCount": 1},
        "engine": {"id": "pdf-text", "model": "text-layer", "version": "0.1.0"},
        "pages": [page],
    }
    errors = list(Draft202012Validator(PAGE_SCHEMA).iter_errors(document))
    assert not errors, errors[0].message
    assert page["index"] == 0
    assert (page["width"], page["height"]) == (1700, 2200)
    assert [b["order"] for b in page["blocks"]] == list(range(len(page["blocks"])))
    assert [b["id"] for b in page["blocks"]] == [f"p0-b{i}" for i in range(len(page["blocks"]))]


def test_blocks_are_headings_paragraphs_lists_and_a_page_number(result):
    _, page = result
    kinds = [(b["type"], b.get("level")) for b in page["blocks"]]
    assert kinds == [
        ("heading", 1),
        ("paragraph", None),
        ("paragraph", None),
        ("heading", 2),
        ("list", None),
        ("page_number", None),
    ]


def test_lines_of_a_paragraph_are_joined(result):
    _, page = result
    paragraph = page["blocks"][1]
    assert paragraph["text"] == " ".join(BODY)
    assert [line["text"] for line in paragraph["lines"]] == BODY


def test_list_items_lose_their_markers(result):
    _, page = result
    items = next(b for b in page["blocks"] if b["type"] == "list")["items"]
    assert [i["text"] for i in items] == ["first point of the list", "second point of the list"]
    assert not any(i["ordered"] for i in items)


def test_boxes_are_in_image_pixels_with_a_top_left_origin(result):
    _, page = result
    heading = page["blocks"][0]
    x1, y1, x2, y2 = heading["bbox"]
    assert x1 == pytest.approx(72 * SCALE, abs=3)
    # 24 pt text with its baseline 72 pt below the top edge of the 792 pt page
    assert y1 == pytest.approx((792 - 720 - 24 * 0.75) * SCALE, abs=12)
    assert y2 > y1
    assert 0 < x2 < 1700
    top_to_bottom = [b["bbox"][1] for b in page["blocks"]]
    assert top_to_bottom == sorted(top_to_bottom)


def test_rotated_pages_map_boxes_into_the_rotated_image(tmp_path):
    spec = PageSpec(
        [Text(72, 700, 14, "Rotated page text lives near the top left corner.")], rotate=90
    )
    response = extract(tmp_path, spec)
    assert "error" not in response, response
    page = json.loads(Path(response["result"]["irPath"]).read_text("utf-8"))
    assert (page["width"], page["height"]) == (2200, 1700)
    x1, y1, x2, y2 = page["blocks"][0]["bbox"]
    assert x2 - x1 > 10 and y2 - y1 > 10
    assert x1 >= 0 and x2 <= 2200 and y1 >= 0 and y2 <= 1700
    # Text at the left of the unrotated page is at the top of a page turned 90 degrees clockwise.
    assert y1 < 400


def test_a_page_without_text_has_no_blocks(tmp_path):
    response = extract(tmp_path, PageSpec([]))
    assert response["result"]["blockCount"] == 0


def test_image_with_another_shape_is_image_mismatch(tmp_path):
    pdf = tmp_path / "in.pdf"
    pdf.write_bytes(make_pdf([page_spec()]))
    wrong = tmp_path / "wrong.png"
    Image.new("RGB", (1000, 1000), "white").save(wrong)
    response = extract(tmp_path, imagePath=str(wrong))
    assert response["error"]["code"] == -32004
    assert response["error"]["data"]["reason"] == "image_mismatch"


def test_errors(tmp_path):
    missing = extract(tmp_path, imagePath=str(tmp_path / "gone.png"))
    assert missing["error"]["data"]["reason"] == "file_not_found"
    out_of_range = extract(tmp_path, pageIndex=3)
    assert out_of_range["error"]["data"]["reason"] == "page_out_of_range"
    absent = extract(tmp_path, source=str(tmp_path / "gone.pdf"))
    assert absent["error"]["data"]["reason"] == "file_not_found"
