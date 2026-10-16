"""Structural check of the OMML subset `omml.py` emits.

This is not a schema validator. It encodes, for each `m:` element the converter produces, which
children may appear and in what order (the OOXML math schema uses ordered sequences), so tests
and the dev script can catch a malformed tree before Word does. Every `m:` element must be in
the math namespace, and `w:` elements may only be run properties inside an `m:r`.
"""

from __future__ import annotations

from lxml import etree

from opendoc_engine.exporters.omml import M_NS, W_NS

Element = etree._Element

# Elements that hold math content (the EG_OMathElements group, restricted to what we emit).
CONTENT = {"r", "f", "sSup", "sSub", "sSubSup", "rad", "nary", "d", "m", "eqArr", "acc", "bar",
           "groupChr", "limLow", "limUpp"}  # fmt: skip

# tag -> ordered child spec. Each entry is (name, "?" | "1" | "+" | "*"); "content" stands for
# any number of CONTENT elements.
_SEQUENCES: dict[str, list[tuple[str, str]]] = {
    "oMathPara": [("oMathParaPr", "?"), ("oMath", "+")],
    "oMathParaPr": [("jc", "?")],
    "f": [("fPr", "?"), ("num", "1"), ("den", "1")],
    "fPr": [("type", "?")],
    "sSup": [("e", "1"), ("sup", "1")],
    "sSub": [("e", "1"), ("sub", "1")],
    "sSubSup": [("e", "1"), ("sub", "1"), ("sup", "1")],
    "rad": [("radPr", "?"), ("deg", "1"), ("e", "1")],
    "radPr": [("degHide", "?")],
    "nary": [("naryPr", "?"), ("sub", "1"), ("sup", "1"), ("e", "1")],
    "naryPr": [("chr", "?"), ("limLoc", "?"), ("subHide", "?"), ("supHide", "?")],
    "d": [("dPr", "?"), ("e", "+")],
    "dPr": [("begChr", "?"), ("sepChr", "?"), ("endChr", "?")],
    "m": [("mPr", "?"), ("mr", "+")],
    "mPr": [("mcs", "?")],
    "mcs": [("mc", "+")],
    "mc": [("mcPr", "1")],
    "mcPr": [("count", "?"), ("mcJc", "?")],
    "mr": [("e", "+")],
    "eqArr": [("eqArrPr", "?"), ("e", "+")],
    "eqArrPr": [("maxDist", "?")],
    "acc": [("accPr", "?"), ("e", "1")],
    "accPr": [("chr", "?")],
    "bar": [("barPr", "?"), ("e", "1")],
    "barPr": [("pos", "?")],
    "groupChr": [("groupChrPr", "?"), ("e", "1")],
    "groupChrPr": [("chr", "?"), ("pos", "?"), ("vertJc", "?")],
    "limLow": [("e", "1"), ("lim", "1")],
    "limUpp": [("e", "1"), ("lim", "1")],
    "r": [("rPr", "?"), ("w:rPr", "?"), ("t", "+")],
    "rPr": [("nor", "?"), ("sty", "?"), ("aln", "?")],
}

# Elements whose only content is more math (any number of CONTENT elements, possibly none).
_CONTAINERS = {"e", "num", "den", "deg", "sub", "sup", "lim", "oMath"}

# Leaf elements with an `m:val` attribute, and the values they accept (None = any string).
_VALUED: dict[str, set[str] | None] = {
    "jc": {"left", "center", "right", "centerGroup"},
    "type": {"bar", "skw", "lin", "noBar"},
    "degHide": {"0", "1"},
    "chr": None,
    "limLoc": {"undOvr", "subSup"},
    "subHide": {"0", "1"},
    "supHide": {"0", "1"},
    "begChr": None,
    "sepChr": None,
    "endChr": None,
    "count": None,
    "mcJc": {"left", "center", "right"},
    "maxDist": {"0", "1"},
    "pos": {"top", "bot"},
    "vertJc": {"top", "bot"},
    "sty": {"p", "b", "i", "bi"},
}
_EMPTY = {"aln", "nor"}


def check_omml(root: Element) -> list[str]:
    """Return a list of problems found under `root` (empty when it is well formed)."""
    errors: list[str] = []
    _check(root, errors)
    return errors


def _name(node: Element) -> str:
    qname = etree.QName(node)
    if qname.namespace == M_NS:
        return qname.localname
    if qname.namespace == W_NS:
        return f"w:{qname.localname}"
    return f"{{{qname.namespace}}}{qname.localname}"


def _check(node: Element, errors: list[str]) -> None:
    name = _name(node)
    if name.startswith("w:") or name.startswith("{"):
        errors.append(f"unexpected element {name}")
        return
    kids = list(node)

    if name in _EMPTY:
        if kids or node.text:
            errors.append(f"{name} must be empty")
        return
    if name in _VALUED:
        allowed = _VALUED[name]
        value = node.get(f"{{{M_NS}}}val")
        if kids:
            errors.append(f"{name} must not have children")
        if value is None:
            errors.append(f"{name} needs m:val")
        elif allowed is not None and value not in allowed:
            errors.append(f"{name} has invalid m:val {value!r}")
        return
    if name == "t":
        if kids:
            errors.append("m:t must only contain text")
        return
    if name in _CONTAINERS:
        for child in kids:
            child_name = _name(child)
            if child_name not in CONTENT:
                errors.append(f"{name} may not contain {child_name}")
            else:
                _check(child, errors)
        return
    spec = _SEQUENCES.get(name)
    if spec is None:
        errors.append(f"unknown element {name}")
        return
    _check_sequence(name, kids, spec, errors)
    for child in kids:
        if _name(child) == "w:rPr":
            _check_word_run_props(child, errors)
        else:
            _check(child, errors)


def _check_word_run_props(node: Element, errors: list[str]) -> None:
    for child in node:
        if etree.QName(child).namespace != W_NS or etree.QName(child).localname != "rFonts":
            errors.append("only w:rFonts is expected inside w:rPr of a math run")


def _check_sequence(
    name: str, kids: list[Element], spec: list[tuple[str, str]], errors: list[str]
) -> None:
    pos = 0
    seen = False
    for child in kids:
        child_name = _name(child)
        while pos < len(spec) and spec[pos][0] != child_name:
            if spec[pos][1] in ("1", "+") and not seen:
                errors.append(f"{name} is missing required {spec[pos][0]} before {child_name}")
                return
            pos += 1
            seen = False
        if pos == len(spec):
            errors.append(f"{name} may not contain {child_name} here")
            return
        if seen and spec[pos][1] in ("?", "1"):
            errors.append(f"{name} has more than one {child_name}")
            return
        seen = True
    while pos < len(spec):
        if spec[pos][1] in ("1", "+") and not seen:
            errors.append(f"{name} is missing required {spec[pos][0]}")
        pos += 1
        seen = False
