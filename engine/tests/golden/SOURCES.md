# Golden test pages

Seven pages that are in the public domain, with the Fast tier's output checked in next to them. `engine/tests/test_golden.py` converts the images in `images/` and compares the result with `expected/`. Every file here may be redistributed with the project.

## Sources

| Image | What it covers | Source | License |
| --- | --- | --- | --- |
| `tom-sawyer-book.jpg` | Book page with an illustration, a caption and a running head | Internet Archive item `adventuresoftoms00twaiiala` (Mark Twain, *The Adventures of Tom Sawyer*, American Publishing Co., Hartford, 1876), page image n100, printed page 91. https://archive.org/download/adventuresoftoms00twaiiala/page/n100.jpg | Public domain. Published in 1876; the Internet Archive marks the item NOT_IN_COPYRIGHT. |
| `statabstract-table.jpg` | Ruled table with merged headers and dot leaders | Internet Archive item `statisticalabstr1920unse` (*Statistical Abstract of the United States 1920*, U.S. Government Printing Office), page image n120, printed page 95, Table 63. https://archive.org/download/statisticalabstr1920unse/page/n120.jpg | Public domain. A 1920 publication of the U.S. federal government. |
| `granville-math.jpg` | Display equations, fractions and radicals | Internet Archive item `elementsofdiffer00granuoft` (Granville and Smith, *Elements of the Differential and Integral Calculus*, Ginn and Company, 1911), page image n100, printed page 77. https://archive.org/download/elementsofdiffer00granuoft/page/n100.jpg | Public domain. Published in 1911; the Internet Archive marks the item NOT_IN_COPYRIGHT. |
| `webster-two-column.jpg` | Two-column dictionary page with small type and two small engravings | Internet Archive item `websterspractica00webs` (*Webster's Practical Dictionary*, Reilly & Britton, Chicago, and G. & C. Merriam, Springfield, 1910), page image n100, printed page 91 (CUDDLE to CURE). https://archive.org/download/websterspractica00webs/page/n100.jpg | Public domain. Published in 1910; the Internet Archive records "The Library of Congress is unaware of any copyright restrictions for this item." |
| `cookbook-lists.jpg` | Recipe page: headings, ingredient lists in two columns, instructions | Internet Archive item `bostoncookschool00farmrich` (Fannie Merritt Farmer, *The Boston Cooking-School Cook Book*, Little, Brown, Boston, 1917), page image n100, printed page 65. https://archive.org/download/bostoncookschool00farmrich/page/n100.jpg | Public domain. Published in 1917; the Internet Archive marks the item NOT_IN_COPYRIGHT. |
| `irs-w9-form.jpg` | U.S. federal form: boxes, numbered certification items, headings | IRS Form W-9, *Request for Taxpayer Identification Number and Certification*, PDF page 1 of 6, fetched 2026-10-15. https://www.irs.gov/pub/irs-pdf/fw9.pdf | Public domain. A work of the U.S. federal government. A born-digital render, not a scan. |
| `irs-1040v-voucher.jpg` | Federal payment voucher: two text columns, bullets, a tear-off form section | IRS Form 1040-V (2025), *Payment Voucher for Individuals*, PDF page 1 of 2, fetched 2026-10-16. https://www.irs.gov/pub/irs-pdf/f1040v.pdf | Public domain. A work of the U.S. federal government. A born-digital render, not a scan. |

SHA-256 of the originals the images were made from:

- `adventuresoftoms00twaiiala` n100.jpg: `a5dc159b6165a70df31d80731b5887791a3c92bda94ea31cf6a7ac34a51ee7ec`
- `statisticalabstr1920unse` n120.jpg: `8647193d0686e3d58a6169c260b1aa18e002cd7e947853a583dc0c6aca513b6e`
- `elementsofdiffer00granuoft` n100.jpg: `5e757bf2f6d43228bfada2145d20fd5fb414d0e0ca0665321b861dbb2cb1b4b2`
- `websterspractica00webs` n100.jpg: `34d33f35a2e4596ea12c97d5fd0f0e5c60813e2c8336bbda5ededd7279338636`
- `bostoncookschool00farmrich` n100.jpg: `74fb0c1fbd97b1e091bf92433e25a15ba50f7514b0454f8edae600a30c9a9b30`
- `fw9.pdf`: `2d420cbb4123dcf1fb82595b2359cfbb5d81f00b9df9d359fcc7af361d093f53`
- `f1040v.pdf`: `2426bb330ac54b765b32fbc3eceb878544aa333a3a823e1170aa8261a40fd251`

The Internet Archive images are the full-size JPEGs from its page-image endpoint. The two PDFs were rendered with pdfium at 200 DPI. Every image was then converted to grayscale, scaled so that its long side is 1700 to 1900 pixels, and saved as a JPEG (quality 55 to 62) to stay under 400 KB. The checked-in JPEGs are what the tests read, so the expected output belongs to them and not to the originals.

## Expected output

`expected/<name>.ir.json` is the page IR that `convert.page` wrote for the image, with the file paths in `sourceImage` and `figure.crop` cut down to file names. `expected/<name>.md` is the Markdown exporter's output for that page, with figure links relative to `<name>_assets/`. Both come from the Fast tier (`paddleocr-structure`, model `fast-ppocrv6-tiny`) exactly as the engine produced them. Nothing was edited by hand.

To regenerate them after an intended change to the engine or the images, with the Fast tier models in `OPENDOC_MODELS_DIR`:

```bash
cd engine
OPENDOC_GOLDEN=1 OPENDOC_GOLDEN_UPDATE=1 uv run pytest tests/test_golden.py
```

Then read every changed file and check that it still makes sense before committing it.

## What the checked-in output looks like

The Fast tier is the simplest tier, and the files record what it does today, including its mistakes. Having read each file, this is what to expect:

- `tom-sawyer-book`: the three paragraphs, the illustration (as a figure) and its caption are found and in order. The running head and the page number are classed as header and page number and left out of the Markdown. A few words are misread ("fell fat" for "fell flat", "They'1l").
- `statabstract-table`: the table is kept as one figure, as the Fast tier does by design. The title and the source note become two captions.
- `granville-math`: the display equations are cropped as figures and the equation numbers stay as separate text. Inline math is plain text and partly garbled, and one caption line near a diagram is nonsense.
- `webster-two-column`: the two columns come out as two blocks in the right order, but each column is a single paragraph, so the entries are not separated. The two small engravings are not detected. The small type produces many misreadings.
- `cookbook-lists`: the headings and the instruction paragraphs are good. Each ingredient list becomes one paragraph with its two columns read across (interleaved), no list block is produced, and several fractions are misread.
- `irs-w9-form`: headings and paragraphs are good. Form boxes become figures. The word "Date" is wrongly taken as a heading, and the title has a typo ("Identiffication").
- `irs-1040v-voucher`: headings and paragraphs are good and the columns are in order. Bullets stay as paragraphs that start with a bullet character. The voucher at the bottom is one figure.

## Tolerances

The tests compare structure, not bytes, because Paddle's numbers differ a little between operating systems. A page matches when:

- its size is the same;
- its sequence of block types is at least 85 percent similar;
- its words are at least 90 percent similar;
- at least 90 percent of the blocks that line up in the type sequence have a box with an intersection over union of 0.85 or more;
- the Markdown has at least 90 percent of the same words and the same number of images, give or take one.

The constants are at the top of `test_golden.py`.
