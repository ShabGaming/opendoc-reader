"""JSON-RPC 2.0 over stdio, one UTF-8 JSON object per line.

This module is only transport and dispatch. It knows nothing about documents or engines;
methods are plain callables added to a `MethodRegistry` (see `methods.py` for the `engine.*`
set). `RpcServer.handle_line` maps one request line to at most one response line, and `serve`
loops that over a pair of text streams, so tests drive it with `io.StringIO` and the real
process drives it with stdin and stdout.

Stdout carries protocol messages only. Anything diagnostic goes through `logging`, which the
entry point points at stderr.
"""

from __future__ import annotations

import json
import logging
from collections.abc import Callable
from typing import Any, TextIO

log = logging.getLogger(__name__)

PARSE_ERROR = -32700
INVALID_REQUEST = -32600
METHOD_NOT_FOUND = -32601
INVALID_PARAMS = -32602
INTERNAL_ERROR = -32603

# A handler receives the request's `params` (None, a list or a dict) and returns a
# JSON-serializable result. It signals a protocol-level failure by raising `RpcError`.
Handler = Callable[[Any], Any]


class RpcError(Exception):
    def __init__(self, code: int, message: str, data: Any = None) -> None:
        super().__init__(message)
        self.code = code
        self.message = message
        self.data = data

    def to_dict(self) -> dict[str, Any]:
        error: dict[str, Any] = {"code": self.code, "message": self.message}
        if self.data is not None:
            error["data"] = self.data
        return error


class MethodRegistry:
    def __init__(self) -> None:
        self._handlers: dict[str, Handler] = {}

    def register(self, name: str, handler: Handler) -> None:
        if name in self._handlers:
            raise ValueError(f"RPC method already registered: {name}")
        self._handlers[name] = handler

    def method(self, name: str) -> Callable[[Handler], Handler]:
        """Decorator form of `register`."""

        def decorator(handler: Handler) -> Handler:
            self.register(name, handler)
            return handler

        return decorator

    def get(self, name: str) -> Handler | None:
        return self._handlers.get(name)

    def names(self) -> list[str]:
        return sorted(self._handlers)


def _error_response(request_id: Any, code: int, message: str, data: Any = None) -> dict[str, Any]:
    return {"jsonrpc": "2.0", "id": request_id, "error": RpcError(code, message, data).to_dict()}


def _valid_id(value: Any) -> bool:
    return value is None or (isinstance(value, (str, int)) and not isinstance(value, bool))


class RpcServer:
    def __init__(self, methods: MethodRegistry) -> None:
        self._methods = methods

    def handle_line(self, line: str) -> str | None:
        """Handle one request line. Returns the response line, or None when nothing is owed
        (a notification, or a batch made only of notifications)."""
        try:
            payload = json.loads(line)
        except json.JSONDecodeError as exc:
            return self._encode(_error_response(None, PARSE_ERROR, "Parse error", str(exc)))
        return self._encode(self._handle_payload(payload))

    def serve(self, reader: TextIO, writer: TextIO) -> None:
        """Read request lines until EOF, writing each response as one line."""
        for raw in reader:
            line = raw.strip()
            if not line:
                continue
            response = self.handle_line(line)
            if response is not None:
                writer.write(response + "\n")
                writer.flush()

    @staticmethod
    def _encode(message: Any) -> str | None:
        if message is None:
            return None
        return json.dumps(message, ensure_ascii=False, separators=(",", ":"))

    def _handle_payload(self, payload: Any) -> Any:
        if isinstance(payload, list):
            if not payload:
                return _error_response(None, INVALID_REQUEST, "Invalid Request", "empty batch")
            responses = [r for r in (self._handle_single(item) for item in payload) if r]
            return responses or None
        return self._handle_single(payload)

    def _handle_single(self, request: Any) -> dict[str, Any] | None:
        if not isinstance(request, dict):
            return _error_response(None, INVALID_REQUEST, "Invalid Request")

        has_id = "id" in request
        request_id = request.get("id")
        method = request.get("method")
        params = request.get("params")
        well_formed = (
            request.get("jsonrpc") == "2.0"
            and isinstance(method, str)
            and (params is None or isinstance(params, (list, dict)))
            and (not has_id or _valid_id(request_id))
        )
        if not well_formed:
            echoed_id = request_id if has_id and _valid_id(request_id) else None
            return _error_response(echoed_id, INVALID_REQUEST, "Invalid Request")

        handler = self._methods.get(method)
        if handler is None:
            if not has_id:
                return None
            return _error_response(request_id, METHOD_NOT_FOUND, f"Method not found: {method}")

        try:
            result = handler(params)
            json.dumps(result)  # fail here, not while writing, if the result is not JSON
        except RpcError as exc:
            response: dict[str, Any] | None = {
                "jsonrpc": "2.0",
                "id": request_id,
                "error": exc.to_dict(),
            }
        except Exception:
            log.exception("RPC method %s failed", method)
            response = _error_response(request_id, INTERNAL_ERROR, "Internal error")
        else:
            response = {"jsonrpc": "2.0", "id": request_id, "result": result}

        return response if has_id else None
