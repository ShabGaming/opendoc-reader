"""LaTeX -> MathML -> OMML (Office Math Markup Language), for native Word equations.

LaTeX goes through `latex2mathml` to Presentation MathML, and this module turns that MathML into
OMML elements. The conversion is our own: Microsoft's MML2OMML.XSL is not redistributable.

Covered: identifiers, numbers, operators and text; fractions (also binomials); scripts; roots;
delimiters (`\\left ... \\right`, `mfenced`, matrices in brackets); n-ary operators (sums,
products, integrals, unions) with their operand; limits on named operators (`lim`, `max`);
accents and bars; groups (over and underbraces); matrices; aligned and gathered environments
(as OMML equation arrays); spaces. Anything else raises `UnsupportedMath`, and the caller keeps
the LaTeX text instead.

The emitted subset is checked by `omml_check.check_omml`.
"""

from __future__ import annotations

import re
from collections.abc import Sequence

import latex2mathml.converter
from lxml import etree

M_NS = "http://schemas.openxmlformats.org/officeDocument/2006/math"
W_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
NSMAP = {"m": M_NS, "w": W_NS}

_M = f"{{{M_NS}}}"
_W = f"{{{W_NS}}}"
_XML_SPACE = "{http://www.w3.org/XML/1998/namespace}space"

ZERO_WIDTH_SPACE = "\u200b"  # keeps a required operand from showing as an empty box


class UnsupportedMath(Exception):
    """The formula uses something this converter does not turn into OMML."""


Element = etree._Element

# --- tables ---------------------------------------------------------------------------------

INTEGRALS = set("∫∬∭∮∯∰∱∲∳")
NARY = INTEGRALS | set("∑∏∐⋀⋁⋂⋃⨀⨁⨂⨃⨄⨅⨆⨉")

LIMIT_NAMES = {
    "lim", "limsup", "liminf", "max", "min", "sup", "inf", "arg", "argmax", "argmin", "det",
    "gcd", "Pr", "projlim", "injlim",
}  # fmt: skip

# Characters that end the operand of an n-ary operator when met outside brackets.
STOP_OPERATORS = set("=+−-±∓<>≤≥≠≈≡∼≃≅∝→←↔⇒⇐⇔↦∈∉∋⊂⊃⊆⊇,;:∣") | {"\u2212", "\u00b1"}
OPENERS = {"(": ")", "[": "]", "{": "}", "⟨": "⟩", "⌊": "⌋", "⌈": "⌉", "|": "|", "‖": "‖"}
CLOSERS = set(OPENERS.values())

# Over-accents turned into m:acc, keyed by what latex2mathml emits.
ACCENT_CHARS = {
    "^": "\u0302", "\u02c6": "\u0302", "\u0302": "\u0302",
    "~": "\u0303", "\u02dc": "\u0303", "\u0303": "\u0303",
    "\u00af": "\u0305", "\u0305": "\u0305", "\u0304": "\u0305",
    "\u02d9": "\u0307", "\u0307": "\u0307",
    "\u00a8": "\u0308", "\u0308": "\u0308",
    "\u02d8": "\u0306", "\u0306": "\u0306",
    "\u02c7": "\u030c", "\u030c": "\u030c",
    "\u00b4": "\u0301", "\u0301": "\u0301",
    "`": "\u0300", "\u0300": "\u0300",
    "\u02da": "\u030a", "\u030a": "\u030a",
    "\u2192": "\u20d7", "\u20d7": "\u20d7",
    "\u20db": "\u20db",
}  # fmt: skip
BAR_CHARS = {"\u2015", "\u203e", "\u2500", "\u2014", "_", "\u0332"}
GROUP_CHARS = {"\u23de": "top", "\u23df": "bot"}

