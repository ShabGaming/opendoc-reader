import json
from pathlib import Path

import pytest
from jsonschema import Draft202012Validator

MODELS_DIR = Path(__file__).resolve().parents[2] / "models"
MANIFEST = MODELS_DIR / "manifest.json"
SCHEMA = MODELS_DIR / "manifest.schema.json"


def test_manifest_matches_schema():
    missing = [p.name for p in (MANIFEST, SCHEMA) if not p.is_file()]
    if missing:
        pytest.skip(f"models/{', models/'.join(missing)} not present yet")

    schema = json.loads(SCHEMA.read_text(encoding="utf-8"))
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))

    Draft202012Validator.check_schema(schema)
    errors = sorted(
        Draft202012Validator(schema).iter_errors(manifest), key=lambda e: list(e.absolute_path)
    )
    assert not errors, "\n".join(
        f"{'/'.join(map(str, e.absolute_path)) or '<root>'}: {e.message}" for e in errors
    )
