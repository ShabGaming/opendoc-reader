# 0006. Structural fidelity for Markdown and Word, searchable PDF for visual fidelity

Status: Accepted

Date: 2026-10-15

## Context

Users want a conversion that is "1:1" with the scan. That phrase can mean two things. One is that the document has the same structure: headings, paragraphs, lists, tables, formulas, figures and reading order. The other is that every page looks the same, with identical fonts and element positions.

The goal of the Markdown and Word exports is an editable document. Recognition produces a structure and bounding boxes, not font data. Typefaces in a scan are not recoverable with any reliability, and exact placement cannot be combined with free editing in Word.

Some users also need a copy that looks exactly like the original but can be searched and selected.

## Decision

For Markdown and Word, "1:1" means structural fidelity. The output preserves headings, paragraphs, reading order, lists, tables including merged cells, formulas, figures cropped from the scan and embedded, and page breaks. Fonts and exact positions are approximated so the output stays editable.

Exact visual fidelity is provided by a separate searchable PDF export: the original page image with an invisible text layer placed at block or line bounding boxes. It is planned for v0.2.

Word export does not use positioned text boxes to imitate the page layout. The UI and README should describe "1:1" as structural fidelity wherever the term appears.

## Consequences

Good:

- Word and Markdown output can be edited in the normal way: text reflows, tables can be changed, equations are native.
- The scope is achievable. Reproducing fonts and spacing is not.
- Users who need the original appearance have an option that matches it exactly.
- Both the DOCX and PDF exporters read the same IR, so the work is shared (ADR 0005).

Bad:

- Word documents will not look identical to the scan. Users who expect that may be disappointed, and the docs must say so before they try.
- Multi-column layouts, sidebars and text wrapped around figures are flattened into a linear reading order, and some layouts will convert awkwardly.
- The searchable PDF depends on accurate bounding boxes. Block-level boxes give coarser text selection than line-level boxes, so we may need to store line boxes in the IR.
- Searchable PDF does not arrive until v0.2, so v0.1 has no exact-fidelity option.

## Alternatives considered

**Positioned text boxes in Word.** Produces a document that looks like the scan. Rejected because the result is nearly impossible to edit: text boxes do not reflow, adding a line breaks the layout, and tables become loose boxes. It would defeat the purpose of converting to Word.

**Attempt font matching.** Rejected because recognition models do not output typeface information, and guessing would produce inconsistent results that would still not match.

**PDF only.** Rejected because the main use case is editable text. A searchable PDF is complementary and is not a replacement.
