/**
 * Generated from engine/schema/ir.schema.json by scripts/gen-types.mjs. Do not edit by hand.
 * Run `pnpm --filter desktop gen:types` after changing the schema.
 */

/**
 * This interface was referenced by `Document`'s JSON-Schema
 * via the `definition` "BlockType".
 */
export type BlockType =
  | "heading"
  | "paragraph"
  | "list"
  | "table"
  | "formula"
  | "figure"
  | "caption"
  | "header"
  | "footer"
  | "footnote"
  | "page_number"
  | "other";
/**
 * [x1, y1, x2, y2] in page pixels.
 *
 * @minItems 4
 * @maxItems 4
 *
 * This interface was referenced by `Document`'s JSON-Schema
 * via the `definition` "BBox".
 */
export type BBox = [number, number, number, number];

/**
 * OpenDoc Reader document IR, schema version 1. Every engine produces this model and every exporter reads it. JSON keys are camelCase. All coordinates are pixels of the rendered page image (`Page.sourceImage`), origin at the top left, as [x1, y1, x2, y2]. All file paths are absolute paths in the native format of the host OS. The Python reference implementation is engine/src/opendoc_engine/ir.py; docs/protocol.md explains where IR files live and which RPC methods write them.
 */
export interface Document {
  /**
   * IR schema version. A reader rejects any other value.
   */
  schemaVersion: "1";
  source: Source;
  engine: EngineInfo;
  /**
   * Converted pages in page order. A finished document holds one entry per source page.
   */
  pages: Page[];
}
/**
 * The file the document was read from.
 *
 * This interface was referenced by `Document`'s JSON-Schema
 * via the `definition` "Source".
 */
export interface Source {
  /**
   * Absolute path of the original file.
   */
  path: string;
  kind: "pdf" | "image";
  /**
   * Number of pages in the source file (frames, for a multi-page TIFF).
   */
  pageCount: number;
}
/**
 * Which engine and model produced the content. Text taken from a PDF text layer uses id `pdf-text` and model `text-layer`.
 *
 * This interface was referenced by `Document`'s JSON-Schema
 * via the `definition` "EngineInfo".
 */
export interface EngineInfo {
  /**
   * Engine id, as listed by engine.capabilities.
   */
  id: string;
  /**
   * Manifest model id, or `none` for engines that use no model.
   */
  model: string;
  /**
   * Engine package version.
   */
  version: string;
}
/**
 * One converted page. `blocks` are stored in reading order, and for multi-column pages that order is the only column information kept. A table never continues onto the next page in v1; a table split by a page break is two tables.
 *
 * This interface was referenced by `Document`'s JSON-Schema
 * via the `definition` "Page".
 */
export interface Page {
  /**
   * 0-based page index within the source file.
   */
  index: number;
  /**
   * Width in pixels of the page image.
   */
  width: number;
  /**
   * Height in pixels of the page image.
   */
  height: number;
  /**
   * Absolute path of the rendered and preprocessed page image (PNG, 200 DPI for PDFs). All bounding boxes refer to this image.
   */
  sourceImage: string;
  /**
   * Blocks in reading order. `order` of each block equals its array index. Block ids are unique within a document.
   */
  blocks: Block[];
}
/**
 * A layout element on a page. Which optional fields are set depends on `type`: headings use `level` and `text`; paragraphs, captions, headers, footers, footnotes, page numbers and `other` use `text`; lists use `items`; tables use `table`; formulas use `latex` (and `number` when the page prints an equation number); figures use `figure` once the crop is written. `text` is inline Markdown (emphasis, links, inline math as $...$).
 *
 * This interface was referenced by `Document`'s JSON-Schema
 * via the `definition` "Block".
 */
export interface Block {
  /**
   * Unique within the document. Engines use `p{pageIndex}-b{order}`; blocks inside table cells append `-r{row}c{col}`.
   */
  id: string;
  type: BlockType;
  bbox: BBox;
  /**
   * Reading-order index within the page, contiguous from 0. Blocks inside a table cell number from 0 within the cell.
   */
  order: number;
  /**
   * Heading level. Headings only.
   */
  level?: number;
  text?: string;
  /**
   * Optional line-level boxes and text for text blocks. Not used by the Word and Markdown exporters; kept for the searchable PDF export planned for v0.2. Engines that cannot produce line boxes omit the field.
   */
  lines?: Line[];
  /**
   * List entries, with nesting. List blocks only.
   */
  items?: ListItem[];
  table?: Table;
  /**
   * Formula source without math delimiters. Formula blocks only.
   */
  latex?: string;
  /**
   * The equation number printed next to a formula, as printed, for example "(1)". Formula blocks only.
   */
  number?: string;
  figure?: {
    /**
     * Absolute path of the PNG cropped from the page image.
     */
    crop: string;
  };
  caption?: {
    /**
     * Id of the figure or table block this caption describes.
     */
    for: string;
  };
  confidence?: number;
}
/**
 * This interface was referenced by `Document`'s JSON-Schema
 * via the `definition` "Line".
 */
export interface Line {
  bbox: BBox;
  text: string;
}
/**
 * This interface was referenced by `Document`'s JSON-Schema
 * via the `definition` "ListItem".
 */
export interface ListItem {
  /**
   * Inline Markdown.
   */
  text: string;
  ordered: boolean;
  /**
   * Nested items.
   */
  children?: ListItem[];
}
/**
 * This interface was referenced by `Document`'s JSON-Schema
 * via the `definition` "Table".
 */
export interface Table {
  rows: number;
  cols: number;
  /**
   * One entry per anchored cell. A cell covered by another cell's span has no entry.
   */
  cells: TableCell[];
}
/**
 * This interface was referenced by `Document`'s JSON-Schema
 * via the `definition` "TableCell".
 */
export interface TableCell {
  row: number;
  col: number;
  rowSpan: number;
  colSpan: number;
  /**
   * Present and true for header cells.
   */
  header?: true;
  /**
   * Cell content, usually one paragraph. Cell blocks are never tables: nested tables are not supported in v1.
   */
  blocks: Block[];
}
