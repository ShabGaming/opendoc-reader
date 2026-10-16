"""The `engine.*` RPC methods, wired to an engine registry."""

from __future__ import annotations

from typing import Any

from opendoc_engine import __version__
from opendoc_engine.engines import EngineRegistry, default_registry, describe
from opendoc_engine.ir import IR_SCHEMA_VERSION
from opendoc_engine.rpc import MethodRegistry, RpcServer


def build_methods(engines: EngineRegistry) -> MethodRegistry:
    methods = MethodRegistry()

    @methods.method("engine.ping")
    def ping(_params: Any) -> str:
        return "pong"

    @methods.method("engine.version")
    def version(_params: Any) -> dict[str, str]:
        return {"engine": __version__, "irSchema": IR_SCHEMA_VERSION}

    @methods.method("engine.capabilities")
    def capabilities(_params: Any) -> list[dict[str, Any]]:
        return [describe(engine) for engine in engines.all()]

    return methods


def build_server(engines: EngineRegistry | None = None) -> RpcServer:
    return RpcServer(build_methods(engines or default_registry()))
