import json
from pathlib import Path

import pytest

from opendoc_engine import rpc
from opendoc_engine.ir import (
    Block,
    BlockType,
    Document,
    EngineInfo,
    Page,
    Source,
    Table,
)
from opendoc_engine.methods import build_server
from test_docx import cell, page, para, png
from test_rpc_schema import errors, method_validator

BOX = (0.0, 0.0, 850.0, 400.0)
SOURCE = {"path": "C:\\scans\\a.pdf", "kind": "pdf", "pageCount": 2}
ENGINE = {"id": "demo", "model": "none", "version": "0.1.0"}


@pytest.fixture
def server():
    return build_server()


def call(server, method: str, params: dict) -> dict:
    line = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params})
    return json.loads(server.handle_line(line))


def write_page(path: Path, page_obj: Page) -> str:
    path.write_text(json.dumps(page_obj.to_dict()), encoding="utf-8")
    return str(path)


def sample_pages(tmp_path: Path) -> list[Page]:
    (tmp_path / "crops").mkdir()
    crop = png(tmp_path / "crops" / "p0-b1.png")
    first = page(
        0,
        Block("h", BlockType.HEADING, BOX, 0, level=1, text="Title"),
        Block("fig", BlockType.FIGURE, BOX, 0, figure_crop=str(crop)),
        para("t", "text"),
    )
    second = page(1, para("u", "page two"))
    return [first, second]


@pytest.fixture
def document_file(server, tmp_path) -> Path:
    pages = [
        write_page(tmp_path / f"page-{i}.json", p) for i, p in enumerate(sample_pages(tmp_path))
    ]
    out = tmp_path / "document.json"
    response = call(
        server,
        "document.assemble",
        {"source": SOURCE, "engine": ENGINE, "pages": pages, "outPath": str(out)},
    )
    assert "result" in response, response
    return out


# --- document.assemble ---------------------------------------------------------------------


def test_assemble_writes_the_document_and_matches_the_schema(server, tmp_path):
    pages = [write_page(tmp_path / f"p{i}.json", p) for i, p in enumerate(sample_pages(tmp_path))]
    out = tmp_path / "work" / "document.json"
    params = {"source": SOURCE, "engine": ENGINE, "pages": pages, "outPath": str(out)}
    response = call(server, "document.assemble", params)

    assert errors(method_validator("document.assemble", "result"), response["result"]) == []
    assert response["result"] == {"path": str(out), "pageCount": 2}
    document = Document.from_dict(json.loads(out.read_text(encoding="utf-8")))
    assert [p.index for p in document.pages] == [0, 1]
    assert document.engine.id == "demo"
    assert not list(out.parent.glob("*.tmp"))  # the temporary file is gone


def test_assemble_replaces_an_earlier_document_atomically(server, tmp_path):
    pages = [write_page(tmp_path / f"p{i}.json", p) for i, p in enumerate(sample_pages(tmp_path))]
    out = tmp_path / "document.json"
    out.write_text("old", encoding="utf-8")
    call(
        server,
        "document.assemble",
        {"source": SOURCE, "engine": ENGINE, "pages": pages, "outPath": str(out)},
    )
    assert json.loads(out.read_text(encoding="utf-8"))["schemaVersion"] == "1"


def test_assemble_rejects_pages_out_of_order(server, tmp_path):
    a, b = sample_pages(tmp_path)
    files = [write_page(tmp_path / "b.json", b), write_page(tmp_path / "a.json", a)]
    response = call(
        server,
        "document.assemble",
        {"source": SOURCE, "engine": ENGINE, "pages": files, "outPath": str(tmp_path / "d.json")},
    )
    assert response["error"]["code"] == rpc.BAD_INPUT
    assert response["error"]["data"]["reason"] == "corrupt"
    assert not (tmp_path / "d.json").exists()


@pytest.mark.parametrize(
    ("content", "reason"),
    [
        (None, "file_not_found"),
        ("{not json", "corrupt"),
        ('{"index": 0}', "corrupt"),
    ],
)
def test_assemble_reports_bad_page_files(server, tmp_path, content, reason):
    bad = tmp_path / "bad.json"
    if content is not None:
        bad.write_text(content, encoding="utf-8")
    response = call(
        server,
        "document.assemble",
        {
            "source": SOURCE,
            "engine": ENGINE,
            "pages": [str(bad)],
            "outPath": str(tmp_path / "d.json"),
        },
    )
    assert response["error"]["code"] == rpc.BAD_INPUT
    assert response["error"]["data"]["reason"] == reason


