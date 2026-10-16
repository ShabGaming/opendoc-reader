"""Entry point: `python -m opendoc_engine` serves JSON-RPC on stdin and stdout."""

from __future__ import annotations

import logging
import os
import sys
import time
from collections.abc import Callable, Iterator
from typing import TextIO


def claim_stdout() -> TextIO:
    """Reserve the real stdout for protocol messages and send everything else to stderr.

    Paddle and other libraries print to stdout, from Python and from native code. The original
    descriptor is duplicated for the protocol, then descriptor 1 (and `sys.stdout`) are pointed
    at stderr, so nothing but protocol lines can reach the pipe Rust reads.
    """
    sys.stdout.flush()
    protocol_fd = os.dup(1)
    os.dup2(2, 1)
    sys.stdout = sys.stderr
    return open(protocol_fd, "w", encoding="utf-8", newline="\n", buffering=1)


def _chunk_reader(fd: int) -> Callable[[], bytes | None]:
    """A function returning the next bytes from `fd`: b"" at end of input, None when nothing
    has arrived yet, so the caller can wait without a read pending on the handle.

    On Windows a read that is blocked on a pipe stalls every other call that touches the same
    pipe, and importing Paddle in the worker thread does (it hangs until stdin has data). Polling
    with PeekNamedPipe never leaves a read pending. Elsewhere a plain blocking read is fine.
    """
    if sys.platform != "win32":
        return lambda: os.read(fd, 1 << 16)

    import ctypes
    import msvcrt
    from ctypes import wintypes

    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel32.PeekNamedPipe.argtypes = [
        wintypes.HANDLE,
        ctypes.c_void_p,
        wintypes.DWORD,
        ctypes.c_void_p,
        ctypes.POINTER(wintypes.DWORD),
        ctypes.c_void_p,
    ]
    handle = wintypes.HANDLE(msvcrt.get_osfhandle(fd))
    broken_pipe = 109

    def read() -> bytes | None:
        available = wintypes.DWORD()
        if not kernel32.PeekNamedPipe(handle, None, 0, None, ctypes.byref(available), None):
            if ctypes.get_last_error() == broken_pipe:
                return b""
            return os.read(fd, 1 << 16)  # not a pipe (a file or a console): reads do not stall
        if available.value == 0:
            return None
        return os.read(fd, min(available.value, 1 << 16))

    return read


def claim_stdin() -> Iterator[str]:
    """Lines from the real stdin, read from the file descriptor.

    `sys.stdin` is replaced by an empty stream: a thread blocked in `sys.stdin.readline()`
    holds the lock of its buffer, and a library that touches `sys.stdin` meanwhile would wait
    on it forever.
    """
    read = _chunk_reader(0)
    sys.stdin = open(os.devnull, encoding="utf-8")  # noqa: SIM115 - lives for the process

    def lines() -> Iterator[str]:
        pending = b""
        while True:
            try:
                chunk = read()
            except OSError:
                break
            if chunk is None:
                time.sleep(0.01)
                continue
            if not chunk:
                break
            pending += chunk
            *complete, pending = pending.split(b"\n")
            for line in complete:
                yield line.decode("utf-8", "replace")
        if pending:
            yield pending.decode("utf-8", "replace")

    return lines()


def main() -> int:
    protocol = claim_stdout()
    requests = claim_stdin()
    # Logs on stderr only; stdout is reserved for protocol messages.
    logging.basicConfig(
        stream=sys.stderr,
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
    )
    # Imported after stdout is claimed: heavy libraries may print while they load.
    from opendoc_engine.methods import build_server

    build_server().serve(requests, protocol)
    protocol.flush()
    return 0


if __name__ == "__main__":
    status = main()
    sys.stdout.flush()
    sys.stderr.flush()
    # A page that Paddle cannot be interrupted in must not keep the process alive past stdin.
    os._exit(status)
