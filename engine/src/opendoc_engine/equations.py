"""Clean-up of display equations in DeepSeek-OCR output.

DeepSeek-OCR v1 has no separate label for equation numbers: it prints "(2)" inside the
formula, in front of it (`(2) \\qquad y = ...`), after it, or as the start of a text block that
holds a single inline formula. Several display formulas on one page region also arrive glued as
`\\[ a \\] \\[ b \\]`. This module moves the printed number into `Block.number`, turns
number-plus-one-formula text blocks into formula blocks, and joins glued formulas into one
`aligned` environment.
"""

from __future__ import annotations

import re

from opendoc_engine.ir import Block, BlockType

_NUMBER = r"\(\s*(?P<number>\d+(?:\.\d+)?[a-z]?)\s*\)"
_SPACING = r"(?:\\q?quad|\\;|\\,|~|\s)*"
_LEADING = re.compile(rf"^\s*{_NUMBER}{_SPACING}(?P<body>\S.*)$", re.DOTALL)
# A trailing number needs an explicit gap before it, or `f(2)` would lose its argument.
_GAP = r"(?:\\q?quad|\\;|~)"
_TRAILING = re.compile(rf"^(?P<body>.*?\S)\s*{_GAP}{_SPACING}{_NUMBER}\s*$", re.DOTALL)
_NUMBERED_INLINE = re.compile(rf"^\s*{_NUMBER}\s*\\\((?P<body>.*)\\\)\s*$", re.DOTALL)
_DISPLAY_JOIN = re.compile(r"\\\]\s*\\\[")


def _join_displays(latex: str) -> str:
    parts = [part.strip() for part in _DISPLAY_JOIN.split(latex)]
    if len(parts) < 2:
        return latex
    return "\\begin{aligned} " + " \\\\ ".join(parts) + " \\end{aligned}"


def _split_number(latex: str) -> tuple[str, str | None]:
    """The formula without a printed number at its start or end, and that number as printed."""
    for pattern in (_LEADING, _TRAILING):
        match = pattern.match(latex)
        if match and match.group("body").strip():
            return match.group("body").strip(), f"({match.group('number')})"
    return latex, None


_INLINE_PAREN = re.compile(r"\\\((.+?)\\\)", re.DOTALL)
_DISPLAY_BRACKET = re.compile(r"\\\[(.+?)\\\]", re.DOTALL)


def dollar_math(text: str) -> str:
    """Rewrite `\\( x \\)` and `\\[ x \\]` in running text as `$x$` and `$$x$$`, the form the IR
    uses for inline math."""
    text = _DISPLAY_BRACKET.sub(lambda m: f"$${m.group(1).strip()}$$", text)
    return _INLINE_PAREN.sub(lambda m: f"${m.group(1).strip()}$", text)


def tidy_equations(blocks: list[Block]) -> None:
    """Fix the equation blocks of one page in place."""
    for block in blocks:
        if block.type is BlockType.FORMULA and block.latex is not None:
            block.latex = _join_displays(block.latex)
            if block.number is None:
                block.latex, block.number = _split_number(block.latex)
        elif block.type is BlockType.PARAGRAPH and block.text:
            match = _NUMBERED_INLINE.match(block.text)
            if match and "\\)" not in match.group("body") and "\\(" not in match.group("body"):
                block.type = BlockType.FORMULA
                block.latex = match.group("body").strip()
                block.number = f"({match.group('number')})"
                block.text = None