_SPACES = {
    "veryverythinmathspace": "\u200a",
    "verythinmathspace": "\u200a",
    "thinmathspace": "\u2009",
    "mediummathspace": "\u2005",
    "thickmathspace": "\u2004",
    "verythickmathspace": "\u2003",
    "veryverythickmathspace": "\u2003",
}

_SCRIPT_TAGS = {"msub", "msup", "msubsup", "munder", "mover", "munderover"}

# --- element helpers ------------------------------------------------------------------------


def _el(tag: str, *children: Element, **attrs: str) -> Element:
    node = etree.Element(_M + tag, nsmap=NSMAP)
    for key, value in attrs.items():
        node.set(_M + key, value)
    node.extend(children)
    return node


def _val(tag: str, value: str) -> Element:
    return _el(tag, val=value)


def _container(tag: str, nodes: Sequence[Element]) -> Element:
    """An `m:e`, `m:num`, ... holder. An empty one gets an invisible character."""
    node = _el(tag)
    node.extend(nodes if nodes else [_run(ZERO_WIDTH_SPACE)])
    return node


def _clean_text(text: str) -> str:
    return "".join(ch for ch in text if ch in "\t\n\r" or ord(ch) >= 0x20)


def _run(text: str, *, sty: str | None = None, nor: bool = False, aln: bool = False) -> Element:
    run = _el("r")
    if sty or nor or aln:
        props = _el("rPr")
        if nor:
            props.append(_el("nor"))
        if sty:
            props.append(_val("sty", sty))
        if aln:
            props.append(_el("aln"))
        run.append(props)
    if not nor:
        fonts = etree.SubElement(
            etree.SubElement(run, _W + "rPr", nsmap=NSMAP), _W + "rFonts", nsmap=NSMAP
        )
        fonts.set(_W + "ascii", "Cambria Math")
        fonts.set(_W + "hAnsi", "Cambria Math")
    t = etree.SubElement(run, _M + "t", nsmap=NSMAP)
    t.text = _clean_text(text)
    if t.text != t.text.strip():
        t.set(_XML_SPACE, "preserve")
    return run


def _local(node: Element) -> str:
    tag = node.tag
    return tag.rsplit("}", 1)[-1] if isinstance(tag, str) else ""


def _text(node: Element) -> str:
    return "".join(node.itertext()).strip()


def _is_op(node: Element, chars: set[str] | None = None) -> bool:
    return _local(node) == "mo" and len(node) == 0 and (chars is None or _text(node) in chars)


# --- LaTeX preparation ----------------------------------------------------------------------

_ALIGN_ENVS = ("aligned", "align*", "align", "split", "gathered", "gather*", "gather", "eqnarray*",
               "eqnarray", "alignedat", "flalign*", "flalign")  # fmt: skip
_ENV_PATTERN = "|".join(re.escape(name) for name in _ALIGN_ENVS)
_WHOLE_ENV = re.compile(
    rf"^\s*\\begin\{{({_ENV_PATTERN})\}}(?:\{{\d+\}})?(.*)\\end\{{\1\}}\s*$", re.DOTALL
)
_ANY_ENV = re.compile(rf"\\begin\{{({_ENV_PATTERN})\}}(?:\{{\d+\}})?(.*?)\\end\{{\1\}}", re.DOTALL)
_TAG = re.compile(r"\\tag\*?\s*\{([^{}]*)\}")
_NOISE = re.compile(r"\\(?:nonumber|notag|label\{[^{}]*\}|displaystyle|textstyle|limits|nolimits)")


def split_tag(latex: str) -> tuple[str, str | None]:
    """Remove `\\tag{..}` from a formula and return it separately."""
    match = _TAG.search(latex)
    if not match:
        return latex, None
    return _TAG.sub("", latex), match.group(1).strip() or None


