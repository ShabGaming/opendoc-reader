"""JSON-RPC 2.0 over stdio, one UTF-8 JSON object per line.

This module is transport, dispatch and the concurrency rules of docs/protocol.md 1.2. It knows
nothing about documents or engines; methods are plain callables added to a `MethodRegistry`
(see `methods.py` for the `engine.*` set).

Two ways to drive a server:

- `RpcServer.handle_line` maps one request line to at most one response line, synchronously.
  Tests and batches use it.
- `RpcServer.serve` is the real loop. The calling thread only reads lines. Requests run one at a
  time on a worker thread, a second request while one is in flight is answered with BUSY, and
  the methods registered as `immediate` (`convert.cancel`, `engine.ping`) are answered on the
  reading thread at once, so they work while the worker is busy.

A handler gets the request's `params` and returns a JSON-serializable result. Handlers that
report progress or honour cancellation call `current_call()` to reach the `CallContext` of the
request they are running for.

Stdout carries protocol messages only. Anything diagnostic goes through `logging`, which the
entry point points at stderr.
"""

from __future__ import annotations

import json
import logging
import queue
import threading
from collections.abc import Callable, Iterable, Iterator
from contextlib import contextmanager
from typing import Any, TextIO

log = logging.getLogger(__name__)

PARSE_ERROR = -32700
INVALID_REQUEST = -32600
METHOD_NOT_FOUND = -32601
INVALID_PARAMS = -32602
INTERNAL_ERROR = -32603

# Application errors, in the JSON-RPC range reserved for implementations. The table, with the
# `data.reason` slugs for each code, is in docs/protocol.md.
MODEL_NOT_READY = -32001
LLAMA_SERVER_UNREACHABLE = -32002
OUT_OF_MEMORY = -32003
BAD_INPUT = -32004
CANCELLED = -32005
NOT_CONFIGURED = -32006
BUSY = -32007
RECOGNITION_FAILED = -32008
IO_ERROR = -32009

# A handler receives the request's `params` (None, a list or a dict) and returns a
# JSON-serializable result. It signals a protocol-level failure by raising `RpcError`.
Handler = Callable[[Any], Any]

# How long `serve` waits for the worker to finish after stdin closes, in seconds. Rust kills
# the process after 5 seconds.
SHUTDOWN_GRACE = 4.0


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


class CallContext:
    """What a running request can do besides returning a result: send `convert.progress`
    notifications, notice that it was cancelled, and abort work in progress when that happens."""

    def __init__(
        self,
        request_id: Any = None,
        *,
        notify: Callable[[dict[str, Any]], None] | None = None,
        cancel_request: Callable[[Any], bool] | None = None,
    ) -> None:
        self.request_id = request_id
        self.page_index = 0
        self.warnings: list[str] = []
        self.cancelled = threading.Event()
        self._notify = notify
        self._cancel_request = cancel_request
        self._hooks: list[Callable[[], None]] = []
        self._hooks_lock = threading.Lock()

    def notify(self, method: str, params: dict[str, Any]) -> None:
        if self._notify is not None:
            self._notify({"jsonrpc": "2.0", "method": method, "params": params})

    def stage(self, stage: str, current: int | None = None, total: int | None = None) -> None:
        """Report a pipeline stage as `convert.progress`. It is also a cancellation point."""
        self.check_cancelled()
        params: dict[str, Any] = {
            "requestId": self.request_id,
            "pageIndex": self.page_index,
            "stage": stage,
        }
        if current is not None:
            params["current"] = current
        if total is not None:
            params["total"] = total
        self.notify("convert.progress", params)

    def warn(self, message: str) -> None:
        """Note a degraded but usable result; it ends up in the call's `warnings`."""
        log.warning("%s", message)
        self.warnings.append(message)

    def check_cancelled(self) -> None:
        if self.cancelled.is_set():
            raise RpcError(CANCELLED, "Cancelled", {"reason": "cancelled"})

    @contextmanager
    def on_cancel(self, hook: Callable[[], None]) -> Iterator[None]:
        """Run `hook` (from the reading thread) if the request is cancelled while the block
        runs, for example to close an HTTP connection that the worker is blocked on."""
        with self._hooks_lock:
            self._hooks.append(hook)
        try:
            if self.cancelled.is_set():
                hook()
            yield
        finally:
            with self._hooks_lock:
                if hook in self._hooks:
                    self._hooks.remove(hook)

    def cancel(self) -> None:
        self.cancelled.set()
        with self._hooks_lock:
            hooks = list(self._hooks)
        for hook in hooks:
            try:
                hook()
            except Exception:
                log.debug("cancel hook failed", exc_info=True)

    def cancel_request(self, request_id: Any) -> bool:
        """Cancel another request in flight on the same server."""
        return self._cancel_request(request_id) if self._cancel_request else False


