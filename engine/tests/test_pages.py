"""pdf.inspect, image.inspect and page.render on small generated files."""

import json
from pathlib import Path

import pytest
from PIL import Image, ImageDraw, ImageFont

from opendoc_engine.methods import build_server
from pdfgen import PageSpec, Text, make_pdf


@pytest.fixture
def server():
    return build_server()


def call(server, method: str, params: dict | None = None) -> dict:
    request = {"jsonrpc": "2.0", "id": 1, "method": method}
    if params is not None:
        request["params"] = params
    return json.loads(server.handle_line(json.dumps(request)))


def ok(response: dict):
    assert "error" not in response, response
    return response["result"]


def reason(response: dict) -> tuple[int, str]:
    return response["error"]["code"], response["error"]["data"]["reason"]


@pytest.fixture
def pdf(tmp_path) -> Path:
    path = tmp_path / "doc.pdf"
    body = "The quick brown fox jumps over the lazy dog."
    path.write_bytes(
        make_pdf(
            [
                PageSpec([Text(72, 700, 12, body), Text(72, 686, 12, body)]),
                PageSpec([], width=300, height=400),
            ]
        )
    )
    return path


# --- pdf.inspect -------------------------------------------------------------------------


def test_pdf_inspect_reports_pages_and_text_layers(server, pdf):
    result = ok(call(server, "pdf.inspect", {"path": str(pdf)}))
    assert result["pageCount"] == 2
    first, second = result["pages"]
    assert (first["index"], first["width"], first["height"]) == (0, 612.0, 792.0)
    assert first["hasTextLayer"] is True
    assert first["charCount"] == 2 * len("Thequickbrownfoxjumpsoverthelazydog.")
    assert (second["width"], second["height"]) == (300.0, 400.0)
    assert second["hasTextLayer"] is False
    assert second["charCount"] == 0


def test_a_few_characters_are_not_a_text_layer(server, tmp_path):
    path = tmp_path / "short.pdf"
    path.write_bytes(make_pdf([PageSpec([Text(72, 700, 12, "Page 1")])]))
    page = ok(call(server, "pdf.inspect", {"path": str(path)}))["pages"][0]
    assert page["charCount"] == 5
    assert page["hasTextLayer"] is False


def test_pdf_inspect_uses_the_size_after_the_pages_own_rotation(server, tmp_path):
    path = tmp_path / "rotated.pdf"
    path.write_bytes(make_pdf([PageSpec([], rotate=90)]))
    page = ok(call(server, "pdf.inspect", {"path": str(path)}))["pages"][0]
    assert (page["width"], page["height"]) == (792.0, 612.0)


def test_pdf_inspect_errors(server, tmp_path):
    assert reason(call(server, "pdf.inspect", {"path": str(tmp_path / "missing.pdf")})) == (
        -32004,
        "file_not_found",
    )
    broken = tmp_path / "broken.pdf"
    broken.write_bytes(b"%PDF-1.4\nnot really")
    assert reason(call(server, "pdf.inspect", {"path": str(broken)})) == (-32004, "corrupt")


def test_pdf_inspect_rejects_bad_parameters(server):
    assert call(server, "pdf.inspect", {})["error"]["data"] == {
        "reason": "missing_param",
        "field": "path",
    }
    assert call(server, "pdf.inspect", {"path": "x", "extra": 1})["error"]["data"]["reason"] == (
        "unknown_param"
    )
    assert call(server, "pdf.inspect", {"path": 5})["error"]["data"]["reason"] == "bad_type"


# --- image.inspect -----------------------------------------------------------------------


@pytest.fixture
def tiff(tmp_path) -> Path:
    path = tmp_path / "scan.tif"
    first = Image.new("RGB", (60, 40), "white")
    second = Image.new("L", (30, 50), 128)
    first.save(path, save_all=True, append_images=[second])
    return path


def test_image_inspect_single_and_multi_page(server, tmp_path, tiff):
    png = tmp_path / "one.png"
    Image.new("RGB", (20, 10)).save(png)
    assert ok(call(server, "image.inspect", {"path": str(png)})) == {
        "pageCount": 1,
        "pages": [{"index": 0, "width": 20, "height": 10}],
    }
    result = ok(call(server, "image.inspect", {"path": str(tiff)}))
    assert result["pageCount"] == 2
    assert [(p["width"], p["height"]) for p in result["pages"]] == [(60, 40), (30, 50)]


def test_image_inspect_errors(server, tmp_path):
    assert reason(call(server, "image.inspect", {"path": str(tmp_path / "no.png")})) == (
        -32004,
        "file_not_found",
    )
    text = tmp_path / "notes.png"
    text.write_text("this is not an image")
    assert reason(call(server, "image.inspect", {"path": str(text)})) == (
        -32004,
        "unsupported_format",
    )


# --- page.render -------------------------------------------------------------------------


def render(server, source: Path, out: Path, **params) -> dict:
    return call(
        server,
        "page.render",
        {"source": str(source), "pageIndex": 0, "outPath": str(out), **params},
    )


def test_pdf_page_renders_at_200_dpi_by_default(server, pdf, tmp_path):
    out = tmp_path / "work" / "pages" / "page-0000.png"  # parent folders do not exist yet
    result = ok(render(server, pdf, out))
    assert result["path"] == str(out)
    assert (result["width"], result["height"]) == (1700, 2200)  # 612x792 pt at 200 dpi
    assert result["dpi"] == 200
    assert (result["rotation"], result["skewAngle"]) == (0, 0.0)
    with Image.open(out) as image:
        assert image.size == (1700, 2200)
        assert image.mode == "RGB"
    assert not list(out.parent.glob(".*.tmp"))