def _split_top(source: str, separator: str) -> list[str]:
    """Split on `separator` outside braces and nested environments."""
    parts: list[str] = []
    depth = 0
    env = 0
    start = 0
    i = 0
    while i < len(source):
        if source.startswith("\\begin{", i):
            env += 1
        elif source.startswith("\\end{", i):
            env -= 1
        elif source[i] == "\\" and not source.startswith(separator, i):
            i += 2  # skip an escape such as \{ or \&
            continue
        elif source[i] == "{":
            depth += 1
        elif source[i] == "}":
            depth -= 1
        if depth == 0 and env == 0 and source.startswith(separator, i):
            parts.append(source[start:i])
            i += len(separator)
            start = i
            continue
        i += 1
    parts.append(source[start:])
    return parts


def _rows_of(body: str) -> list[list[str]]:
    rows = [_split_top(row, "&") for row in _split_top(body, "\\\\")]
    rows = [[cell.strip() for cell in row] for row in rows]
    return [row for row in rows if any(row)]  # drops the empty row after a trailing \\


def _rewrite_inner_envs(latex: str) -> str:
    """Aligned environments inside a bigger formula become `array`, which latex2mathml has."""

    def replace(match: re.Match[str]) -> str:
        rows = _rows_of(match.group(2))
        columns = max((len(r) for r in rows), default=1)
        spec = ("rl" * columns)[:columns]
        body = " \\\\ ".join(" & ".join(r) for r in rows)
        return f"\\begin{{array}}{{{spec}}}{body}\\end{{array}}"

    return _ANY_ENV.sub(replace, latex)


# --- the converter --------------------------------------------------------------------------


