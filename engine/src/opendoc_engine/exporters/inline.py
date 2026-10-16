"""Inline Markdown in IR `text` fields, split into runs for the DOCX exporter.

The IR keeps emphasis, links and inline math as Markdown (`*em*`, `**strong**`, `` `code` ``,
`[label](url)`, `$latex$`). This is a small tokenizer for exactly that subset; it is not a full
CommonMark parser. Anything it does not recognise stays literal text.
"""

from __future__ import annotations

import re
import string
from dataclasses import dataclass
from typing import Literal

SegmentKind = Literal["text", "math", "code", "link"]

_LINK = re.compile(r"\[([^\]\n]+)\]\(([^)\s]+)\)")
_LATEX_HINT = re.compile(r"[\\^_{}]")
_ESCAPABLE = set(string.punctuation)  # CommonMark: a backslash makes any of these literal


@dataclass
class Segment:
    kind: SegmentKind
    text: str
    bold: bool = False
    italic: bool = False
    url: str = ""


def _looks_like_math(body: str) -> bool:
    stripped = body.strip()
    if not stripped:
        return False
    if _LATEX_HINT.search(stripped):
        return True
    # "$5 and $10" is two prices, not math; anything else between dollars is read as math.
    return not stripped[0].isdigit()


def _closing_dollar(text: str, start: int) -> int:
    i = start
    while i < len(text):
        if text[i] == "\\":
            i += 2
            continue
        if text[i] == "$":
            return i
        i += 1
    return -1


def _can_open(text: str, i: int, marker: str) -> bool:
    after = text[i + len(marker) : i + len(marker) + 1]
    before = text[i - 1 : i] if i else ""
    if not after or after.isspace():
        return False
    return not (marker.startswith("_") and before.isalnum())


def _can_close(text: str, i: int, marker: str) -> bool:
    before = text[i - 1 : i] if i else ""
    after = text[i + len(marker) : i + len(marker) + 1]
    if not before or before.isspace():
        return False
    return not (marker.startswith("_") and after.isalnum())


def parse_inline(text: str) -> list[Segment]:
    segments: list[Segment] = []
    buffer: list[str] = []
    bold = italic = False

    def flush() -> None:
        if buffer:
            segments.append(Segment("text", "".join(buffer), bold, italic))
            buffer.clear()

    i = 0
    while i < len(text):
        ch = text[i]
        if ch == "\\" and i + 1 < len(text) and text[i + 1] in _ESCAPABLE:
            buffer.append(text[i + 1])
            i += 2
            continue
        if ch == "$":
            end = _closing_dollar(text, i + 1)
            if end > i + 1 and _looks_like_math(text[i + 1 : end]):
                flush()
                segments.append(Segment("math", text[i + 1 : end].strip()))
                i = end + 1
                continue
        elif ch == "`":
            end = text.find("`", i + 1)
            if end > i + 1:
                flush()
                segments.append(Segment("code", text[i + 1 : end], bold, italic))
                i = end + 1
                continue
        elif ch == "[":
            match = _LINK.match(text, i)
            if match:
                flush()
                segments.append(Segment("link", match.group(1), bold, italic, match.group(2)))
                i = match.end()
                continue
        elif ch in "*_":
            marker = ch * 2 if text.startswith(ch * 2, i) else ch
            toggled = bold if len(marker) == 2 else italic
            usable = (
                _can_close(text, i, marker)
                if toggled
                else _can_open(text, i, marker) and text.find(marker, i + len(marker)) > 0
            )
            if usable:
                flush()
                if len(marker) == 2:
                    bold = not bold
                else:
                    italic = not italic
                i += len(marker)
                continue
        buffer.append(ch)
        i += 1
    flush()
    return segments


def plain_text(text: str) -> str:
    """The text with Markdown markers removed (used for titles)."""
    parts = []
    for seg in parse_inline(text):
        parts.append(f"${seg.text}$" if seg.kind == "math" else seg.text)
    return "".join(parts).strip()


def tidy_math(text: str) -> str:
    """Remove the padding inside `$ ... $` so Markdown renderers accept the inline math."""
    out: list[str] = []
    i = 0
    while i < len(text):
        if text[i] == "\\" and i + 1 < len(text):
            out.append(text[i : i + 2])
            i += 2
            continue
        if text[i] == "$":
            end = _closing_dollar(text, i + 1)
            if end > i + 1 and _looks_like_math(text[i + 1 : end]):
                out.append("$" + text[i + 1 : end].strip() + "$")
                i = end + 1
                continue
        out.append(text[i])
        i += 1
    return "".join(out)