_local = threading.local()


def current_call() -> CallContext:
    """The context of the request running on this thread (a detached one outside a request)."""
    return getattr(_local, "call", None) or CallContext()


class MethodRegistry:
    def __init__(self) -> None:
        self._handlers: dict[str, Handler] = {}
        self._immediate: set[str] = set()

    def register(self, name: str, handler: Handler, *, immediate: bool = False) -> None:
        if name in self._handlers:
            raise ValueError(f"RPC method already registered: {name}")
        self._handlers[name] = handler
        if immediate:
            self._immediate.add(name)

    def method(self, name: str, *, immediate: bool = False) -> Callable[[Handler], Handler]:
        """Decorator form of `register`."""

        def decorator(handler: Handler) -> Handler:
            self.register(name, handler, immediate=immediate)
            return handler

        return decorator

    def get(self, name: str) -> Handler | None:
        return self._handlers.get(name)

    def is_immediate(self, name: str) -> bool:
        return name in self._immediate

    def names(self) -> list[str]:
        return sorted(self._handlers)


def _error_response(request_id: Any, code: int, message: str, data: Any = None) -> dict[str, Any]:
    return {"jsonrpc": "2.0", "id": request_id, "error": RpcError(code, message, data).to_dict()}


def _valid_id(value: Any) -> bool:
    return value is None or (isinstance(value, (str, int)) and not isinstance(value, bool))


def _encode(message: Any) -> str | None:
    if message is None:
        return None
    return json.dumps(message, ensure_ascii=False, separators=(",", ":"))


class _LineWriter:
    """Writes whole lines to the output stream from any thread."""

    def __init__(self, stream: TextIO) -> None:
        self._stream = stream
        self._lock = threading.Lock()
        self._broken = False

    def write(self, message: Any) -> None:
        line = _encode(message)
        if line is None:
            return
        with self._lock:
            if self._broken:
                return
            try:
                self._stream.write(line + "\n")
                self._stream.flush()
            except (OSError, ValueError):
                # The reader is gone; stdin closes next and ends the loop.
                self._broken = True
                log.warning("cannot write to the output stream")


class _Job:
    __slots__ = ("call", "handler", "has_id", "method", "params", "request_id")

    def __init__(self, request_id, has_id, method, handler, params, call) -> None:
        self.request_id = request_id
        self.has_id = has_id
        self.method = method
        self.handler = handler
        self.params = params
        self.call = call