class _Converter:
    def __init__(self, display: bool) -> None:
        self.display = display

    # Entry points

    def math_root(self, mathml: str) -> list[Element]:
        try:
            root = etree.fromstring(mathml.encode("utf-8"))
        except etree.XMLSyntaxError as exc:
            raise UnsupportedMath(f"invalid MathML: {exc}") from exc
        return self.seq(list(root))

    def latex_nodes(self, latex: str) -> list[Element]:
        return self.math_root(_to_mathml(latex))

    # Sequences

    def seq(self, nodes: list[Element]) -> list[Element]:
        out: list[Element] = []
        i = 0
        while i < len(nodes):
            node = nodes[i]
            matched = self._pattern(nodes, i)
            if matched is not None:
                converted, i = matched
                out.extend(converted)
                continue
            info = _nary_info(node)
            if info is not None:
                end = _operand_end(nodes, i + 1)
                out.append(self._nary(info, nodes[i + 1 : end]))
                i = end
                continue
            out.extend(self.one(node))
            i += 1
        return _merge_runs(out)

    def _pattern(self, nodes: list[Element], i: int) -> tuple[list[Element], int] | None:
        first = nodes[i]
        if not _is_op(first):
            return None
        # binomial: ( frac-without-bar )
        if (
            _is_op(first)
            and "minsize" in first.attrib
            and i + 2 < len(nodes)
            and _local(nodes[i + 1]) == "mfrac"
            and nodes[i + 1].get("linethickness") == "0"
            and _is_op(nodes[i + 2])
            and "minsize" in nodes[i + 2].attrib
        ):
            frac = self.one(nodes[i + 1])
            return [self._delimiters(_text(first), _text(nodes[i + 2]), [frac])], i + 3
        # bracketed matrix: ( mtable )
        if (
            _text(first) in OPENERS
            and i + 2 < len(nodes)
            and _local(nodes[i + 1]) == "mtable"
            and _is_op(nodes[i + 2], {OPENERS[_text(first)]})
        ):
            matrix = self.one(nodes[i + 1])
            return [self._delimiters(_text(first), _text(nodes[i + 2]), [matrix])], i + 3
        return None

    # Single elements

    def one(self, node: Element) -> list[Element]:
        tag = _local(node)
        handler = getattr(self, f"_t_{tag}", None)
        if handler is None:
            raise UnsupportedMath(f"MathML element <{tag}> is not supported")
        return handler(node)

    def arg(self, node: Element) -> list[Element]:
        return self.seq([node])

    def _t_math(self, node: Element) -> list[Element]:
        return self.seq(list(node))

    def _t_mrow(self, node: Element) -> list[Element]:
        kids = list(node)
        if len(kids) >= 2 and _is_fence(kids[0]) and _form(kids[0]) == "prefix":
            last = kids[-1]
            if _is_fence(last) and _form(last) == "postfix":
                return [self._fenced(_text(kids[0]), _text(last), kids[1:-1])]
            if len(kids) == 2 and _local(kids[1]) == "mtable":
                return [self._fenced(_text(kids[0]), "", kids[1:])]
        return self.seq(kids)

    def _t_mstyle(self, node: Element) -> list[Element]:
        return self.seq(list(node))

    def _t_mfenced(self, node: Element) -> list[Element]:
        beg = node.get("open", "(")
        end = node.get("close", ")")
        seps = node.get("separators", ",").replace(" ", "")
        parts = [self.arg(child) for child in node]
        return [self._delimiters(beg, end, parts or [[]], seps[:1] if seps else "")]

    def _fenced(self, beg: str, end: str, inner: list[Element]) -> Element:
        return self._delimiters(beg, end, [self.seq(inner)])

    def _delimiters(
        self, beg: str, end: str, parts: list[list[Element]], separator: str = ""
    ) -> Element:
        props = _el("dPr", _val("begChr", beg))
        if separator and len(parts) > 1:
            props.append(_val("sepChr", separator))
        props.append(_val("endChr", end))
        node = _el("d", props)
        node.extend(_container("e", part) for part in parts)
        return node

    def _t_mi(self, node: Element) -> list[Element]:
        if len(node):
            raise UnsupportedMath("<mi> with child elements")
        text = _text(node)
        if not text:
            return []
        sty = {"normal": "p", "bold": "b", "italic": "i", "bold-italic": "bi"}.get(
            node.get("mathvariant", "")
        )
        if sty is None and len(text) > 1 and text.isalpha():
            sty = "p"  # function names such as sin, log
        return [_run(text, sty=sty)]

    def _t_mn(self, node: Element) -> list[Element]:
        text = _text(node)
        return [_run(text)] if text else []

    def _t_mo(self, node: Element) -> list[Element]:
        if len(node):
            raise UnsupportedMath("<mo> with child elements")
        text = _text(node)
        if not text:
            return []
        sty = "p" if len(text) > 1 and text.isalpha() else None
        return [_run(text, sty=sty)]

    def _t_mtext(self, node: Element) -> list[Element]:
        text = "".join(node.itertext())
        return [_run(text, nor=True)] if text else []

    def _t_mspace(self, node: Element) -> list[Element]:
        if node.get("linebreak"):
            raise UnsupportedMath("line break outside an aligned environment")
        text = _space_text(node.get("width", "0"))
        return [_run(text, nor=True)] if text else []

    def _t_mfrac(self, node: Element) -> list[Element]:
        kids = _expect(node, 2)
        props = _el("fPr", _val("type", "noBar")) if node.get("linethickness") == "0" else None
        frac = _el("f")
        if props is not None:
            frac.append(props)
        frac.append(_container("num", self.arg(kids[0])))
        frac.append(_container("den", self.arg(kids[1])))
        return [frac]

    def _t_msqrt(self, node: Element) -> list[Element]:
        inner = self.seq(list(node))
        props = _el("radPr", _val("degHide", "1"))
        return [_el("rad", props, _el("deg"), _container("e", inner))]

    def _t_mroot(self, node: Element) -> list[Element]:
        base, index = _expect(node, 2)
        return [_el("rad", _el("radPr"), _container("deg", self.arg(index)),
                    _container("e", self.arg(base)))]  # fmt: skip

    def _t_msub(self, node: Element) -> list[Element]:
        base, sub = _expect(node, 2)
        if _limit_name(base):
            return [self._lim("limLow", base, sub)]
        return [_el("sSub", _container("e", self.arg(base)), _container("sub", self.arg(sub)))]

    def _t_msup(self, node: Element) -> list[Element]:
        base, sup = _expect(node, 2)
        if _limit_name(base):
            return [self._lim("limUpp", base, sup)]
        return [_el("sSup", _container("e", self.arg(base)), _container("sup", self.arg(sup)))]

    def _t_msubsup(self, node: Element) -> list[Element]:
        base, sub, sup = _expect(node, 3)
        if _limit_name(base):
            return [self._both(base, sub, sup)]
        return [
            _el(
                "sSubSup",
                _container("e", self.arg(base)),
                _container("sub", self.arg(sub)),
                _container("sup", self.arg(sup)),
            )
        ]

    def _t_munder(self, node: Element) -> list[Element]:
        base, under = _expect(node, 2)
        if _local(under) == "mo" and _text(under) in BAR_CHARS:
            return [_el("bar", _el("barPr", _val("pos", "bot")), _container("e", self.arg(base)))]
        if _local(under) == "mo" and _text(under) in GROUP_CHARS:
            return [self._group(base, _text(under))]
        return [self._lim("limLow", base, under)]

    def _t_mover(self, node: Element) -> list[Element]:
        base, over = _expect(node, 2)
        char = _text(over) if _local(over) == "mo" else ""
        if char in BAR_CHARS:
            return [_el("bar", _el("barPr", _val("pos", "top")), _container("e", self.arg(base)))]
        if char in GROUP_CHARS:
            return [self._group(base, char)]
        if char in ACCENT_CHARS:
            props = _el("accPr", _val("chr", ACCENT_CHARS[char]))
            return [_el("acc", props, _container("e", self.arg(base)))]
        return [self._lim("limUpp", base, over)]

    def _t_munderover(self, node: Element) -> list[Element]:
        base, under, over = _expect(node, 3)
        return [self._both(base, under, over)]

    def _group(self, base: Element, char: str) -> Element:
        props = _el("groupChrPr", _val("chr", char), _val("pos", GROUP_CHARS[char]),
                    _val("vertJc", "bot" if GROUP_CHARS[char] == "top" else "top"))  # fmt: skip
        return _el("groupChr", props, _container("e", self.arg(base)))

    def _lim(self, tag: str, base: Element, limit: Element) -> Element:
        return _el(tag, _container("e", self._lim_base(base)), _container("lim", self.arg(limit)))

    def _both(self, base: Element, under: Element, over: Element) -> Element:
        lower = _el(
            "limLow", _container("e", self._lim_base(base)), _container("lim", self.arg(under))
        )
        return _el("limUpp", _container("e", [lower]), _container("lim", self.arg(over)))

    def _lim_base(self, base: Element) -> list[Element]:
        if _local(base) == "mo" and len(base):  # \mathop{...}
            return self.seq(list(base))
        if _local(base) == "mo":
            text = _text(base)
            return [_run(text, sty="p" if text.isalpha() else None)]
        return self.arg(base)

    def _t_mtable(self, node: Element) -> list[Element]:
        rows = [row for row in node if _local(row) == "mtr"]
        if len(rows) != len(node):
            raise UnsupportedMath("<mtable> with a child that is not <mtr>")
        grid: list[list[list[Element]]] = []
        aligns: list[str] = []
        for row in rows:
            cells = [cell for cell in row if _local(cell) == "mtd"]
            if len(cells) != len(row):
                raise UnsupportedMath("<mtr> with a child that is not <mtd>")
            for col, cell in enumerate(cells):
                if col >= len(aligns):
                    aligns.append(cell.get("columnalign", "center"))
            grid.append([self.seq(list(cell)) for cell in cells])
        width = max((len(r) for r in grid), default=0)
        if width == 0:
            raise UnsupportedMath("empty matrix")
        aligns += ["center"] * (width - len(aligns))
        matrix = _el("m")
        if any(a in ("left", "right") for a in aligns):
            columns = _el("mcs")
            for align in aligns:
                columns.append(
                    _el("mc", _el("mcPr", _val("count", "1"),
                                  _val("mcJc", align if align in ("left", "right") else "center")))
                )  # fmt: skip
            matrix.append(_el("mPr", columns))
        for cells in grid:
            padded = cells + [[] for _ in range(width - len(cells))]
            matrix.append(_el("mr", *[_container("e", cell) for cell in padded]))
        return [matrix]

    # N-ary operators

    def _nary(self, info: _NaryInfo, operand: list[Element]) -> Element:
        under_over = info.under_over or (self.display and info.char not in INTEGRALS)
        props = _el("naryPr", _val("chr", info.char),
                    _val("limLoc", "undOvr" if under_over else "subSup"))  # fmt: skip
        if info.sub is None:
            props.append(_val("subHide", "1"))
        if info.sup is None:
            props.append(_val("supHide", "1"))
        node = _el("nary", props)
        node.append(_el("sub") if info.sub is None else _container("sub", self.arg(info.sub)))
        node.append(_el("sup") if info.sup is None else _container("sup", self.arg(info.sup)))
        node.append(_container("e", self.seq(operand)))
        return node


