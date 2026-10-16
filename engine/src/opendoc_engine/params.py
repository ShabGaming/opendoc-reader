"""Validation of RPC `params` against a small field table.

docs/protocol.md 1.1: `params` is an object, optional parameters have stated defaults, and an
unknown parameter is an INVALID_PARAMS error so that a typo does not pass silently.
"""

from __future__ import annotations

from collections.abc import Mapping
from dataclasses import dataclass
from typing import Any

from opendoc_engine.errors import invalid_params

_MISSING: Any = object()


@dataclass(frozen=True)
class Field:
    """One parameter. `kind` is `str`, `path`, `int`, `bool`, `object`, `id` (a JSON-RPC id:
    integer or string) or `nullable_str`. `choices` limits a value to a fixed set."""

    kind: str
    required: bool = True
    default: Any = None
    minimum: int | None = None
    maximum: int | None = None
    choices: tuple[Any, ...] | None = None


def _is_int(value: Any) -> bool:
    return isinstance(value, int) and not isinstance(value, bool)


def _check(name: str, field: Field, value: Any) -> Any:
    kind = field.kind
    ok = {
        "str": isinstance(value, str),
        "path": isinstance(value, str) and bool(value),
        "nullable_str": value is None or isinstance(value, str),
        "int": _is_int(value),
        "bool": isinstance(value, bool),
        "object": isinstance(value, dict),
        "id": _is_int(value) or isinstance(value, str),
        "any": True,
    }[kind]
    if field.choices is not None:
        ok = ok and any(value == c and type(value) is type(c) for c in field.choices)
    if ok and kind == "int":
        ok = (field.minimum is None or value >= field.minimum) and (
            field.maximum is None or value <= field.maximum
        )
    if not ok:
        raise invalid_params("bad_type", name)
    return value


def parse_params(params: Any, spec: Mapping[str, Field]) -> dict[str, Any]:
    """Check `params` against `spec` and return every field, with defaults filled in."""
    if params is None:
        params = {}
    if not isinstance(params, dict):
        raise invalid_params("bad_type")
    for name in params:
        if name not in spec:
            raise invalid_params("unknown_param", name)
    result: dict[str, Any] = {}
    for name, field in spec.items():
        value = params.get(name, _MISSING)
        if value is _MISSING:
            if field.required:
                raise invalid_params("missing_param", name)
            result[name] = field.default
        else:
            result[name] = _check(name, field, value)
    return result
