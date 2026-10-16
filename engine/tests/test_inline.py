from opendoc_engine.exporters.inline import Segment, parse_inline, plain_text, tidy_math


def kinds(text: str) -> list[tuple[str, str]]:
    return [(s.kind, s.text) for s in parse_inline(text)]


def test_plain_text_is_one_segment():
    assert parse_inline("just words") == [Segment("text", "just words")]


def test_emphasis_and_strong():
    segments = parse_inline("a *b* and **c** d")
    assert [(s.text, s.italic, s.bold) for s in segments] == [
        ("a ", False, False),
        ("b", True, False),
        (" and ", False, False),
        ("c", False, True),
        (" d", False, False),
    ]


def test_unmatched_markers_stay_literal():
    assert kinds("2 * 3 = 6") == [("text", "2 * 3 = 6")]
    assert kinds("snake_case_name") == [("text", "snake_case_name")]
    assert kinds("a * b *c") == [("text", "a * b *c")]


def test_inline_math_in_both_spacings():
    assert kinds("x $a^2$ y") == [("text", "x "), ("math", "a^2"), ("text", " y")]
    assert kinds("at $ P_{1}(x_{1}) $, we") == [
        ("text", "at "),
        ("math", "P_{1}(x_{1})"),
        ("text", ", we"),
    ]


def test_bare_symbols_are_math_but_prices_are_not():
    assert kinds("the axis $OX$ here") == [("text", "the axis "), ("math", "OX"), ("text", " here")]
    assert kinds("(= $(= TM)$)")[1] == ("math", "(= TM)")
    assert kinds("costs $5 and $10 total") == [("text", "costs $5 and $10 total")]


def test_unclosed_dollar_is_literal():
    assert kinds("only $ one") == [("text", "only $ one")]


def test_code_and_links():
    assert kinds("use `x*y` now") == [("text", "use "), ("code", "x*y"), ("text", " now")]
    link = parse_inline("see [the site](https://example.com/a) ok")[1]
    assert (link.kind, link.text, link.url) == ("link", "the site", "https://example.com/a")


def test_escapes():
    assert kinds(r"a \* b \$ c") == [("text", "a * b $ c")]


def test_plain_text_for_titles():
    assert plain_text("A **bold** *title* with $x$") == "A bold title with $x$"


def test_tidy_math_removes_padding_only_in_math():
    assert tidy_math("at $ P_1 $ and $5 and $6") == "at $P_1$ and $5 and $6"
    assert tidy_math(r"keep \$ this $ x^2 $") == r"keep \$ this $x^2$"
