"""Environment and model-loading helpers shared by everything that uses Paddle.

Paddle and PaddleX read their settings from environment variables at import time, so
`prepare_paddle_env` has to run before the first `import paddleocr` or `import paddlex`. The
engine never lets them download anything: every model comes from a local directory that the
downloader filled (docs/protocol.md 1.3).
"""

from __future__ import annotations

import os
from pathlib import Path

from opendoc_engine.errors import model_not_ready

# Files that make a Paddle inference model directory usable.
PADDLE_MODEL_FILES = ("inference.json", "inference.pdiparams", "inference.yml")


def prepare_paddle_env(scratch_dir: Path) -> None:
    """Point PaddleX's cache at the engine's scratch space and turn off its network probe."""
    os.environ["PADDLE_PDX_DISABLE_MODEL_SOURCE_CHECK"] = "True"
    cache = scratch_dir / "paddle-cache"
    cache.mkdir(parents=True, exist_ok=True)
    os.environ["PADDLE_PDX_CACHE_HOME"] = str(cache)


def model_file(models_dir: Path, model_id: str, *parts: str) -> Path:
    """`<modelsDir>/<modelId>/[dir/]name`, the layout the downloader writes."""
    return models_dir.joinpath(model_id, *parts)


def require_files(models_dir: Path, model_id: str, files: list[tuple[str, ...]]) -> None:
    """Raise MODEL_NOT_READY (`files_missing`) naming the first file that is not there."""
    for parts in files:
        path = model_file(models_dir, model_id, *parts)
        if not path.is_file():
            raise model_not_ready("files_missing", str(path))


def paddle_dir_files(directory: str) -> list[tuple[str, ...]]:
    return [(directory, name) for name in PADDLE_MODEL_FILES]


def find_shared_model(models_dir: Path, directory: str) -> Path | None:
    """A Paddle model directory called `directory` inside any installed model's folder, for
    helpers (page orientation, unwarping) that belong to no single recognition model."""
    if not models_dir.is_dir():
        return None
    for candidate in sorted(models_dir.glob(f"*/{directory}")):
        if all((candidate / name).is_file() for name in PADDLE_MODEL_FILES):
            return candidate
    return None
