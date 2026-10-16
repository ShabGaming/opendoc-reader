"""Helpers for the LaTeX that recognition models emit."""

from __future__ import annotations

_MATH_DELIMITERS = (("\\[", "\\]"), ("\\(", "\\)"), ("$$", "$$"), ("$", "$"))


def strip_math(text: str) -> str:
    """The formula source without its `\\[ \\]`, `\\( \\)`, `$$` or `$` delimiters."""
    text = text.strip()
    for opening, closing in _MATH_DELIMITERS:
        if (
            len(text) >= len(opening) + len(closing)
            and text.startswith(opening)
            and text.endswith(closing)
        ):
            return text[len(opening) : -len(closing)].strip()
    return text
