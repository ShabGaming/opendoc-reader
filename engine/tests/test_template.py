import importlib.util
import io
import zipfile
from pathlib import Path

import docx

from opendoc_engine.exporters.docx import template_bytes

SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "build_reference_template.py"


def open_template() -> docx.document.Document:
    return docx.Document(io.BytesIO(template_bytes()))


def test_template_has_the_styles_the_exporter_uses():
    names = {s.name for s in open_template().styles}
    for name in (
        "Normal",
        "Heading 1",
        "Heading 2",
        "Heading 3",
        "Heading 4",
        "Heading 5",
        "Heading 6",
        "Title",
        "Caption",
        "List Paragraph",
        "Footnote Text",
        "Table Text",
        "Equation",
        "Table Grid",
        "Hyperlink",
        "Code",
    ):
        assert name in names


def test_template_is_small_and_has_no_blank_document_leftovers():
    data = template_bytes()
    assert len(data) < 40_000
    parts = zipfile.ZipFile(io.BytesIO(data)).namelist()
    assert not [p for p in parts if "thumbnail" in p or "customXml" in p or "WithEffects" in p]


def test_template_defines_bullet_and_decimal_lists():
    root = open_template().part.numbering_part.element
    ns = {"w": "http://schemas.openxmlformats.org/wordprocessingml/2006/main"}
    formats = {
        a.get(f"{{{ns['w']}}}abstractNumId"): a.xpath("w:lvl[1]/w:numFmt/@w:val", namespaces=ns)[0]
        for a in root.findall("w:abstractNum", ns)
    }
    assert formats == {"1": "bullet", "2": "decimal"}
    assert len(root.findall("w:abstractNum/w:lvl", ns)) == 18  # nine levels each


def test_template_opens_in_word_mode_15_with_plain_fonts():
    package = open_template()
    settings = package.settings.element
    xml = settings.xml
    assert 'w:name="compatibilityMode"' in xml
    assert 'w:val="15"' in xml
    assert package.styles["Normal"].element.xpath("ancestor::w:styles//w:docDefaults//w:rFonts")


def test_template_matches_what_the_build_script_produces():
    spec = importlib.util.spec_from_file_location("build_reference_template", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    rebuilt = docx.Document(io.BytesIO(module.build()))
    committed = open_template()
    assert {s.name for s in rebuilt.styles} == {s.name for s in committed.styles}
    assert rebuilt.styles.element.xml == committed.styles.element.xml
    assert rebuilt.part.numbering_part.element.xml == committed.part.numbering_part.element.xml