class _NaryInfo:
    def __init__(
        self, char: str, sub: Element | None, sup: Element | None, under_over: bool
    ) -> None:
        self.char = char
        self.sub = sub
        self.sup = sup
        self.under_over = under_over


def _nary_info(node: Element) -> _NaryInfo | None:
    tag = _local(node)
    if tag == "mo" and len(node) == 0 and _text(node) in NARY:
        return _NaryInfo(_text(node), None, None, False)
    if tag not in _SCRIPT_TAGS or len(node) == 0:
        return None
    base = node[0]
    if not (_is_op(base) and _text(base) in NARY):
        return None
    kids = list(node)
    char = _text(base)
    if tag == "msub":
        return _NaryInfo(char, kids[1], None, False)
    if tag == "msup":
        return _NaryInfo(char, None, kids[1], False)
    if tag == "msubsup":
        return _NaryInfo(char, kids[1], kids[2], False)
    if tag == "munder":
        return _NaryInfo(char, kids[1], None, True)
    if tag == "mover":
        return _NaryInfo(char, None, kids[1], True)
    return _NaryInfo(char, kids[1], kids[2], True)


def _operand_end(nodes: list[Element], start: int) -> int:
    """Index after the last sibling that belongs to the operand of an n-ary operator."""
    depth = 0
    k = start
    while k < len(nodes):
        node = nodes[k]
        if _is_op(node):
            char = _text(node)
            if char in OPENERS and char not in CLOSERS:
                depth += 1
            elif char in CLOSERS and char not in OPENERS and depth > 0:
                depth -= 1
            elif (char in CLOSERS and char not in OPENERS) or (
                depth == 0 and char in STOP_OPERATORS and not (k == start and char in "+-−")
            ):
                break
        k += 1
    return k


