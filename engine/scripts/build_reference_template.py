"""Build `exporters/templates/reference.docx`, the style template the DOCX exporter starts from.

Run from `engine/`:  uv run python scripts/build_reference_template.py

The template starts from python-docx's blank document and keeps a small set of styles (Normal,
Heading 1-6, Title, Caption, Footnote Text, Table Grid and a few helpers), sets plain fonts, adds
two multilevel list definitions (bullet and decimal) that the exporter attaches paragraphs to,
and drops the parts a blank document does not need (thumbnail, customXml, stylesWithEffects).
The result is committed; this script only exists so the template can be rebuilt and reviewed.
"""

from __future__ import annotations

import io
import re
import sys
import zipfile
from pathlib import Path

from docx import Document
from docx.enum.style import WD_STYLE_TYPE
from docx.oxml import parse_xml
from docx.oxml.ns import nsdecls, qn
from docx.shared import Pt

TARGET = (
    Path(__file__).resolve().parents[1]
    / "src"
    / "opendoc_engine"
    / "exporters"
    / "templates"
    / "reference.docx"
)

FONT = "Calibri"
KEEP = {
    "Normal", "Default Paragraph Font", "Normal Table", "No List", "Heading 1", "Heading 2",
    "Heading 3", "Heading 4", "Heading 5", "Heading 6", "Title", "Subtitle", "Caption",
    "List Paragraph", "Table Grid", "Strong", "Emphasis", "Quote", "Header", "Footer",
}  # fmt: skip

HEADINGS = {  # level -> (size in half-points, colour, space before in twips)
    1: (40, "1F3864", 360),
    2: (32, "2F5496", 280),
    3: (26, "2F5496", 240),
    4: (24, "404040", 200),
    5: (22, "404040", 200),
    6: (22, "595959", 200),
}

BULLETS = ["•", "–", "▪"]
DECIMAL_FORMATS = ["decimal", "lowerLetter", "lowerRoman"]


def _plain_fonts(rpr, font: str) -> None:
    """Replace theme font references in a run-properties element with one named font."""
    fonts = rpr.find(qn("w:rFonts"))
    if fonts is None:
        fonts = parse_xml(f"<w:rFonts {nsdecls('w')}/>")
        rpr.insert(0, fonts)
    for attr in list(fonts.attrib):
        del fonts.attrib[attr]
    for name in ("ascii", "hAnsi", "cs", "eastAsia"):
        fonts.set(qn(f"w:{name}"), font)


def _prune_styles(doc) -> None:
    root = doc.styles.element
    by_id = {s.get(qn("w:styleId")): s for s in root.findall(qn("w:style"))}
    names = {sid: s.find(qn("w:name")).get(qn("w:val")) for sid, s in by_id.items()}
    keep_names = {n.lower() for n in KEEP}
    keep_ids = {sid for sid, n in names.items() if n.lower() in keep_names}
    queue = list(keep_ids)
    while queue:  # styles that kept styles refer to stay too
        style = by_id[queue.pop()]
        for tag in ("w:basedOn", "w:next", "w:link"):
            ref = style.find(qn(tag))
            if ref is not None and ref.get(qn("w:val")) in by_id:
                target = ref.get(qn("w:val"))
                if target not in keep_ids:
                    keep_ids.add(target)
                    queue.append(target)
    for sid, style in by_id.items():
        if sid not in keep_ids:
            root.remove(style)


def _style_xml(doc, style_id: str):
    for style in doc.styles.element.findall(qn("w:style")):
        if style.get(qn("w:styleId")) == style_id:
            return style
    raise KeyError(style_id)