class RpcServer:
    def __init__(self, methods: MethodRegistry, *, on_shutdown: Callable[[], None] | None = None):
        self._methods = methods
        self._on_shutdown = on_shutdown
        self._state = threading.Lock()
        self._inflight: dict[Any, CallContext] = {}
        self._busy = False
        self._writer: _LineWriter | None = None
        self._jobs: queue.SimpleQueue[_Job | None] = queue.SimpleQueue()

    # --- synchronous use -------------------------------------------------------------------

    def handle_line(self, line: str) -> str | None:
        """Handle one request line. Returns the response line, or None when nothing is owed
        (a notification, or a batch made only of notifications)."""
        try:
            payload = json.loads(line)
        except json.JSONDecodeError as exc:
            return _encode(_error_response(None, PARSE_ERROR, "Parse error", str(exc)))
        return _encode(self._handle_payload(payload))

    # --- the real loop ---------------------------------------------------------------------

    def serve(self, reader: Iterable[str], writer: TextIO) -> None:
        """Read request lines until EOF, answering on `writer` as described in the module
        docstring. On EOF the request in flight is cancelled, the worker gets a few seconds to
        finish, and the shutdown hook releases the models."""
        out = _LineWriter(writer)
        self._writer = out
        worker = threading.Thread(target=self._work, args=(out,), name="rpc-worker", daemon=True)
        worker.start()
        try:
            for raw in reader:
                line = raw.strip()
                if line:
                    self._receive(line, out)
        finally:
            self._stop(worker)

    def _receive(self, line: str, out: _LineWriter) -> None:
        try:
            payload = json.loads(line)
        except json.JSONDecodeError as exc:
            out.write(_error_response(None, PARSE_ERROR, "Parse error", str(exc)))
            return
        if not isinstance(payload, dict):
            out.write(self._handle_payload(payload))  # batches and junk: synchronous
            return

        parsed = self._parse_request(payload)
        if isinstance(parsed, dict):
            if parsed:
                out.write(parsed)
            return
        request_id, has_id, method, handler, params = parsed

        call = CallContext(
            request_id if has_id else None,
            notify=out.write,
            cancel_request=self._cancel,
        )
        job = _Job(request_id, has_id, method, handler, params, call)
        if self._methods.is_immediate(method):
            response = self._run(job)
            if has_id:
                out.write(response)
            return

        with self._state:
            if self._busy:
                busy = True
            else:
                busy = False
                self._busy = True
                if has_id:
                    self._inflight[request_id] = call
        if busy:
            if has_id:
                out.write(
                    _error_response(request_id, BUSY, "Engine busy", {"reason": "call_in_flight"})
                )
            return
        self._jobs.put(job)

    def _work(self, out: _LineWriter) -> None:
        while True:
            job = self._jobs.get()
            if job is None:
                return
            response = self._run(job)
            # Free the slot before the response goes out: the next request can arrive the
            # moment Rust reads it, and must not find the engine busy.
            with self._state:
                self._inflight.pop(job.request_id, None)
                self._busy = False
            if job.has_id:
                out.write(response)

    def _run(self, job: _Job) -> dict[str, Any]:
        _local.call = job.call
        try:
            return self._invoke(job.request_id, job.method, job.handler, job.params)
        finally:
            _local.call = None

    def _cancel(self, request_id: Any) -> bool:
        with self._state:
            call = self._inflight.get(request_id)
        if call is None:
            return False
        call.cancel()
        return True

    def _stop(self, worker: threading.Thread) -> None:
        with self._state:
            pending = list(self._inflight.values())
        for call in pending:
            call.cancel()
        self._jobs.put(None)
        worker.join(SHUTDOWN_GRACE)
        if self._on_shutdown is not None:
            try:
                self._on_shutdown()
            except Exception:
                log.exception("shutdown hook failed")

    # --- shared dispatch -------------------------------------------------------------------

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
        parsed = self._parse_request(request)
        if isinstance(parsed, dict):
            return parsed or None
        request_id, has_id, method, handler, params = parsed
        call = CallContext(request_id if has_id else None)
        _local.call = call
        try:
            response = self._invoke(request_id, method, handler, params)
        finally:
            _local.call = None
        return response if has_id else None

    def _parse_request(
        self, request: dict[str, Any]
    ) -> dict[str, Any] | tuple[Any, bool, str, Handler, Any]:
        """Validate a request object. Returns an error response to send as is, or the parts
        `(id, has_id, method, handler, params)` of a request that can be run. A notification
        that cannot be run (unknown method) comes back as an error response too; the callers
        drop it when it has no id."""
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
                return {}  # nothing is owed; callers skip an empty response
            return _error_response(request_id, METHOD_NOT_FOUND, f"Method not found: {method}")
        return request_id, has_id, method, handler, params

    @staticmethod
    def _invoke(request_id: Any, method: str, handler: Handler, params: Any) -> dict[str, Any]:
        try:
            result = handler(params)
            json.dumps(result)  # fail here, not while writing, if the result is not JSON
        except RpcError as exc:
            return {"jsonrpc": "2.0", "id": request_id, "error": exc.to_dict()}
        except Exception:
            log.exception("RPC method %s failed", method)
            return _error_response(request_id, INTERNAL_ERROR, "Internal error")
        return {"jsonrpc": "2.0", "id": request_id, "result": result}
