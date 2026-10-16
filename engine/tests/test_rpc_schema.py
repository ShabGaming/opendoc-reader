"""The RPC schemas in engine/schema/rpc, the examples in docs/protocol-examples.json and the
text of docs/protocol.md must agree with each other and with the engine code."""

import json
import re
from pathlib import Path

import pytest
from jsonschema import Draft202012Validator
from referencing import Registry, Resource
from referencing.jsonschema import DRAFT202012

from opendoc_engine import rpc
from opendoc_engine.methods import build_server

REPO = Path(__file__).resolve().parents[2]
SCHEMA_DIR = REPO / "engine" / "schema"
RPC_DIR = SCHEMA_DIR / "rpc"
EXAMPLES_PATH = REPO / "docs" / "protocol-examples.json"
PROTOCOL_MD = REPO / "docs" / "protocol.md"
MANIFEST = json.loads((REPO / "models" / "manifest.json").read_text(encoding="utf-8"))

# Every request/response method of the protocol, in the order of docs/protocol.md.
METHODS = [
    "engine.ping",
    "engine.version",
    "engine.capabilities",
    "engine.configure",
    "pdf.inspect",
    "image.inspect",
    "page.render",
    "convert.page",
    "text.extract",
    "convert.cancel",
    "document.assemble",
    "export.docx",
    "export.markdown",
    "calibrate.run",
]
NOTIFICATIONS = ["convert.progress"]

IR_SCHEMA = json.loads((SCHEMA_DIR / "ir.schema.json").read_text(encoding="utf-8"))
EXAMPLES = json.loads(EXAMPLES_PATH.read_text(encoding="utf-8"))


def _load_schemas() -> dict[str, dict]:
    schemas = {}
    for path in [SCHEMA_DIR / "ir.schema.json", *sorted(RPC_DIR.glob("*.schema.json"))]:
        schemas[path.name] = json.loads(path.read_text(encoding="utf-8"))
    return schemas


SCHEMAS = _load_schemas()
REGISTRY = Registry().with_resources(
    (s["$id"], Resource.from_contents(s, default_specification=DRAFT202012))
    for s in SCHEMAS.values()
)


def validator(name: str, fragment: str) -> Draft202012Validator:
    """A validator for `#/$defs/<fragment>` of the schema file `name`."""
    ref = f"{SCHEMAS[name]['$id']}#/$defs/{fragment}" if fragment else SCHEMAS[name]["$id"]
    return Draft202012Validator({"$ref": ref}, registry=REGISTRY)


def errors(v: Draft202012Validator, instance) -> list[str]:
    return [
        f"{'/'.join(map(str, e.absolute_path)) or '<root>'}: {e.message}"
        for e in sorted(v.iter_errors(instance), key=lambda e: list(e.absolute_path))
    ]


def method_validator(method: str, part: str) -> Draft202012Validator:
    return validator(f"{method}.schema.json", part)


# --- the schema files -------------------------------------------------------------------------


def test_every_schema_is_valid_draft_2020_12():
    for name, schema in SCHEMAS.items():
        Draft202012Validator.check_schema(schema)
        assert schema["$schema"] == "https://json-schema.org/draft/2020-12/schema", name


def test_there_is_a_schema_for_every_method_and_notification():
    for name in [*METHODS, *NOTIFICATIONS]:
        assert f"{name}.schema.json" in SCHEMAS, name
    extra = {
        n.removesuffix(".schema.json")
        for n in SCHEMAS
        if n.startswith(("engine.", "pdf.", "image.", "page.", "convert.", "text."))
        or n.startswith(("document.", "export.", "calibrate."))
    } - {*METHODS, *NOTIFICATIONS}
    assert not extra, f"schema files for unknown methods: {sorted(extra)}"


@pytest.mark.parametrize("method", METHODS)
def test_method_schemas_have_params_and_result(method):
    defs = SCHEMAS[f"{method}.schema.json"]["$defs"]
    assert set(defs) == {"params", "result"}


def test_ids_match_file_names():
    for name, schema in SCHEMAS.items():
        assert schema["$id"].endswith("/" + name), name


# --- examples validate against the schemas ----------------------------------------------------


def example_pairs(method: str):
    return EXAMPLES[f"{method}.request"], EXAMPLES[f"{method}.response"]


@pytest.mark.parametrize("method", METHODS)
def test_request_and_response_examples_validate(method):
    request, response = example_pairs(method)

    assert errors(validator("envelope.schema.json", "request"), request) == []
    assert request["method"] == method
    assert errors(method_validator(method, "params"), request.get("params")) == []

    assert errors(validator("envelope.schema.json", "successResponse"), response) == []
    assert response["id"] == request["id"]
    assert errors(method_validator(method, "result"), response["result"]) == []


