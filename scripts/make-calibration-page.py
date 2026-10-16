"""Draws the sample page the app times during setup (calibrate.run).

    uv run --no-project --with pillow python scripts/make-calibration-page.py

The page is A4 at 200 DPI with a heading, three paragraphs and a small ruled table. The text is
from Pride and Prejudice (1813, public domain). The font is the one bundled with Pillow, so
nothing here depends on a system font or a font license.
"""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

WIDTH, HEIGHT = 1654, 2339
MARGIN = 170
OUT = (
    Path(__file__).resolve().parent.parent
    / "apps"
    / "desktop"
    / "src-tauri"
    / "resources"
    / "calibration-page.png"
)

HEADING = "Chapter I"
PARAGRAPHS = [
    "It is a truth universally acknowledged, that a single man in possession of a good fortune, "
    "must be in want of a wife. However little known the feelings or views of such a man may be "
    "on his first entering a neighbourhood, this truth is so well fixed in the minds of the "
    "surrounding families, that he is considered the rightful property of some one or other of "
    "their daughters.",
    '"My dear Mr. Bennet," said his lady to him one day, "have you heard that Netherfield Park '
    'is let at last?" Mr. Bennet replied that he had not. "But it is," returned she; "for Mrs. '
    'Long has just been here, and she told me all about it." Mr. Bennet made no answer.',
    '"Do you not want to know who has taken it?" cried his wife impatiently. "You want to tell '
    'me, and I have no objection to hearing it." This was invitation enough.',
]
TABLE = [
    ("Household", "Daughters", "Income"),
    ("Longbourn", "5", "2,000"),
    ("Lucas Lodge", "2", "1,500"),
    ("Netherfield", "0", "5,000"),
]


def wrap(draw, text, font, max_width):
    lines, line = [], ""
    for word in text.split():
        candidate = f"{line} {word}".strip()
        if draw.textlength(candidate, font=font) <= max_width:
            line = candidate
        else:
            lines.append(line)
            line = word
    if line:
        lines.append(line)
    return lines


def main():
    image = Image.new("RGB", (WIDTH, HEIGHT), (250, 249, 245))
    draw = ImageDraw.Draw(image)
    title_font = ImageFont.load_default(size=64)
    body_font = ImageFont.load_default(size=34)
    y = MARGIN
    draw.text((MARGIN, y), HEADING, font=title_font, fill=(20, 20, 20))
    y += 130
    for paragraph in PARAGRAPHS:
        for line in wrap(draw, paragraph, body_font, WIDTH - 2 * MARGIN):
            draw.text((MARGIN, y), line, font=body_font, fill=(25, 25, 25))
            y += 52
        y += 38

    y += 20
    col_widths = [520, 300, 300]
    row_height = 70
    x0 = MARGIN
    for r, row in enumerate(TABLE):
        x = x0
        for c, cell in enumerate(row):
            draw.rectangle(
                [x, y, x + col_widths[c], y + row_height], outline=(40, 40, 40), width=2
            )
            draw.text((x + 18, y + 16), cell, font=body_font, fill=(25, 25, 25))
            x += col_widths[c]
        y += row_height

    OUT.parent.mkdir(parents=True, exist_ok=True)
    image.save(OUT, optimize=True)
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
