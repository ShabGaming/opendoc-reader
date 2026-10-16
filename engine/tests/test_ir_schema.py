"""engine/schema/ir.schema.json against the Python IR and the code that produces it."""

import copy
import json
from pathlib import Path

import pytest
from jsonschema import Draft202012Validator

from opendoc_engine.engines import DemoEngine
from opendoc_engine.grounding import parse_grounding
from opendoc_engine.ir import (
    Block,
    BlockType,
    Document,
    EngineInfo,
    Line,
    ListItem,
    Page,
    Source,
    Table,
    TableCell,
)
from test_grounding import SAMPLE as GROUNDING_SAMPLE

SCHEMA_PATH = Path(__file__).resolve().parents[1] / "schema" / "ir.schema.json"
SCHEMA = json.loads(SCHEMA_PATH.read_text(encoding="utf-8"))
VALIDATOR = Draft202012Validator(SCHEMA)


def errors_of(instance) -> list[str]:
    return [
        f"{'/'.join(map(str, e.absolute_path)) or '<root>'}: {e.message}"
        for e in sorted(VALIDATOR.iter_errors(instance), key=lambda e: list(e.absolute_path))
    ]


def document(*pages: Page) -> Document:
    return Document(
        source=Source("C:/scans/a.pdf", "pdf", max(len(pages), 1)),
        engine=EngineInfo("demo", "none", "0.1.0"),
        pages=list(pages),
    )


def full_block() -> Block:
    """A block with every optional field set, so schema and dataclass can be compared."""
    return Block(
        "b0",
        BlockType.FORMULA,
        (1, 2, 3, 4),
        0,
        level=2,
        text="t",
        lines=[Line((1, 2, 3, 4), "line")],
        items=[ListItem("a", False, [ListItem("b")])],
        table=Table(1, 1, [TableCell(0, 0, header=True)]),
        latex="x",
        number="(1)",
        figure_crop="C:/crops/c.png",
        caption_for="b9",
        confidence=0.5,
    )


def rich_document() -> Document:
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
    blocks.append(Block("b5", BlockType.FIGURE, (0, 0, 5, 5), 5, figure_crop="C:/c/p0-b5.png"))
    blocks.append(Block("b6", BlockType.CAPTION, (0, 6, 5, 8), 6, text="Fig 1", caption_for="b5"))
    blocks.append(
        Block(
            "b7",
            BlockType.PARAGRAPH,
            (0, 10, 50, 30),
            7,
            text="Two lines",
            lines=[Line((0, 10, 50, 20), "Two"), Line((0, 20, 50, 30), "lines")],
            confidence=0.97,
        )
    )
    blocks.append(Block("b8", BlockType.FORMULA, (0, 40, 5, 50), 8, latex="a=b", number="(2)"))
    return document(Page(0, 1000, 1400, "C:/pages/0.png", blocks))


# --- documents produced by the code validate -------------------------------------------------


def test_schema_is_valid_draft_2020_12():
    Draft202012Validator.check_schema(SCHEMA)


def test_demo_engine_output_validates():
    blocks = DemoEngine().run("page.png", (1000, 1400))
    doc = document(Page(0, 1000, 1400, "C:/pages/0.png", blocks))
    assert errors_of(doc.to_dict()) == []


def test_grounding_parser_output_validates():
    blocks = parse_grounding(GROUNDING_SAMPLE, (1000, 2000), page_index=2)
    doc = document(Page(2, 1000, 2000, "C:/pages/2.png", blocks))
    assert errors_of(doc.to_dict()) == []
    # The sample has a formula with its number attached.
    assert any(b.number == "(1)" for b in blocks)


def test_document_with_every_block_kind_validates():
    assert errors_of(rich_document().to_dict()) == []


def test_empty_document_validates():
    assert errors_of(document().to_dict()) == []


# --- exact round trip -------------------------------------------------------------------------


@pytest.mark.parametrize(
    "doc",
    [
        rich_document(),
        document(Page(2, 1000, 2000, "p.png", parse_grounding(GROUNDING_SAMPLE, (1000, 2000), 2))),
    ],
    ids=["rich", "grounding"],
)
def test_ir_round_trips_exactly_through_json(doc):
    data = json.loads(json.dumps(doc.to_dict()))
    assert errors_of(data) == []
    restored = Document.from_dict(data)
    assert restored == doc
    assert restored.to_dict() == data


def test_lines_and_number_survive_the_round_trip():
    data = rich_document().to_dict()
    blocks = data["pages"][0]["blocks"]
    assert blocks[-2]["lines"] == [
        {"bbox": [0.0, 10.0, 50.0, 20.0], "text": "Two"},
        {"bbox": [0.0, 20.0, 50.0, 30.0], "text": "lines"},
    ]
    assert blocks[-1]["number"] == "(2)"
    restored = Document.from_dict(data)
    assert restored.pages[0].blocks[-1].number == "(2)"
    assert restored.pages[0].blocks[-2].lines[1].text == "lines"


# --- the schema and the dataclasses describe the same thing -----------------------------------


def test_block_properties_match_the_dataclass():
    produced = set(full_block().to_dict())
    declared = set(SCHEMA["$defs"]["Block"]["properties"])
    assert produced == declared


def test_block_types_match_the_enum():
    assert set(SCHEMA["$defs"]["BlockType"]["enum"]) == {t.value for t in BlockType}


def test_schema_version_constant_matches_the_ir_module():
    from opendoc_engine.ir import IR_SCHEMA_VERSION

    assert SCHEMA["properties"]["schemaVersion"]["const"] == IR_SCHEMA_VERSION


# --- the schema rejects bad documents ---------------------------------------------------------


def mutated(mutate) -> dict:
    data = copy.deepcopy(rich_document().to_dict())
    mutate(data)
    return data


@pytest.mark.parametrize(
    ("name", "mutate"),
    [
        ("wrong schema version", lambda d: d.update(schemaVersion="2")),
        ("unknown top-level key", lambda d: d.update(extra=1)),
        ("unknown block key", lambda d: d["pages"][0]["blocks"][0].update(colour="red")),
        ("unknown block type", lambda d: d["pages"][0]["blocks"][0].update(type="banner")),
        ("short bbox", lambda d: d["pages"][0]["blocks"][0].update(bbox=[1, 2, 3])),
        ("heading level 7", lambda d: d["pages"][0]["blocks"][0].update(level=7)),
        ("confidence above 1", lambda d: d["pages"][0]["blocks"][0].update(confidence=1.5)),
        ("table block without table", lambda d: d["pages"][0]["blocks"][2].pop("table")),
        ("list block without items", lambda d: d["pages"][0]["blocks"][4].pop("items")),
        ("formula block without latex", lambda d: d["pages"][0]["blocks"][3].pop("latex")),
        ("line without text", lambda d: d["pages"][0]["blocks"][-2]["lines"][0].pop("text")),
        ("number is not a string", lambda d: d["pages"][0]["blocks"][-1].update(number=1)),
        (
            "cell without spans",
            lambda d: d["pages"][0]["blocks"][2]["table"]["cells"][0].pop("rowSpan"),
        ),
        ("page without image", lambda d: d["pages"][0].pop("sourceImage")),
        ("source kind", lambda d: d["source"].update(kind="docx")),
    ],
)
def test_schema_rejects(name, mutate):
    assert errors_of(mutated(mutate)), f"schema accepted a document with: {name}"