def test_assemble_parameter_errors(server, tmp_path):
    good = {
        "source": SOURCE,
        "engine": ENGINE,
        "pages": [str(tmp_path / "x.json")],
        "outPath": str(tmp_path / "d.json"),
    }
    missing = {k: v for k, v in good.items() if k != "engine"}
    assert call(server, "document.assemble", missing)["error"]["data"] == {
        "reason": "missing_param",
        "field": "engine",
    }
    extra = {**good, "extra": 1}
    assert call(server, "document.assemble", extra)["error"]["data"]["reason"] == "unknown_param"
    relative = {**good, "outPath": "d.json"}
    assert call(server, "document.assemble", relative)["error"]["data"] == {
        "reason": "bad_type",
        "field": "outPath",
    }
    empty = {**good, "pages": []}
    assert call(server, "document.assemble", empty)["error"]["data"]["field"] == "pages"


# --- export.docx ---------------------------------------------------------------------------


def test_export_docx_writes_the_file(server, document_file, tmp_path):
    out = tmp_path / "out" / "report.docx"
    response = call(server, "export.docx", {"document": str(document_file), "outputPath": str(out)})
    assert errors(method_validator("export.docx", "result"), response["result"]) == []
    assert response["result"] == {"files": [str(out)], "warnings": []}
    assert out.read_bytes()[:2] == b"PK"
    assert not list(out.parent.glob("*.tmp"))


def test_export_docx_accepts_the_document_object(server, tmp_path):
    document = Document(
        Source("a.pdf", "pdf", 1), EngineInfo("demo", "none", "0.1.0"), [page(0, para("p", "x"))]
    )
    out = tmp_path / "a.docx"
    response = call(server, "export.docx", {"document": document.to_dict(), "outputPath": str(out)})
    assert response["result"]["files"] == [str(out)]


def test_export_docx_never_overwrites(server, document_file, tmp_path):
    out = tmp_path / "report.docx"
    out.write_bytes(b"precious")
    response = call(server, "export.docx", {"document": str(document_file), "outputPath": str(out)})
    assert response["error"]["code"] == rpc.IO_ERROR
    assert response["error"]["data"]["reason"] == "already_exists"
    assert out.read_bytes() == b"precious"


def test_export_docx_bad_documents(server, tmp_path):
    out = tmp_path / "a.docx"
    missing = call(
        server, "export.docx", {"document": str(tmp_path / "none.json"), "outputPath": str(out)}
    )
    assert missing["error"]["data"]["reason"] == "file_not_found"
    broken = tmp_path / "broken.json"
    broken.write_text('{"schemaVersion": "9"}', encoding="utf-8")
    response = call(server, "export.docx", {"document": str(broken), "outputPath": str(out)})
    assert response["error"]["code"] == rpc.BAD_INPUT
    assert response["error"]["data"]["reason"] == "corrupt"
    assert not out.exists()


def test_export_docx_reports_math_warnings(server, tmp_path):
    formula = Block("p1-b4", BlockType.FORMULA, BOX, 0, latex=r"\cancel{x}")
    document = Document(
        Source("a.pdf", "pdf", 1), EngineInfo("demo", "none", "0.1.0"), [page(0, formula)]
    )
    response = call(
        server,
        "export.docx",
        {"document": document.to_dict(), "outputPath": str(tmp_path / "a.docx")},
    )
    assert response["result"]["warnings"][0].startswith("formula p1-b4 kept as LaTeX text")


# --- export.markdown -----------------------------------------------------------------------


def test_export_markdown_writes_text_and_assets(server, document_file, tmp_path):
    out = tmp_path / "out" / "report.md"
    assets = tmp_path / "out" / "report_assets"
    response = call(
        server,
        "export.markdown",
        {"document": str(document_file), "outputPath": str(out), "assetsDir": str(assets)},
    )
    assert errors(method_validator("export.markdown", "result"), response["result"]) == []
    assert response["result"]["files"] == [str(out), str(assets / "p0-b1.png")]
    assert response["result"]["warnings"] == []
    text = out.read_text(encoding="utf-8")
    assert "![](report_assets/p0-b1.png)" in text
    assert "<!-- page 2 -->" in text
    assert (assets / "p0-b1.png").read_bytes() == (tmp_path / "crops" / "p0-b1.png").read_bytes()


