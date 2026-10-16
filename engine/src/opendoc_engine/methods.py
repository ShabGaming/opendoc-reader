"""The RPC method set, wired to an engine registry.

`engine.ping`, `engine.version` and `engine.capabilities` live here. The rest is registered by
one module per area, each exposing `register(...)`: `methods_pages` (input files) and
`methods_convert` (recognition and calibration).
"""

from __future__ import annotations

from typing import Any

from opendoc_engine import __version__, methods_convert, methods_pages
from opendoc_engine.engines import EngineRegistry, default_registry, describe
from opendoc_engine.ir import IR_SCHEMA_VERSION
from opendoc_engine.rpc import MethodRegistry, RpcServer
from opendoc_engine.runtime import Runtime

# Version of the RPC protocol in docs/protocol.md. Rust refuses an engine with another value.
PROTOCOL_VERSION = "1"


def build_methods(engines: EngineRegistry, runtime: Runtime | None = None) -> MethodRegistry:
    runtime = runtime or Runtime()
    methods = MethodRegistry()

    @methods.method("engine.ping", immediate=True)
    def ping(_params: Any) -> str:
        return "pong"

    @methods.method("engine.version")
    def version(_params: Any) -> dict[str, str]:
        return {
            "engine": __version__,
            "irSchema": IR_SCHEMA_VERSION,
            "protocol": PROTOCOL_VERSION,
        }

    @methods.method("engine.capabilities")
    def capabilities(_params: Any) -> list[dict[str, Any]]:
        return [describe(engine) for engine in engines.all()]

    methods_pages.register(methods, runtime)
    methods_convert.register(methods, engines, runtime)
    return methods


def build_server(engines: EngineRegistry | None = None) -> RpcServer:
    engines = engines or default_registry()
    return RpcServer(build_methods(engines), on_shutdown=engines.release_all)
