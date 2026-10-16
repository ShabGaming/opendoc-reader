"""Atomic file output (docs/protocol.md 1.3): write to a temporary file in the target's folder,
then rename, so a crash never leaves a half-written file that looks valid."""

from __future__ import annotations

import contextlib
import json
import os
from collections.abc import Callable
from pathlib import Path
from typing import IO, Any

from PIL import Image

from opendoc_engine.errors import from_os_error, io_error


def write_atomic(path: str | Path, write: Callable[[IO[bytes]], None]) -> None:
    """Create the parent folders, call `write` with a binary file object, and move the result
    to `path`. Failures become IO_ERROR with the protocol's reason."""
    target = Path(path)
    temporary = target.with_name(f".{target.name}.{os.getpid()}.tmp")
    try:
        target.parent.mkdir(parents=True, exist_ok=True)
        with temporary.open("wb") as handle:
            write(handle)
        os.replace(temporary, target)
    except OSError as exc:
        with contextlib.suppress(OSError):
            temporary.unlink()
        if isinstance(exc, FileExistsError | NotADirectoryError):
            # A file is in the way of a parent folder; this write replaces its target anyway.
            raise io_error("write_failed", f"{target}: {exc}") from exc
        raise from_os_error(exc, target) from exc
    except BaseException:
        with contextlib.suppress(OSError):
            temporary.unlink()
        raise


def write_json(path: str | Path, data: Any) -> None:
    payload = json.dumps(data, ensure_ascii=False, indent=2).encode("utf-8")
    write_atomic(path, lambda handle: handle.write(payload))


def write_png(path: str | Path, image: Image.Image) -> None:
    write_atomic(path, lambda handle: image.save(handle, format="PNG"))
