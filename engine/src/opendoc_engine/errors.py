"""Constructors for the application errors of docs/protocol.md 1.4, and a mapping from the
exceptions that libraries raise to those errors.

Every function returns an `RpcError` to raise. `detail` is for the log (an exception message or
a path); the UI never shows it as is.
"""

from __future__ import annotations

import errno
import logging

from opendoc_engine import rpc
from opendoc_engine.rpc import RpcError

log = logging.getLogger(__name__)


def _error(code: int, message: str, reason: str | None, detail: str | None = None) -> RpcError:
    data: dict[str, str] = {}
    if reason:
        data["reason"] = reason
    if detail:
        data["detail"] = detail
    return RpcError(code, message, data or None)


def invalid_params(reason: str, field: str | None = None) -> RpcError:
    data = {"reason": reason}
    if field:
        data["field"] = field
    return RpcError(rpc.INVALID_PARAMS, "Invalid params", data)


def model_not_ready(reason: str, detail: str | None = None) -> RpcError:
    return _error(rpc.MODEL_NOT_READY, "Model not ready", reason, detail)


def llama_server_unreachable(reason: str, detail: str | None = None) -> RpcError:
    return _error(rpc.LLAMA_SERVER_UNREACHABLE, "llama-server unreachable", reason, detail)


def out_of_memory(reason: str, detail: str | None = None) -> RpcError:
    return _error(rpc.OUT_OF_MEMORY, "Out of memory", reason, detail)


def bad_input(reason: str, detail: str | None = None) -> RpcError:
    return _error(rpc.BAD_INPUT, "Bad input", reason, detail)


def cancelled() -> RpcError:
    return _error(rpc.CANCELLED, "Cancelled", "cancelled")


def not_configured(reason: str = "not_configured") -> RpcError:
    return _error(rpc.NOT_CONFIGURED, "Engine not configured", reason)


def recognition_failed(reason: str, detail: str | None = None) -> RpcError:
    return _error(rpc.RECOGNITION_FAILED, "Recognition failed", reason, detail)


def io_error(reason: str, detail: str | None = None) -> RpcError:
    return _error(rpc.IO_ERROR, "I/O error", reason, detail)


def from_os_error(exc: OSError, path: object = None) -> RpcError:
    """Writing an output failed."""
    detail = f"{path}: {exc}" if path is not None else str(exc)
    if exc.errno == errno.ENOSPC:
        return io_error("disk_full", detail)
    if isinstance(exc, PermissionError) or exc.errno in (errno.EACCES, errno.EPERM):
        return io_error("permission_denied", detail)
    if isinstance(exc, FileExistsError):
        return io_error("already_exists", detail)
    return io_error("write_failed", detail)


_GPU_OOM_MARKERS = ("cuda", "vram", "gpu", "failed to allocate", "alloc")
_OOM_MARKERS = ("out of memory", "not enough memory", "bad_alloc", "cannot allocate memory")


def looks_like_oom(text: str) -> bool:
    lowered = text.lower()
    return any(marker in lowered for marker in _OOM_MARKERS)


def oom_reason(text: str) -> str:
    """`gpu` when the message points at the GPU, else `ram`."""
    lowered = text.lower()
    return "gpu" if any(marker in lowered for marker in _GPU_OOM_MARKERS) else "ram"


def _chain(exc: BaseException):
    seen = set()
    while exc is not None and id(exc) not in seen:
        seen.add(id(exc))
        yield exc
        exc = exc.__cause__ or exc.__context__


def map_exception(exc: BaseException) -> RpcError:
    """Turn an exception raised while running a model into the protocol error that describes
    it. An `RpcError` passes through unchanged. Anything unrecognised becomes a
    RECOGNITION_FAILED with reason `output_unusable` and the message as detail, so the log keeps
    the cause; callers that want INTERNAL_ERROR for programming mistakes should not use this."""
    if isinstance(exc, RpcError):
        return exc
    for item in _chain(exc):
        name = type(item).__name__
        text = str(item)
        if isinstance(item, MemoryError) or looks_like_oom(text) or "ResourceExhausted" in name:
            return out_of_memory(
                oom_reason(text) if not isinstance(item, MemoryError) else "ram", text
            )
        if isinstance(item, ConnectionRefusedError) or name in {
            "ConnectError",
            "APIConnectionError",
        }:
            return llama_server_unreachable("connection_refused", text)
        if isinstance(item, TimeoutError) or "Timeout" in name:
            return llama_server_unreachable("timeout", text)
    return recognition_failed("output_unusable", f"{type(exc).__name__}: {exc}")