def _is_fence(node: Element) -> bool:
    return _local(node) == "mo" and node.get("fence") == "true" and len(node) == 0


def _form(node: Element) -> str:
    return node.get("form", "")


def _limit_name(base: Element) -> bool:
    return _local(base) == "mo" and (
        len(base) > 0 or (_text(base) in LIMIT_NAMES and _text(base).isalpha())
    )


def _expect(node: Element, count: int) -> list[Element]:
    kids = list(node)
    if len(kids) != count:
        raise UnsupportedMath(f"<{_local(node)}> expects {count} children, has {len(kids)}")
    return kids


def _space_text(width: str) -> str:
    width = width.strip()
    if width in _SPACES:
        return _SPACES[width]
    if width.startswith("negative") or width.startswith("-"):
        return ""
    match = re.fullmatch(r"([0-9.]+)\s*(em|ex)?", width)
    if not match:
        return ""
    em = float(match.group(1)) * (0.5 if match.group(2) == "ex" else 1.0)
    if em < 0.1:
        return ""
    if em <= 0.2:
        return "\u2009"
    if em <= 0.25:
        return "\u2005"
    if em <= 0.3:
        return "\u2004"
    return "\u2003" * max(1, round(em))


def _run_signature(run: Element) -> bytes | None:
    """Runs that print the same way can share one `m:r`. Alignment markers never merge."""
    if _local(run) != "r":
        return None
    parts = []
    for child in run:
        if _local(child) == "t":
            continue
        if _local(child) == "rPr" and child.find(_M + "aln") is not None:
            return None
        parts.append(etree.tostring(child))
    return b"".join(parts)


