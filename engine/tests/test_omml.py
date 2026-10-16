import pytest
from lxml import etree

from fixtures import math_formulas
from opendoc_engine.exporters.omml import (
    M_NS,
    UnsupportedMath,
    latex_to_omath,
    latex_to_omml_xml,
    split_tag,
    wrap_display,
)
from opendoc_engine.exporters.omml_check import check_omml

NS = {"m": M_NS}


def convert(latex: str, *, display: bool = True) -> etree._Element:
    node = latex_to_omath(latex, display=display)
    assert check_omml(node) == []
    return node


def names(node: etree._Element) -> list[str]:
    """Local names of every OMML element under `node`, in document order."""
    return [etree.QName(e).localname for e in node.iter() if etree.QName(e).namespace == M_NS]


def text_of(node: etree._Element) -> str:
    return "".join(t.text or "" for t in node.iterfind(".//m:t", NS))


def val(node: etree._Element, path: str) -> str | None:
    found = node.find(path, NS)
    return None if found is None else found.get(f"{{{M_NS}}}val")


# --- constructs ---------------------------------------------------------------------------


def test_identifiers_numbers_operators_share_runs():
    node = convert("a+12=b")
    runs = node.findall("m:r", NS)
    assert len(runs) == 1  # same-style neighbours are merged into one run
    assert text_of(node) == "a+12=b"


def test_function_names_are_upright():
    node = convert(r"\sin x")
    first = node.find("m:r", NS)
    assert val(first, "m:rPr/m:sty") == "p"
    assert text_of(first) == "sin"


def test_text_is_normal_text_and_keeps_spaces():
    node = convert(r"x \text{ if } y")
    runs = node.findall("m:r", NS)
    nor = [r for r in runs if r.find("m:rPr/m:nor", NS) is not None]
    assert len(nor) == 1
    assert text_of(nor[0]) == "\xa0if\xa0"


def test_fraction():
    node = convert(r"\frac{a}{b}")
    fraction = node.find("m:f", NS)
    assert text_of(fraction.find("m:num", NS)) == "a"
    assert text_of(fraction.find("m:den", NS)) == "b"
    assert fraction.find("m:fPr", NS) is None


def test_binomial_is_a_delimited_fraction_without_bar():
    node = convert(r"\binom{n}{k}")
    delimiter = node.find("m:d", NS)
    assert val(delimiter, "m:dPr/m:begChr") == "("
    assert val(delimiter, "m:e/m:f/m:fPr/m:type") == "noBar"


def test_scripts():
    assert names(convert("x^2")).count("sSup") == 1
    assert names(convert("x_i")).count("sSub") == 1
    sub_sup = convert("x_i^2").find("m:sSubSup", NS)
    assert text_of(sub_sup.find("m:sub", NS)) == "i"
    assert text_of(sub_sup.find("m:sup", NS)) == "2"


def test_empty_base_gets_an_invisible_placeholder():
    node = convert("{}^{2}x")
    base = node.find("m:sSup/m:e", NS)
    assert text_of(base) == "\u200b"


def test_square_root_hides_the_degree():
    node = convert(r"\sqrt{x+1}")
    radical = node.find("m:rad", NS)
    assert val(radical, "m:radPr/m:degHide") == "1"
    assert text_of(radical.find("m:e", NS)) == "x+1"


def test_nth_root_has_a_degree():
    radical = convert(r"\sqrt[3]{x}").find("m:rad", NS)
    assert text_of(radical.find("m:deg", NS)) == "3"
    assert radical.find("m:radPr/m:degHide", NS) is None


def test_left_right_delimiters():
    node = convert(r"\left( \frac{a}{b} \right)")
    delimiter = node.find("m:d", NS)
    assert val(delimiter, "m:dPr/m:begChr") == "("
    assert val(delimiter, "m:dPr/m:endChr") == ")"
    assert delimiter.find("m:e/m:f", NS) is not None


def test_invisible_right_delimiter():
    delimiter = convert(r"\left\{ x \right.").find("m:d", NS)
    assert val(delimiter, "m:dPr/m:begChr") == "{"
    assert val(delimiter, "m:dPr/m:endChr") == ""


def test_angle_brackets():
    delimiter = convert(r"\left\langle x \right\rangle").find("m:d", NS)
    assert val(delimiter, "m:dPr/m:begChr") == "⟨"
    assert val(delimiter, "m:dPr/m:endChr") == "⟩"


def test_mfenced_with_separators():
    from opendoc_engine.exporters import omml

    converter = omml._Converter(display=True)
    nodes = converter.math_root(
        '<math xmlns="http://www.w3.org/1998/Math/MathML">'
        '<mfenced open="[" close="]" separators=";"><mi>a</mi><mi>b</mi></mfenced></math>'
    )
    delimiter = nodes[0]
    assert val(delimiter, "m:dPr/m:begChr") == "["
    assert val(delimiter, "m:dPr/m:sepChr") == ";"
    assert len(delimiter.findall("m:e", NS)) == 2


