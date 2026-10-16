"""What the two Paddle engines (Fast and Accurate) have in common: hooks into a running Paddle
pipeline for progress and cancellation, and the teardown of a loaded pipeline.

Paddle's pipelines are one blocking call per page. To report stages and stop between them, the
engines wrap a few of the pipeline's internal callables (the layout model, the recognizer).
Those are private attributes of PaddleX, so every hook is optional: if an attribute is not
there, the page still converts, just with coarser progress and cancellation.
"""

from __future__ import annotations

import asyncio
import gc
import inspect
import logging
from collections.abc import Callable
from typing import Any

log = logging.getLogger(__name__)


def inner_pipeline(pipeline: Any) -> Any | None:
    """The PaddleX pipeline object behind a PaddleOCR wrapper, or None if its layout changed."""
    outer = getattr(pipeline, "paddlex_pipeline", None)
    return getattr(outer, "_pipeline", outer)


class _Hooked:
    """Stands in for a callable (a function or a model object) and delegates everything else."""

    def __init__(
        self,
        original: Any,
        before: Callable[[], None] | None,
        after: Callable[[Any], None] | None,
    ) -> None:
        self._original = original
        self._before = before
        self._after = after

    def __getattr__(self, name: str) -> Any:
        return getattr(self._original, name)

    def __call__(self, *args: Any, **kwargs: Any) -> Any:
        if self._before is not None:
            self._before()
        result = self._original(*args, **kwargs)
        was_generator = inspect.isgenerator(result)
        if was_generator:
            result = list(result)
        if self._after is not None:
            self._after(result)
        return iter(result) if was_generator else result


def wrap(
    owner: Any,
    attribute: str,
    *,
    before: Callable[[], None] | None = None,
    after: Callable[[Any], None] | None = None,
) -> bool:
    """Replace `owner.attribute` with a proxy that runs `before()`, the original callable, then
    `after(result)`. A generator result is consumed first so that `after` sees the finished
    output. Returns False (and logs) when `owner` has no such attribute."""
    original = getattr(owner, attribute, None)
    if original is None:
        log.debug("cannot hook %s on %s", attribute, type(owner).__name__)
        return False
    setattr(owner, attribute, _Hooked(original, before, after))
    return True


def wrap_stream(
    owner: Any,
    attribute: str,
    *,
    each: Callable[[], None],
    before: Callable[[], None] | None = None,
) -> bool:
    """Like `wrap` for a method that returns a generator: `each()` runs after every item is
    produced and `before()` when the call starts. Exceptions from `each` stop the stream."""
    original = getattr(owner, attribute, None)
    if original is None:
        log.debug("cannot hook %s on %s", attribute, type(owner).__name__)
        return False

    def hooked(*args: Any, **kwargs: Any) -> Any:
        if before is not None:
            before()
        for item in original(*args, **kwargs):
            each()
            yield item

    setattr(owner, attribute, hooked)
    return True


def abort_genai_requests() -> None:
    """Cancel the HTTP requests that a PaddleOCR-VL pipeline has in flight.

    The pipeline sends them from PaddleX's background event loop. Cancelling the tasks closes
    their connections, and the waiting caller gets `CancelledError`. The client stays usable
    for the next page. Does nothing when no loop is running.
    """
    try:
        from paddlex.inference.models.common.genai import get_async_manager

        loop = get_async_manager().loop
    except Exception:
        return
    if loop is None or loop.is_closed():
        return

    def cancel_all() -> None:
        for task in asyncio.all_tasks(loop):
            task.cancel()

    loop.call_soon_threadsafe(cancel_all)


def close_pipeline(pipeline: Any) -> None:
    """Release a loaded PaddleOCR pipeline."""
    try:
        close = getattr(pipeline, "close", None)
        if close is not None:
            close()
    except Exception:
        log.debug("closing the pipeline failed", exc_info=True)
    gc.collect()