def _merge_runs(nodes: list[Element]) -> list[Element]:
    merged: list[Element] = []
    for node in nodes:
        signature = _run_signature(node)
        if merged and signature is not None and _run_signature(merged[-1]) == signature:
            previous = merged[-1].find(_M + "t")
            addition = node.find(_M + "t")
            assert previous is not None and addition is not None
            previous.text = (previous.text or "") + (addition.text or "")
            if previous.text != previous.text.strip():
                previous.set(_XML_SPACE, "preserve")
            continue
        merged.append(node)
    return merged


def _to_mathml(latex: str) -> str:
    try:
        return latex2mathml.converter.convert(latex)
    except Exception as exc:  # latex2mathml raises assorted errors on input it cannot parse
        raise UnsupportedMath(f"LaTeX not understood: {type(exc).__name__}") from exc


# --- public API -----------------------------------------------------------------------------


def latex_to_omath(latex: str, *, display: bool = True) -> Element:
    """Convert LaTeX to an `m:oMath` element. Raises `UnsupportedMath` when it cannot."""
    source, _tag = split_tag(latex)
    source = _NOISE.sub("", source).strip()
    if not source:
        raise UnsupportedMath("empty formula")
    converter = _Converter(display)

    whole = _WHOLE_ENV.match(source)
    if whole:
        nodes = _equation_array(converter, whole.group(2))
    else:
        nodes = converter.latex_nodes(_rewrite_inner_envs(source))
    if not nodes:
        raise UnsupportedMath("empty formula")
    math = _el("oMath")
    math.extend(nodes)
    return math


def _equation_array(converter: _Converter, body: str) -> list[Element]:
    rows = _rows_of(body)
    if not rows:
        return []
    row_nodes: list[list[Element]] = []
    aligned = False
    for row in rows:
        nodes: list[Element] = []
        for index, cell in enumerate(row):
            cell_nodes = converter.latex_nodes(cell) if cell else []
            if index % 2 == 1:
                aligned = True
                if (
                    cell_nodes
                    and _local(cell_nodes[0]) == "r"
                    and cell_nodes[0].find(_M + "rPr") is None
                ):
                    cell_nodes[0].insert(0, _el("rPr", _el("aln")))
                else:
                    cell_nodes.insert(0, _run("", aln=True))
            nodes.extend(cell_nodes)
        row_nodes.append(nodes)
    if len(row_nodes) == 1 and not aligned:
        return row_nodes[0]
    array = _el("eqArr", _el("eqArrPr", _val("maxDist", "1")))
    array.extend(_container("e", nodes) for nodes in row_nodes)
    return [array]


def latex_to_omml_xml(latex: str, *, display: bool = True) -> str:
    """The OMML for `latex` as an XML string (an `m:oMath` or, for display, `m:oMathPara`)."""
    math = latex_to_omath(latex, display=display)
    if display:
        math = wrap_display(math)
    return etree.tostring(math, encoding="unicode")


def wrap_display(math: Element, *, align: str = "center") -> Element:
    return _el("oMathPara", _el("oMathParaPr", _val("jc", align)), math)