def test_notification_examples_validate():
    keys = [k for k in EXAMPLES if ".notification." in k]
    assert keys
    for key in keys:
        message = EXAMPLES[key]
        assert errors(validator("envelope.schema.json", "notification"), message) == [], key
        assert message["method"] == "convert.progress"
        assert errors(method_validator("convert.progress", "params"), message["params"]) == []
        # A progress notification belongs to the request in the same example group.
        request = EXAMPLES[key.split(".notification.")[0] + ".request"]
        assert message["params"]["requestId"] == request["id"], key


def test_error_examples_validate_and_cover_every_application_code():
    seen = set()
    for key, message in EXAMPLES.items():
        if not key.startswith("error."):
            continue
        assert errors(validator("envelope.schema.json", "errorResponse"), message) == [], key
        assert errors(validator("error.schema.json", "error"), message["error"]) == [], key
        seen.add(message["error"]["code"])
    application = {c for c in _constants() if -32099 <= c <= -32000}
    assert application <= seen, f"no example for codes {sorted(application - seen)}"


def test_every_method_has_examples():
    for method in METHODS:
        assert f"{method}.request" in EXAMPLES
        assert f"{method}.response" in EXAMPLES


def test_review_page_example_holds_a_valid_ir_page():
    page = EXAMPLES["tauri.ReviewPage"]["page"]
    assert errors(validator("ir.schema.json", "Page"), page) == []


# --- consistency with the manifest ------------------------------------------------------------


def test_example_engines_and_models_exist_in_the_manifest():
    models = MANIFEST["models"]
    engine_of = {model_id: model["engine"] for model_id, model in models.items()}

    for key in ["convert.page.request", "calibrate.run.request"]:
        params = EXAMPLES[key]["params"]
        assert params["modelId"] in models, key
        assert engine_of[params["modelId"]] == params["engineId"], key

    listed = {
        e["id"]: e["manifestModels"] for e in EXAMPLES["engine.capabilities.response"]["result"]
    }
    for engine_id in set(engine_of.values()):
        assert engine_id in listed, f"engine.capabilities example lacks {engine_id}"
    for engine_id, model_ids in listed.items():
        for model_id in model_ids:
            assert engine_of[model_id] == engine_id

    info = EXAMPLES["tauri.ModelInfo"]
    assert info["tier"] == models[info["id"]]["tier"]
    assert info["engineId"] == models[info["id"]]["engine"]
    assert info["license"] == models[info["id"]]["license"]
    assert info["downloadSizeBytes"] == sum(f["sizeBytes"] for f in models[info["id"]]["files"])

    for fit in EXAMPLES["tauri.Recommendation"]["fits"]:
        assert fit["modelId"] in models
    assert set(EXAMPLES["tauri.Recommendation"]["fits"][i]["modelId"] for i in range(4)) == set(
        models
    )


# --- the schemas and the engine code agree ----------------------------------------------------


def _constants() -> dict[int, str]:
    codes = {
        value: name
        for name, value in vars(rpc).items()
        if name.isupper() and isinstance(value, int) and not isinstance(value, bool)
    }
    return codes


def test_error_code_enum_matches_rpc_module():
    enum = SCHEMAS["error.schema.json"]["$defs"]["error"]["properties"]["code"]["enum"]
    assert set(enum) == set(_constants())


def test_error_table_in_the_doc_matches_rpc_module():
    text = PROTOCOL_MD.read_text(encoding="utf-8")
    rows = re.findall(r"^\| (-\d+) \| ([A-Z_]+) \|", text, flags=re.MULTILINE)
    assert {(int(code), name) for code, name in rows} == set(_constants().items())


def call(method: str):
    response = build_server().handle_line(json.dumps({"jsonrpc": "2.0", "id": 1, "method": method}))
    return json.loads(response)


@pytest.mark.parametrize("method", ["engine.ping", "engine.version", "engine.capabilities"])
def test_implemented_methods_answer_in_the_schema_shape(method):
    response = call(method)
    assert errors(validator("envelope.schema.json", "successResponse"), response) == []
    assert errors(method_validator(method, "result"), response["result"]) == []


# --- docs/protocol.md quotes the examples file ------------------------------------------------

_BLOCK = re.compile(r"<!-- example: ([\w.\-]+) -->\s*```json\n(.*?)\n```", re.DOTALL)


def test_every_example_in_the_doc_equals_the_examples_file_and_none_is_missing():
    text = PROTOCOL_MD.read_text(encoding="utf-8")
    quoted = {}
    for key, body in _BLOCK.findall(text):
        assert key in EXAMPLES, f"protocol.md quotes unknown example {key!r}"
        assert key not in quoted, f"protocol.md quotes {key!r} twice"
        quoted[key] = json.loads(body)
        assert quoted[key] == EXAMPLES[key], f"protocol.md differs from the examples file: {key}"
    assert set(quoted) == set(EXAMPLES), (
        f"not shown in protocol.md: {sorted(set(EXAMPLES) - set(quoted))}"
    )