def test_sum_with_limits_is_nary_with_operand():
    node = convert(r"\sum_{i=1}^{n} a_i + b")
    nary = node.find("m:nary", NS)
    assert val(nary, "m:naryPr/m:chr") == "∑"
    assert val(nary, "m:naryPr/m:limLoc") == "undOvr"  # display style: limits above and below
    assert text_of(nary.find("m:sub", NS)) == "i=1"
    assert text_of(nary.find("m:sup", NS)) == "n"
    assert nary.find("m:e/m:sSub", NS) is not None  # operand stops before the +
    assert text_of(node).endswith("+b")


def test_inline_sum_puts_limits_at_the_side():
    nary = convert(r"\sum_{i} a_i", display=False).find("m:nary", NS)
    assert val(nary, "m:naryPr/m:limLoc") == "subSup"


def test_integral_keeps_limits_at_the_side_and_takes_the_integrand():
    node = convert(r"\int_0^1 f(x)\,dx = 1")
    nary = node.find("m:nary", NS)
    assert val(nary, "m:naryPr/m:chr") == "∫"
    assert val(nary, "m:naryPr/m:limLoc") == "subSup"
    assert "f(x)" in text_of(nary.find("m:e", NS))
    assert text_of(node).endswith("=1")


def test_operand_stops_at_brackets_not_inside_them():
    nary = convert(r"\sum_i (a_i + b_i) + c").find("m:nary", NS)
    assert "+" in text_of(nary.find("m:e", NS))
    assert not text_of(nary.find("m:e", NS)).endswith("c")


def test_bare_operator_hides_both_limits():
    nary = convert(r"\prod x").find("m:nary", NS)
    assert val(nary, "m:naryPr/m:subHide") == "1"
    assert val(nary, "m:naryPr/m:supHide") == "1"


def test_limit_on_a_named_operator():
    node = convert(r"\lim_{x \to 0} f")
    low = node.find("m:limLow", NS)
    assert text_of(low.find("m:e", NS)) == "lim"
    assert text_of(low.find("m:lim", NS)) == "x→0"


@pytest.mark.parametrize(
    ("latex", "mark"),
    [
        (r"\hat{x}", "\u0302"),
        (r"\tilde{x}", "\u0303"),
        (r"\bar{x}", "\u0305"),
        (r"\dot{x}", "\u0307"),
        (r"\ddot{x}", "\u0308"),
        (r"\vec{v}", "\u20d7"),
    ],
)
def test_accents(latex, mark):
    accent = convert(latex).find("m:acc", NS)
    assert val(accent, "m:accPr/m:chr") == mark


def test_overline_and_underline_are_bars():
    over = convert(r"\overline{TM}").find("m:bar", NS)
    assert val(over, "m:barPr/m:pos") == "top"
    assert text_of(over) == "TM"
    under = convert(r"\underline{xy}").find("m:bar", NS)
    assert val(under, "m:barPr/m:pos") == "bot"


def test_braces_are_group_characters():
    group = convert(r"\underbrace{a+b}").find("m:groupChr", NS)
    assert val(group, "m:groupChrPr/m:pos") == "bot"
    assert val(group, "m:groupChrPr/m:chr") == "⏟"


def test_stacked_symbols_use_limit_elements():
    assert names(convert(r"\overset{a}{=}")).count("limUpp") == 1
    assert names(convert(r"\underset{a}{=}")).count("limLow") == 1


def test_matrix_in_brackets():
    delimiter = convert(r"\begin{bmatrix} 1 & 2 \\ 3 & 4 \end{bmatrix}").find("m:d", NS)
    assert val(delimiter, "m:dPr/m:begChr") == "["
    rows = delimiter.findall("m:e/m:m/m:mr", NS)
    assert len(rows) == 2
    assert [text_of(c) for c in rows[1].findall("m:e", NS)] == ["3", "4"]


def test_plain_matrix_and_array_have_no_delimiters():
    node = convert(r"\begin{matrix} a & b \\ c & d \end{matrix}")
    assert node.find("m:m", NS) is not None
    assert node.find("m:d", NS) is None


def test_cases_is_a_left_brace_matrix_with_left_alignment():
    delimiter = convert(r"\begin{cases} a & x>0 \\ b & x<0 \end{cases}").find("m:d", NS)
    assert val(delimiter, "m:dPr/m:begChr") == "{"
    assert val(delimiter, "m:dPr/m:endChr") == ""
    jc = delimiter.findall("m:e/m:m/m:mPr/m:mcs/m:mc/m:mcPr/m:mcJc", NS)
    assert [j.get(f"{{{M_NS}}}val") for j in jc] == ["left", "left"]