def test_pdf_page_renders_at_a_chosen_dpi(server, pdf, tmp_path):
    result = ok(render(server, pdf, tmp_path / "p.png", dpi=72))
    assert (result["width"], result["height"], result["dpi"]) == (612, 792, 72)


def test_pdf_text_is_actually_drawn(server, pdf, tmp_path):
    out = tmp_path / "p.png"
    ok(render(server, pdf, out, deskew=False, rotate=0))
    with Image.open(out) as image:
        extrema = image.convert("L").getextrema()
    assert extrema[0] < 100  # dark glyph pixels on the white page


def test_image_page_keeps_its_size_and_reports_no_dpi(server, tmp_path, tiff):
    result = ok(render(server, tiff, tmp_path / "f1.png", rotate=0, deskew=False))
    assert (result["width"], result["height"], result["dpi"]) == (60, 40, None)
    second = ok(
        call(
            server,
            "page.render",
            {"source": str(tiff), "pageIndex": 1, "outPath": str(tmp_path / "f2.png")},
        )
    )
    assert (second["width"], second["height"]) == (30, 50)


def test_exif_orientation_is_applied(server, tmp_path):
    source = tmp_path / "photo.jpg"
    image = Image.new("RGB", (40, 20), "white")
    exif = Image.Exif()
    exif[0x0112] = 6  # rotate 90 degrees clockwise to display
    image.save(source, exif=exif)
    result = ok(render(server, source, tmp_path / "o.png", deskew=False))
    assert (result["width"], result["height"]) == (20, 40)


def test_explicit_rotation_is_clockwise(server, tmp_path):
    source = tmp_path / "wide.png"
    image = Image.new("RGB", (40, 20), "white")
    image.putpixel((0, 0), (255, 0, 0))  # top-left corner
    image.save(source)
    out = tmp_path / "r.png"
    result = ok(render(server, source, out, rotate=90, deskew=False))
    assert (result["rotation"], result["width"], result["height"]) == (90, 20, 40)
    with Image.open(out) as rotated:
        assert rotated.getpixel((19, 0)) == (255, 0, 0)  # top-left went to top-right


def test_auto_rotation_without_the_model_leaves_the_page(server, tmp_path):
    source = tmp_path / "a.png"
    Image.new("RGB", (40, 20), "white").save(source)
    ok(call(server, "engine.configure", _configure(tmp_path)))
    assert ok(render(server, source, tmp_path / "o.png", deskew=False))["rotation"] == 0


def _configure(tmp_path: Path) -> dict:
    return {
        "llamaServerUrl": None,
        "workDir": str(tmp_path / "work"),
        "modelsDir": str(tmp_path / "models"),
    }


def _text_page(angle: float) -> Image.Image:
    page = Image.new("RGB", (900, 1200), "white")
    draw = ImageDraw.Draw(page)
    font = ImageFont.load_default(size=26)
    for row in range(26):
        draw.text(
            (70, 90 + row * 40), "Lorem ipsum dolor sit amet, consectetur " * 2, "black", font
        )
    return page.rotate(angle, resample=Image.Resampling.BICUBIC, fillcolor="white")


@pytest.mark.parametrize("angle", [3.0, -4.5])
def test_deskew_straightens_a_skewed_page(server, tmp_path, angle):
    source = tmp_path / "skewed.png"
    _text_page(angle).save(source)
    result = ok(render(server, source, tmp_path / "out.png", rotate=0))
    # The page was turned by `angle` counterclockwise; the correction is the opposite turn.
    assert result["skewAngle"] == pytest.approx(-angle, abs=0.5)


def test_a_straight_page_and_a_blank_page_are_left_alone(server, tmp_path):
    straight = tmp_path / "straight.png"
    _text_page(0).save(straight)
    assert ok(render(server, straight, tmp_path / "a.png", rotate=0))["skewAngle"] == 0.0
    blank = tmp_path / "blank.png"
    Image.new("RGB", (300, 300), "white").save(blank)
    assert ok(render(server, blank, tmp_path / "b.png", rotate=0))["skewAngle"] == 0.0


def test_dewarp_without_the_model_is_model_not_ready(server, tmp_path):
    source = tmp_path / "a.png"
    Image.new("RGB", (40, 20), "white").save(source)
    ok(call(server, "engine.configure", _configure(tmp_path)))
    assert reason(render(server, source, tmp_path / "o.png", dewarp=True)) == (
        -32001,
        "files_missing",
    )


def test_render_errors(server, pdf, tmp_path):
    out = tmp_path / "o.png"
    assert reason(render(server, tmp_path / "nope.pdf", out)) == (-32004, "file_not_found")
    assert reason(call(server, "page.render", _args(pdf, out, pageIndex=5))) == (
        -32004,
        "page_out_of_range",
    )
    assert call(server, "page.render", _args(pdf, out, dpi=10))["error"]["code"] == -32602
    assert call(server, "page.render", _args(pdf, out, rotate=45))["error"]["code"] == -32602
    assert call(server, "page.render", _args(pdf, out, rotate=True))["error"]["code"] == -32602
    assert not out.exists()


def _args(source: Path, out: Path, **extra) -> dict:
    return {"source": str(source), "pageIndex": 0, "outPath": str(out), **extra}


def test_unwritable_output_is_an_io_error(server, pdf, tmp_path):
    blocker = tmp_path / "file"
    blocker.write_text("x")
    code, why = reason(render(server, pdf, blocker / "sub" / "p.png"))
    assert code == -32009
    assert why in {"write_failed", "permission_denied"}
