from opendoc_engine.equations import dollar_math, tidy_equations
from opendoc_engine.ir import Block, BlockType

BOX = (0.0, 0.0, 10.0, 10.0)


def formula(latex: str, number: str | None = None) -> Block:
    return Block("b0", BlockType.FORMULA, BOX, 0, latex=latex, number=number)


def paragraph(text: str) -> Block:
    return Block("b0", BlockType.PARAGRAPH, BOX, 0, text=text)


def test_leading_number_moves_to_the_number_field():
    block = formula(r"(2) \qquad y - y_1 = -\frac{dx_1}{dy_1}")
    tidy_equations([block])
    assert block.number == "(2)"
    assert block.latex == r"y - y_1 = -\frac{dx_1}{dy_1}"


def test_trailing_number_moves_to_the_number_field():
    block = formula(r"x^2 + y^2 = r^2 \quad (7)")
    tidy_equations([block])
    assert (block.latex, block.number) == ("x^2 + y^2 = r^2", "(7)")


def test_a_function_argument_at_the_end_is_not_a_number():
    block = formula("y = f(2)")
    tidy_equations([block])
    assert (block.latex, block.number) == ("y = f(2)", None)


def test_a_note_in_parentheses_is_not_a_number():
    block = formula(r"a = b. \quad (By 55, p. 3)")
    tidy_equations([block])
    assert block.number is None


def test_an_existing_number_is_kept():
    block = formula(r"(3) \quad z = 1", number="(9)")
    tidy_equations([block])
    assert block.number == "(9)"


def test_number_and_single_inline_formula_become_a_formula_block():
    block = paragraph(r"(3) \(TM^* = y_1 \frac{dx_1}{dy_1}\)")
    tidy_equations([block])
    assert block.type is BlockType.FORMULA
    assert block.latex == r"TM^* = y_1 \frac{dx_1}{dy_1}"
    assert block.number == "(3)"
    assert block.text is None


def test_text_with_a_formula_and_more_words_stays_a_paragraph():
    block = paragraph(r"(3) where \(x = 1\) and \(y = 2\)")
    tidy_equations([block])
    assert block.type is BlockType.PARAGRAPH


def test_inline_and_display_math_in_text_use_dollars():
    text = r"slope \( m = 1 \) and \[ y = x \] end"
    assert dollar_math(text) == "slope $m = 1$ and $$y = x$$ end"


def test_glued_display_formulas_become_one_aligned_environment():
    block = formula(r"a = b\] \[c = d\] \[e = f")
    tidy_equations([block])
    assert block.latex == r"\begin{aligned} a = b \\ c = d \\ e = f \end{aligned}"
