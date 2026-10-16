"""Atomic file writes for the exporters.

A file is written to a temporary name in its own folder and then moved into place, so a crash
never leaves a half-written file under the final name. `write_new` additionally refuses to
replace an existing file: the move itself fails when the target exists, so two exports racing
for one name cannot both win.
"""

from __future__ import annotations

import contextlib
import errno
import os
import shutil
import tempfile
from pathlib import Path

REASON_EXISTS = "already_exists"
REASON_DISK_FULL = "disk_full"
REASON_PERMISSION = "permission_denied"
REASON_WRITE = "write_failed"


class OutputError(Exception):
    """An output could not be written. `reason` is one of the IO_ERROR reasons of the protocol."""

    def __init__(self, reason: str, path: Path | str, detail: str = "") -> None:
        super().__init__(f"{reason}: {path}" + (f" ({detail})" if detail else ""))
        self.reason = reason
        self.path = str(path)
        self.detail = detail or self.path


def _classify(exc: OSError, path: Path) -> OutputError:
    if isinstance(exc, FileExistsError):
        return OutputError(REASON_EXISTS, path)
    if isinstance(exc, PermissionError):
        return OutputError(REASON_PERMISSION, path, str(exc))
    if exc.errno in (errno.ENOSPC, errno.EDQUOT):
        return OutputError(REASON_DISK_FULL, path, str(exc))
    return OutputError(REASON_WRITE, path, str(exc))


def ensure_free(path: Path) -> None:
    if path.exists():
        raise OutputError(REASON_EXISTS, path)


def _move_new(tmp: Path, dest: Path) -> None:
    """Move `tmp` to `dest`, failing with FileExistsError when `dest` exists."""
    if os.name == "nt":
        os.rename(tmp, dest)  # Windows refuses to rename onto an existing file
        return
    os.link(tmp, dest)  # a hard link fails when dest exists; POSIX rename would replace it
    os.unlink(tmp)


def _write_temp(dest: Path, data: bytes | None, source: Path | None) -> Path:
    dest.parent.mkdir(parents=True, exist_ok=True)
    handle, name = tempfile.mkstemp(dir=dest.parent, prefix=f".{dest.name}.", suffix=".tmp")
    tmp = Path(name)
    try:
        with os.fdopen(handle, "wb") as out:
            if source is not None:
                with source.open("rb") as src:
                    shutil.copyfileobj(src, out)
            else:
                assert data is not None
                out.write(data)
    except BaseException:
        with contextlib.suppress(OSError):
            tmp.unlink()
        raise
    return tmp


def _publish(dest: Path, data: bytes | None, source: Path | None, *, replace: bool) -> None:
    try:
        if not replace:
            ensure_free(dest)
        tmp = _write_temp(dest, data, source)
    except OutputError:
        raise
    except OSError as exc:
        raise _classify(exc, dest) from exc
    try:
        if replace:
            os.replace(tmp, dest)
        else:
            _move_new(tmp, dest)
    except OSError as exc:
        with contextlib.suppress(OSError):
            tmp.unlink()
        raise _classify(exc, dest) from exc


def write_new(dest: Path, data: bytes) -> None:
    """Write `data` to `dest`, which must not exist."""
    _publish(dest, data, None, replace=False)


def copy_new(source: Path, dest: Path) -> None:
    """Copy `source` to `dest`, which must not exist."""
    _publish(dest, None, source, replace=False)


def write_replacing(dest: Path, data: bytes) -> None:
    """Write `data` to `dest`, replacing any existing file in one step."""
    _publish(dest, data, None, replace=True)
