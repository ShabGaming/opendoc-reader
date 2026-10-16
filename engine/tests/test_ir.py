import json

import pytest

from opendoc_engine.engines import DemoEngine
from opendoc_engine.ir import (
    Block,
    BlockType,
    Document,
    EngineInfo,
    ListItem,
    Page,
    Source,
    Table,
    TableCell,
)


def sample_document() -> Document:
    blocks = DemoEngine().run("page.png", (1000, 1400))
    blocks.append(
        Block(
            "b4",
            BlockType.LIST,
            (10, 20, 30, 40),
            4,
            items=[ListItem("one", True, [ListItem("nested")]), ListItem("two", True)],
        )
    )
    blocks.append(
        Block(
            "b5", BlockType.FIGURE, (0, 0, 5, 5), 5, figure_crop="crops/p0-b5.png", confidence=0.9
        )
    )
    blocks.append(Block("b6", BlockType.CAPTION, (0, 6, 5, 8), 6, text="Fig 1", caption_for="b5"))
    return Document(
        source=Source("C:/scans/a.pdf", "pdf", 1),
        engine=EngineInfo("demo", "none", "0.1.0"),
        pages=[Page(0, 1000, 1400, "pages/0.png", blocks)],
    )


def test_round_trip_through_json():
    doc = sample_document()
    restored = Document.from_dict(json.loads(json.dumps(doc.to_dict())))
    assert restored == doc


def test_json_keys_are_camel_case_and_optional_fields_omitted():
    data = sample_document().to_dict()
    assert data["schemaVersion"] == "1"
    assert data["source"]["pageCount"] == 1
    page = data["pages"][0]
    assert page["sourceImage"] == "pages/0.png"
    heading = page["blocks"][0]
    assert heading["bbox"] == [100.0, 70.0, 900.0, 140.0]
    assert "latex" not in heading and "table" not in heading
    cell = page["blocks"][2]["table"]["cells"][0]
    assert cell["colSpan"] == 2 and cell["rowSpan"] == 1 and cell["header"] is True
    assert page["blocks"][-2]["figure"] == {"crop": "crops/p0-b5.png"}
    assert page["blocks"][-1]["caption"] == {"for": "b5"}


def test_table_merged_cell_detection():
    plain = Table(1, 1, [TableCell(0, 0)])
    merged = Table(1, 2, [TableCell(0, 0, col_span=2)])
    assert not plain.has_merged_cells
    assert merged.has_merged_cells


def test_unknown_schema_version_rejected():
    data = sample_document().to_dict()
    data["schemaVersion"] = "2"
    with pytest.raises(ValueError, match="schemaVersion"):
        Document.from_dict(data)


def test_missing_field_and_bad_bbox_rejected():
    data = sample_document().to_dict()
    del data["pages"][0]["width"]
    with pytest.raises(ValueError, match="width"):
        Document.from_dict(data)

    data = sample_document().to_dict()
    data["pages"][0]["blocks"][0]["bbox"] = [1, 2, 3]
    with pytest.raises(ValueError, match="bbox"):
        Document.from_dict(data)
