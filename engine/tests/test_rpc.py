import io
import json

import pytest

from opendoc_engine import __version__
from opendoc_engine.methods import build_server
from opendoc_engine.rpc import MethodRegistry, RpcError, RpcServer


@pytest.fixture
def server() -> RpcServer:
    return build_server()


def call(server: RpcServer, message: object) -> dict | list | None:
    line = message if isinstance(message, str) else json.dumps(message)
    response = server.handle_line(line)
    return None if response is None else json.loads(response)


def test_ping(server):
    assert call(server, {"jsonrpc": "2.0", "id": 1, "method": "engine.ping"}) == {
        "jsonrpc": "2.0",
        "id": 1,
        "result": "pong",
    }


def test_version(server):
    response = call(server, {"jsonrpc": "2.0", "id": "a", "method": "engine.version"})
    assert response["id"] == "a"
    assert response["result"] == {"engine": __version__, "irSchema": "1", "protocol": "1"}


def test_capabilities_lists_demo_engine(server):
    response = call(server, {"jsonrpc": "2.0", "id": 2, "method": "engine.capabilities"})
    engines = {e["id"]: e for e in response["result"]}
    assert engines["demo"]["capabilities"]["tables"] is True
    assert engines["demo"]["manifestModels"] == []


def test_unknown_method(server):
    response = call(server, {"jsonrpc": "2.0", "id": 3, "method": "engine.nope"})
    assert response["id"] == 3
    assert response["error"]["code"] == -32601


def test_parse_error(server):
    response = call(server, "{not json")
    assert response["id"] is None
    assert response["error"]["code"] == -32700


@pytest.mark.parametrize(
    "message",
    [
        {"jsonrpc": "1.0", "id": 1, "method": "engine.ping"},
        {"jsonrpc": "2.0", "id": 1},
        {"jsonrpc": "2.0", "id": 1, "method": "engine.ping", "params": 5},
        {"jsonrpc": "2.0", "id": [1], "method": "engine.ping"},
        [],
        42,
    ],
)
def test_invalid_request(server, message):
    assert call(server, message)["error"]["code"] == -32600


def test_notification_gets_no_response(server):
    assert call(server, {"jsonrpc": "2.0", "method": "engine.ping"}) is None
    assert call(server, {"jsonrpc": "2.0", "method": "engine.nope"}) is None


def test_batch(server):
    batch = [
        {"jsonrpc": "2.0", "id": 1, "method": "engine.ping"},
        {"jsonrpc": "2.0", "method": "engine.ping"},
        {"jsonrpc": "2.0", "id": 2, "method": "engine.nope"},
    ]
    responses = call(server, batch)
    assert [r["id"] for r in responses] == [1, 2]


def test_handler_exception_becomes_internal_error():
    methods = MethodRegistry()

    def boom(_params):
        raise RuntimeError("secret detail")

    methods.register("boom", boom)
    response = call(RpcServer(methods), {"jsonrpc": "2.0", "id": 1, "method": "boom"})
    assert response["error"]["code"] == -32603
    assert "secret detail" not in json.dumps(response)


def test_rpc_error_passes_through():
    methods = MethodRegistry()

    def bad(_params):
        raise RpcError(-32602, "Invalid params", {"field": "x"})

    methods.register("bad", bad)
    response = call(RpcServer(methods), {"jsonrpc": "2.0", "id": 1, "method": "bad"})
    assert response["error"] == {
        "code": -32602,
        "message": "Invalid params",
        "data": {"field": "x"},
    }


def test_duplicate_method_registration_rejected():
    methods = MethodRegistry()
    methods.register("m", lambda _p: None)
    with pytest.raises(ValueError):
        methods.register("m", lambda _p: None)


def test_serve_loop_over_streams(server):
    lines = [
        {"jsonrpc": "2.0", "id": 1, "method": "engine.ping"},
        {"jsonrpc": "2.0", "method": "engine.ping"},
        "",
        "garbage",
        {"jsonrpc": "2.0", "id": 2, "method": "engine.version"},
    ]
    reader = io.StringIO(
        "\n".join(m if isinstance(m, str) else json.dumps(m) for m in lines) + "\n"
    )
    writer = io.StringIO()
    server.serve(reader, writer)

    out = [json.loads(line) for line in writer.getvalue().splitlines()]
    assert [r["id"] for r in out] == [1, None, 2]
    assert out[1]["error"]["code"] == -32700
