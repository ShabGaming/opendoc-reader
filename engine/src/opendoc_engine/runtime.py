"""What `engine.configure` tells the engine: llama-server URL, work and models directories, CPU
threads. One `Runtime` lives for the whole process and every method that needs these values
reads them from it."""

from __future__ import annotations

import os
import threading
from dataclasses import dataclass
from pathlib import Path

from opendoc_engine.errors import not_configured


def default_cpu_threads() -> int:
    return max(1, min(os.cpu_count() or 4, 8))


@dataclass(frozen=True)
class Settings:
    llama_server_url: str | None
    work_dir: Path
    models_dir: Path
    cpu_threads: int

    @property
    def scratch_dir(self) -> Path:
        """The engine's own space, `<workDir>/_engine/`."""
        return self.work_dir / "_engine"

    @property
    def llama_base_url(self) -> str:
        """The server's base URL without a trailing slash or `/v1`."""
        if not self.llama_server_url:
            raise not_configured("llama_server_url")
        url = self.llama_server_url.rstrip("/")
        return url.removesuffix("/v1")


class Runtime:
    def __init__(self) -> None:
        self._settings: Settings | None = None
        self._lock = threading.Lock()

    def configure(
        self,
        *,
        llama_server_url: str | None,
        work_dir: str,
        models_dir: str,
        cpu_threads: int | None,
    ) -> Settings:
        settings = Settings(
            llama_server_url=llama_server_url or None,
            work_dir=Path(work_dir),
            models_dir=Path(models_dir),
            cpu_threads=cpu_threads or default_cpu_threads(),
        )
        with self._lock:
            self._settings = settings
        return settings

    def settings(self) -> Settings:
        """The current settings. Raises NOT_CONFIGURED before `engine.configure` was called."""
        with self._lock:
            settings = self._settings
        if settings is None:
            raise not_configured()
        return settings

    def settings_or_none(self) -> Settings | None:
        """The current settings, or None for methods that work without configuration."""
        with self._lock:
            return self._settings