def test_aligned_becomes_an_equation_array_with_alignment_points():
    node = convert(r"\begin{aligned} a &= b \\ c &= d \end{aligned}")
    array = node.find("m:eqArr", NS)
    rows = array.findall("m:e", NS)
    assert len(rows) == 2
    for row in rows:
        assert row.find(".//m:rPr/m:aln", NS) is not None
    assert text_of(rows[0]) == "a=b"


def test_aligned_inside_a_delimiter():
    node = convert(r"\left\{ \begin{aligned} a &= 1 \\ b &= 2 \end{aligned} \right.")
    assert node.find("m:d/m:e/m:m", NS) is not None  # inner environments become arrays


def test_spaces_become_space_characters():
    node = convert(r"a \, b \quad c \qquad d")
    assert "\u2009" in text_of(node)
    assert "\u2003" in text_of(node)
    assert "\u2003\u2003" in text_of(node)


def test_negative_space_is_dropped():
    assert text_of(convert(r"a \! b")) == "ab"


def test_tag_is_split_off():
    assert split_tag(r"x = 1 \tag{3}") == ("x = 1 ", "3")
    assert split_tag("x = 1") == ("x = 1", None)
    assert text_of(convert(r"x = 1 \tag{3}")) == "x=1"


def test_display_wrapper():
    wrapped = wrap_display(convert("x"))
    assert etree.QName(wrapped).localname == "oMathPara"
    assert val(wrapped, "m:oMathParaPr/m:jc") == "center"
    assert check_omml(wrapped) == []


def test_xml_string_is_well_formed_and_in_the_math_namespace():
    xml = latex_to_omml_xml(r"\frac{a}{b}")
    root = etree.fromstring(xml)
    assert etree.QName(root).namespace == M_NS
    assert etree.QName(root).localname == "oMathPara"


# --- what falls back ------------------------------------------------------------------------


@pytest.mark.parametrize(
    "latex",
    [
        "",
        r"\not=",
        r"\cancel{x}",
        r"\xrightarrow{a}",
        r"a \\ b",
        "\\frac{a}{",
    ],
)
def test_unsupported_input_raises(latex):
    with pytest.raises(UnsupportedMath):
        latex_to_omath(latex)


# --- the validator itself -----------------------------------------------------------------


def test_validator_catches_a_missing_required_child():
    bad = etree.fromstring(f'<m:oMath xmlns:m="{M_NS}"><m:f><m:num/></m:f></m:oMath>')
    assert any("den" in e for e in check_omml(bad))


def test_validator_catches_wrong_order_and_unknown_elements():
    wrong_order = etree.fromstring(
        f'<m:oMath xmlns:m="{M_NS}"><m:sSup><m:sup/><m:e/></m:sSup></m:oMath>'
    )
    assert check_omml(wrong_order)
    unknown = etree.fromstring(f'<m:oMath xmlns:m="{M_NS}"><m:bogus/></m:oMath>')
    assert check_omml(unknown)
    foreign = etree.fromstring(f'<m:oMath xmlns:m="{M_NS}"><x:y xmlns:x="urn:x"/></m:oMath>')
    assert check_omml(foreign)


def test_validator_checks_values():
    bad = etree.fromstring(
        f'<m:oMath xmlns:m="{M_NS}"><m:nary><m:naryPr><m:limLoc m:val="sideways"/></m:naryPr>'
        "<m:sub/><m:sup/><m:e/></m:nary></m:oMath>"
    )
    assert any("limLoc" in e for e in check_omml(bad))


# --- the spike's real formulas --------------------------------------------------------------

REAL_DISPLAY = math_formulas.PADDLE_DISPLAY + math_formulas.DEEPSEEK_DISPLAY
REAL_INLINE = math_formulas.PADDLE_INLINE + math_formulas.DEEPSEEK_INLINE


@pytest.mark.parametrize("latex", REAL_DISPLAY)
def test_real_display_formulas_convert_to_valid_omml(latex):
    node = convert(latex)
    xml = etree.tostring(node, encoding="unicode")
    assert "ns0:" not in xml
    etree.fromstring(xml.encode())  # well-formed on its own


@pytest.mark.parametrize("latex", REAL_INLINE)
def test_real_inline_formulas_convert_to_valid_omml(latex):
    convert(latex, display=False)


def test_real_aligned_formula_keeps_its_structure():
    latex = math_formulas.PADDLE_DISPLAY[4]
    node = convert(latex)
    array = node.find("m:eqArr", NS)
    assert len(array.findall("m:e", NS)) == 2
    assert len(node.findall(".//m:rad", NS)) == 3
    assert len(node.findall(".//m:bar", NS)) == 2  # \overline{TM}, \overline{MP}
    assert len(node.findall(".//m:d", NS)) == 3  # \left( ... \right) twice and ( y_1 )^2 pieces