def _set_style_run(style, *, size=None, bold=None, color=None, italic=None, font=FONT) -> None:
    rpr = style.find(qn("w:rPr"))
    if rpr is None:
        rpr = parse_xml(f"<w:rPr {nsdecls('w')}/>")
        style.append(rpr)
    for tag in ("w:rFonts", "w:b", "w:bCs", "w:i", "w:iCs", "w:color", "w:sz", "w:szCs"):
        for old in rpr.findall(qn(tag)):
            rpr.remove(old)
    w = nsdecls("w")
    pieces = [f'<w:rFonts {w} w:ascii="{font}" w:hAnsi="{font}" w:cs="{font}"/>']
    if bold:
        pieces += [f"<w:b {w}/>", f"<w:bCs {w}/>"]
    if italic:
        pieces += [f"<w:i {w}/>", f"<w:iCs {w}/>"]
    if color:
        pieces.append(f'<w:color {w} w:val="{color}"/>')
    if size:
        pieces += [f'<w:sz {w} w:val="{size}"/>', f'<w:szCs {w} w:val="{size}"/>']
    for piece in pieces:
        rpr.append(parse_xml(piece))


def _style_fonts(doc) -> None:
    defaults = doc.styles.element.find(qn("w:docDefaults"))
    rpr = defaults.find(qn("w:rPrDefault")).find(qn("w:rPr"))
    _plain_fonts(rpr, FONT)
    ppr = defaults.find(qn("w:pPrDefault")).find(qn("w:pPr"))
    spacing = ppr.find(qn("w:spacing"))
    spacing.set(qn("w:after"), "160")
    spacing.set(qn("w:line"), "264")
    spacing.set(qn("w:lineRule"), "auto")

    for level, (size, color, before) in HEADINGS.items():
        style = _style_xml(doc, f"Heading{level}")
        _set_style_run(style, size=size, bold=True, color=color)
        spacing = style.find(qn("w:pPr")).find(qn("w:spacing"))
        spacing.set(qn("w:before"), str(before))
        spacing.set(qn("w:after"), "120")

    _set_style_run(_style_xml(doc, "Title"), size=52, bold=True, color="1F3864")
    _set_style_run(_style_xml(doc, "Subtitle"), size=28, italic=True, color="595959")
    _set_style_run(_style_xml(doc, "Caption"), size=18, italic=True, color="404040")
    for sid in ("Heading1Char", "Heading2Char", "Heading3Char", "TitleChar", "SubtitleChar"):
        try:
            char = _style_xml(doc, sid)
        except KeyError:
            continue
        rpr = char.find(qn("w:rPr"))
        if rpr is not None:
            _plain_fonts(rpr, FONT)


def _add_styles(doc) -> None:
    styles = doc.styles

    def paragraph_style(name: str, base: str = "Normal"):
        style = styles.add_style(name, WD_STYLE_TYPE.PARAGRAPH)
        style.base_style = styles[base]
        style.quick_style = True
        return style

    footnote = paragraph_style("Footnote Text")
    footnote.font.size = Pt(9)
    footnote.paragraph_format.space_after = Pt(2)
    footnote.paragraph_format.line_spacing = 1.0

    table_text = paragraph_style("Table Text")
    table_text.font.size = Pt(10)
    table_text.paragraph_format.space_after = Pt(2)
    table_text.paragraph_format.space_before = Pt(2)
    table_text.paragraph_format.line_spacing = 1.0

    equation = paragraph_style("Equation")
    equation.paragraph_format.space_before = Pt(6)
    equation.paragraph_format.space_after = Pt(6)
    equation.paragraph_format.line_spacing = 1.0

    hyperlink = styles.add_style("Hyperlink", WD_STYLE_TYPE.CHARACTER)
    hyperlink.font.underline = True
    hyperlink.element.get_or_add_rPr().append(
        parse_xml(f'<w:color {nsdecls("w")} w:val="0563C1"/>')
    )

    code = styles.add_style("Code", WD_STYLE_TYPE.CHARACTER)
    code.font.name = "Consolas"
    code.element.get_or_add_rPr().find(qn("w:rFonts")).set(qn("w:hAnsi"), "Consolas")
    code.font.size = Pt(10)


