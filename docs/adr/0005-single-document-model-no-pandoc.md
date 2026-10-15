# 0005. One document model for all exporters, no pandoc

Status: Accepted

Date: 2026-10-15

## Context

The three tiers produce different raw output. PaddleOCR pipelines return structured results, PaddleOCR-VL returns text per element, and DeepSeek-OCR returns text with grounding tags and HTML tables. We plan to export to Markdown, Word, searchable PDF, HTML, TXT, and later XLSX and JSON.

If each engine had its own path to each format, the number of converters would be the number of engines times the number of formats. Review also needs a structure to show the result next to the page.

A common shortcut is to produce Markdown and then convert it with pandoc. PaddleOCR also has a built-in DOCX export.

## Decision

Each engine normalizes its output into one shared document model, the document IR: Document, then Pages (size, source image), then Blocks. Block types include heading, paragraph, list, table, formula, figure, caption, header and footer. Each block has a bounding box in page coordinates and a reading-order index. Text is stored as inline Markdown, table cells carry row and column spans, formulas are LaTeX, and figures are crops from the page image.

All exporters read the IR and nothing else:

- DOCX through python-docx, with LaTeX converted to MathML and then OMML for native Word equations.
- Markdown with an assets folder, using GFM tables where possible and HTML tables when cells are merged.
- Searchable PDF, HTML and TXT from v0.2, and XLSX and JSON from v0.3.

Pandoc is not used. PaddleOCR's DOCX export is not used.

The schema is a draft and will be finalized in v0.1.

## Consequences

Good:

- A new exporter works for every engine, and a new engine works with every exporter. Cost grows by addition, not multiplication.
- Review, golden-file tests and the JSON export all use the same structure.
- Merged table cells, native equations and embedded figures are under our control.
- No roughly 100 MB pandoc binary to ship.

Bad:

- We own the IR design. A poor early choice, such as how to represent cross-page tables or multi-column reading order, is expensive to change after exporters depend on it.
- Each engine needs a normalizer, and the normalizers are where most bugs will be. Parsing DeepSeek-OCR's grounding output and HTML tables correctly is real work.
- We write and maintain the DOCX, Markdown and PDF writers ourselves, including edge cases that pandoc already handles.
- The LaTeX to MathML to OMML path depends on third-party conversion that may fail on unusual formulas. A fallback (for example inserting the LaTeX as text) is needed.

## Alternatives considered

**Markdown first, then pandoc for DOCX and others.** Fast to build. Rejected because pandoc drops raw HTML, and both VLMs emit HTML tables. Its merged-cell support is weak, and it adds about 100 MB to the download. Markdown would also become the lowest common denominator, losing bounding boxes needed for the searchable PDF.

**PaddleOCR's built-in DOCX export.** Rejected because it only covers Paddle engines, so DeepSeek-OCR and future engines would need a second path.

**Per-engine exporters.** Rejected for the multiplication problem described above.