def test_export_markdown_without_figures_creates_no_assets_folder(server, tmp_path):
    document = Document(
        Source("a.pdf", "pdf", 1), EngineInfo("demo", "none", "0.1.0"), [page(0, para("p", "x"))]
    )
    out, assets = tmp_path / "a.md", tmp_path / "a_assets"
    response = call(
        server,
        "export.markdown",
        {"document": document.to_dict(), "outputPath": str(out), "assetsDir": str(assets)},
    )
    assert response["result"]["files"] == [str(out)]
    assert not assets.exists()


def test_export_markdown_never_overwrites_the_markdown_file(server, document_file, tmp_path):
    out = tmp_path / "report.md"
    out.write_text("precious", encoding="utf-8")
    response = call(
        server,
        "export.markdown",
        {
            "document": str(document_file),
            "outputPath": str(out),
            "assetsDir": str(tmp_path / "report_assets"),
        },
    )
    assert response["error"]["data"]["reason"] == "already_exists"
    assert out.read_text(encoding="utf-8") == "precious"
    assert not (tmp_path / "report_assets").exists()


def test_export_markdown_never_overwrites_an_asset_and_writes_nothing(
    server, document_file, tmp_path
):
    assets = tmp_path / "report_assets"
    assets.mkdir()
    (assets / "p0-b1.png").write_bytes(b"precious")
    out = tmp_path / "report.md"
    response = call(
        server,
        "export.markdown",
        {"document": str(document_file), "outputPath": str(out), "assetsDir": str(assets)},
    )
    assert response["error"]["code"] == rpc.IO_ERROR
    assert response["error"]["data"]["reason"] == "already_exists"
    assert (assets / "p0-b1.png").read_bytes() == b"precious"
    assert not out.exists()


def test_export_markdown_warns_about_missing_crops(server, tmp_path):
    figure = Block("fig", BlockType.FIGURE, BOX, 0, figure_crop=str(tmp_path / "gone.png"))
    document = Document(
        Source("a.pdf", "pdf", 1), EngineInfo("demo", "none", "0.1.0"), [page(0, figure)]
    )
    out = tmp_path / "a.md"
    response = call(
        server,
        "export.markdown",
        {
            "document": document.to_dict(),
            "outputPath": str(out),
            "assetsDir": str(tmp_path / "a_assets"),
        },
    )
    assert response["result"]["files"] == [str(out)]
    assert "crop image not found" in response["result"]["warnings"][0]


def test_failed_markdown_write_removes_the_assets_it_copied(
    server, document_file, tmp_path, monkeypatch
):
    from opendoc_engine import methods_export

    def fail(dest, data):
        raise methods_export.files.OutputError("disk_full", dest)

    monkeypatch.setattr(methods_export.files, "write_new", fail)
    assets = tmp_path / "report_assets"
    response = call(
        server,
        "export.markdown",
        {
            "document": str(document_file),
            "outputPath": str(tmp_path / "r.md"),
            "assetsDir": str(assets),
        },
    )
    assert response["error"]["data"]["reason"] == "disk_full"
    assert not assets.exists()


def test_export_parameter_errors(server, tmp_path):
    response = call(
        server, "export.markdown", {"document": "x", "outputPath": str(tmp_path / "a.md")}
    )
    assert response["error"]["data"] == {"reason": "missing_param", "field": "assetsDir"}
    response = call(server, "export.docx", {"document": 5, "outputPath": str(tmp_path / "a.docx")})
    assert response["error"]["data"] == {"reason": "bad_type", "field": "document"}
    response = call(
        server, "export.docx", {"document": "relative.json", "outputPath": str(tmp_path / "a.docx")}
    )
    assert response["error"]["data"] == {"reason": "bad_type", "field": "document"}


def test_table_documents_export_in_both_formats(server, tmp_path):
    table = Table(1, 2, [cell(0, 0, "a", col_span=2)])
    document = Document(
        Source("a.pdf", "pdf", 1),
        EngineInfo("demo", "none", "0.1.0"),
        [page(0, Block("t", BlockType.TABLE, BOX, 0, table=table))],
    )
    assert "result" in call(
        server,
        "export.docx",
        {"document": document.to_dict(), "outputPath": str(tmp_path / "t.docx")},
    )
    md = call(
        server,
        "export.markdown",
        {
            "document": document.to_dict(),
            "outputPath": str(tmp_path / "t.md"),
            "assetsDir": str(tmp_path / "t_assets"),
        },
    )
    assert md["result"]["files"] == [str(tmp_path / "t.md")]
    assert 'colspan="2"' in (tmp_path / "t.md").read_text(encoding="utf-8")