def _numbering_xml() -> str:
    def levels(kind: str) -> str:
        out = []
        for i in range(9):
            left = 720 * (i + 1)
            if kind == "bullet":
                fmt, text = "bullet", BULLETS[i % len(BULLETS)]
                font = f'<w:rFonts w:ascii="{FONT}" w:hAnsi="{FONT}" w:hint="default"/>'
            else:
                fmt, text = DECIMAL_FORMATS[i % 3], f"%{i + 1}."
                font = ""
            out.append(
                f'<w:lvl w:ilvl="{i}"><w:start w:val="1"/><w:numFmt w:val="{fmt}"/>'
                f'<w:lvlText w:val="{text}"/><w:lvlJc w:val="left"/>'
                f'<w:pPr><w:ind w:left="{left}" w:hanging="360"/></w:pPr>'
                f"<w:rPr>{font}</w:rPr></w:lvl>"
            )
        return "".join(out)

    return (
        f"<w:numbering {nsdecls('w')}>"
        f'<w:abstractNum w:abstractNumId="1"><w:multiLevelType w:val="hybridMultilevel"/>'
        f"{levels('bullet')}</w:abstractNum>"
        f'<w:abstractNum w:abstractNumId="2"><w:multiLevelType w:val="hybridMultilevel"/>'
        f"{levels('decimal')}</w:abstractNum>"
        '<w:num w:numId="1"><w:abstractNumId w:val="1"/></w:num>'
        '<w:num w:numId="2"><w:abstractNumId w:val="2"/></w:num>'
        "</w:numbering>"
    )


def _numbering(doc) -> None:
    root = doc.part.numbering_part.element
    for child in list(root):
        root.remove(child)
    for child in parse_xml(_numbering_xml()):
        root.append(child)


def _settings(doc) -> None:
    root = doc.settings.element
    for setting in root.iter(qn("w:compatSetting")):
        if setting.get(qn("w:name")) == "compatibilityMode":
            setting.set(qn("w:val"), "15")
    for stale in (qn("w:rsids"), qn("w:proofState"), qn("w:savePreviewPicture")):
        for node in root.findall(stale):
            root.remove(node)


def _strip_parts(raw: bytes) -> bytes:
    """Drop the thumbnail, customXml and stylesWithEffects parts and their references."""
    src = zipfile.ZipFile(io.BytesIO(raw))
    dropped = {
        n
        for n in src.namelist()
        if n.startswith("customXml/") or n.endswith("stylesWithEffects.xml") or "thumbnail" in n
    }
    out = io.BytesIO()
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as dst:
        for name in src.namelist():
            if name in dropped:
                continue
            data = src.read(name)
            if name in ("_rels/.rels", "word/_rels/document.xml.rels"):
                text = data.decode("utf-8")
                text = re.sub(
                    r'<Relationship [^>]*Target="[^"]*(?:thumbnail|customXml|stylesWithEffects)'
                    r'[^"]*"[^>]*/>',
                    "",
                    text,
                )
                data = text.encode("utf-8")
            elif name == "[Content_Types].xml":
                text = data.decode("utf-8")
                text = re.sub(
                    r'<Override PartName="/(?:customXml|word/stylesWithEffects)[^"]*"[^>]*/>',
                    "",
                    text,
                )
                text = text.replace('<Default Extension="jpeg" ContentType="image/jpeg"/>', "")
                data = text.encode("utf-8")
            elif name == "docProps/app.xml":
                data = (
                    b'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
                    b'<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/'
                    b'extended-properties"><Application>OpenDoc Reader</Application></Properties>'
                )
            dst.writestr(name, data)
    return out.getvalue()


def build() -> bytes:
    doc = Document()
    _prune_styles(doc)
    _style_fonts(doc)
    _add_styles(doc)
    _numbering(doc)
    _settings(doc)
    core = doc.core_properties
    core.author = "OpenDoc Reader"
    core.last_modified_by = ""
    core.comments = ""
    core.title = ""
    buffer = io.BytesIO()
    doc.save(buffer)
    return _strip_parts(buffer.getvalue())


def main() -> int:
    TARGET.parent.mkdir(parents=True, exist_ok=True)
    TARGET.write_bytes(build())
    print(f"wrote {TARGET} ({TARGET.stat().st_size} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
