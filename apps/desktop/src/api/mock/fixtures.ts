import type { Block, EngineInfo, Page, TableCell } from "../../types/ir";

// Small hand-written IR pages for the mock bridge and the tests. The page images are SVG data
// URLs drawn from the blocks, so there is no binary fixture in the repository.

export const PAGE_WIDTH = 1240;
export const PAGE_HEIGHT = 1754;
export const FIXTURE_PAGE_COUNT = 3;

export const MOCK_ENGINE: EngineInfo = {
  id: "paddleocr-vl",
  model: "accurate-paddleocr-vl-1.6",
  version: "0.1.0",
};

type Box = [number, number, number, number];

function block(
  page: number,
  order: number,
  type: Block["type"],
  bbox: Box,
  extra: Partial<Block> = {},
): Block {
  return { id: `p${page}-b${order}`, type, bbox, order, ...extra };
}

function cell(
  page: number,
  order: number,
  row: number,
  col: number,
  text: string,
  spans: { rowSpan?: number; colSpan?: number; header?: true } = {},
): TableCell {
  return {
    row,
    col,
    rowSpan: spans.rowSpan ?? 1,
    colSpan: spans.colSpan ?? 1,
    ...(spans.header ? { header: true as const } : {}),
    blocks: [
      {
        id: `p${page}-b${order}-r${row}c${col}`,
        type: "paragraph",
        bbox: [110, 640, 1130, 1010],
        order: 0,
        text,
      },
    ],
  };
}

function proseBlocks(): Block[] {
  const p = 0;
  return [
    block(p, 0, "header", [420, 90, 820, 126], { text: "Town library annual report" }),
    block(p, 1, "page_number", [1090, 90, 1150, 126], { text: "12" }),
    block(p, 2, "heading", [110, 190, 900, 250], { level: 1, text: "The reading room" }),
    block(p, 3, "paragraph", [110, 280, 1130, 520], {
      text:
        "The reading room opened in the spring of 1919 with **two hundred** chairs and a " +
        "collection of about *nine thousand* volumes. Most of the books had been given by " +
        "families in the district, and the board spent its first year sorting and repairing them.",
    }),
    block(p, 4, "paragraph", [110, 540, 1130, 700], {
      text:
        "Attendance was highest on Saturday afternoons. The librarian kept a daily tally, " +
        "which the board reviewed each month together with the list of requests.",
    }),
    block(p, 5, "list", [110, 730, 1130, 930], {
      items: [
        { text: "Fiction and poetry, on the ground floor", ordered: true },
        {
          text: "Reference works",
          ordered: true,
          children: [
            { text: "Atlases and gazetteers", ordered: false },
            { text: "Directories", ordered: false },
          ],
        },
        { text: "Local history, by request", ordered: true },
      ],
    }),
    block(p, 6, "figure", [220, 960, 1020, 1380], {
      figure: { crop: figureCropUrl(0) },
    }),
    block(p, 7, "caption", [330, 1396, 910, 1432], {
      text: "Figure 1. The reading room in 1919.",
      caption: { for: "p0-b6" },
    }),
    block(p, 8, "paragraph", [110, 1470, 1130, 1590], {
      text:
        "A second reading room for children was added in the autumn. It is described in " +
        "the next section.",
    }),
    block(p, 9, "footnote", [110, 1640, 1130, 1690], {
      text: "1. Figures are taken from the minutes of the board.",
    }),
  ];
}

function tableBlocks(): Block[] {
  const p = 1;
  const cells: TableCell[] = [
    cell(p, 4, 0, 0, "Branch", { rowSpan: 2, header: true }),
    cell(p, 4, 0, 1, "Books lent", { colSpan: 2, header: true }),
    cell(p, 4, 0, 3, "Change", { rowSpan: 2, header: true }),
    cell(p, 4, 1, 1, "1918", { header: true }),
    cell(p, 4, 1, 2, "1919", { header: true }),
    cell(p, 4, 2, 0, "Central"),
    cell(p, 4, 2, 1, "12,480"),
    cell(p, 4, 2, 2, "13,102"),
    cell(p, 4, 2, 3, "+5%"),
    cell(p, 4, 3, 0, "Eastside"),
    cell(p, 4, 3, 1, "4,310"),
    cell(p, 4, 3, 2, "4,128"),
    cell(p, 4, 3, 3, "-4%"),
    cell(p, 4, 4, 0, "Total"),
    cell(p, 4, 4, 1, "16,790"),
    cell(p, 4, 4, 2, "17,230"),
    cell(p, 4, 4, 3, "+3%"),
  ];
  return [
    block(p, 0, "header", [420, 90, 820, 126], { text: "Town library annual report" }),
    block(p, 1, "page_number", [1090, 90, 1150, 126], { text: "13" }),
    block(p, 2, "heading", [110, 190, 900, 250], { level: 1, text: "Lending by branch" }),
    block(p, 3, "paragraph", [110, 280, 1130, 600], {
      text:
        "Lending rose at the central branch and fell slightly at Eastside, where the " +
        "building was closed for three weeks in the winter.",
    }),
    block(p, 4, "table", [110, 640, 1130, 1010], { table: { rows: 5, cols: 4, cells } }),
    block(p, 5, "caption", [110, 1026, 1130, 1062], {
      text: "Table 1. Books lent by branch.",
      caption: { for: "p1-b4" },
    }),
    block(p, 6, "paragraph", [110, 1100, 1130, 1300], {
      text: "The totals include books lent to schools, which are counted once per term.",
    }),
  ];
}

function formulaBlocks(): Block[] {
  const p = 2;
  return [
    block(p, 0, "header", [420, 90, 820, 126], { text: "Town library annual report" }),
    block(p, 1, "page_number", [1090, 90, 1150, 126], { text: "14" }),
    block(p, 2, "heading", [110, 190, 900, 250], { level: 1, text: "Rates of use" }),
    block(p, 3, "paragraph", [110, 280, 1130, 480], {
      text:
        "If $n$ readers each borrow $k$ books in a year, the yearly total grows with the " +
        "number of readers. The board used the following relation to plan shelf space.",
    }),
    block(p, 4, "formula", [260, 520, 1130, 640], {
      latex: "\\int_0^1 x^2\\,dx = \\frac{1}{3}",
      number: "(1)",
    }),
    block(p, 5, "paragraph", [110, 670, 1130, 780], {
      text: "For the running total over the first $n$ weeks it used a simple sum.",
    }),
    block(p, 6, "formula", [260, 810, 1130, 940], {
      latex: "\\sum_{k=1}^{n} k = \\frac{n(n+1)}{2}",
      number: "(2)",
    }),
    block(p, 7, "paragraph", [110, 970, 1130, 1100], {
      text: "Both results agree with the daily tally to within one percent.",
    }),
  ];
}

const BUILDERS: Array<() => Block[]> = [proseBlocks, tableBlocks, formulaBlocks];

/** A fresh copy of fixture page `fixtureIndex`, labelled as page `pageIndex` of its file. */
export function fixturePage(fixtureIndex: number, pageIndex = fixtureIndex): Page {
  const i = ((fixtureIndex % FIXTURE_PAGE_COUNT) + FIXTURE_PAGE_COUNT) % FIXTURE_PAGE_COUNT;
  const blocks = BUILDERS[i]!();
  // Ids stay unique per page, as the IR requires.
  const renamed = pageIndex === i ? blocks : blocks.map((b) => renameBlock(b, i, pageIndex));
  return {
    index: pageIndex,
    width: PAGE_WIDTH,
    height: PAGE_HEIGHT,
    sourceImage: pageImageUrl(i),
    blocks: renamed,
  };
}

function renameBlock(b: Block, from: number, to: number): Block {
  const rename = (id: string) => id.replace(`p${from}-`, `p${to}-`);
  const out: Block = { ...b, id: rename(b.id) };
  if (b.caption) out.caption = { for: rename(b.caption.for) };
  if (b.table) {
    out.table = {
      ...b.table,
      cells: b.table.cells.map((c) => ({
        ...c,
        blocks: c.blocks.map((cb) => ({ ...cb, id: rename(cb.id) })),
      })),
    };
  }
  return out;
}

// ---------------------------------------------------------------------------------------------
// Placeholder page images

const imageCache = new Map<number, string>();

/** The placeholder scan for fixture page `fixtureIndex`, as an SVG data URL. */
export function pageImageUrl(fixtureIndex: number): string {
  const i = fixtureIndex % FIXTURE_PAGE_COUNT;
  let url = imageCache.get(i);
  if (!url) {
    url = svgUrl(pageSvg(BUILDERS[i]!(), i));
    imageCache.set(i, url);
  }
  return url;
}

export function figureCropUrl(fixtureIndex: number): string {
  const w = 800;
  const h = 420;
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${w} ${h}" width="${w}" height="${h}">` +
    `<rect width="${w}" height="${h}" fill="#e9e6df"/>` +
    `<rect x="60" y="250" width="680" height="110" fill="#c9c4b8"/>` +
    `<rect x="110" y="120" width="140" height="130" fill="#a9a398"/>` +
    `<rect x="300" y="90" width="180" height="160" fill="#b8b2a5"/>` +
    `<rect x="540" y="140" width="140" height="110" fill="#a9a398"/>` +
    `<circle cx="640" cy="70" r="28" fill="#d8d3c6"/>` +
    `<text x="400" y="395" font-family="sans-serif" font-size="18" text-anchor="middle" fill="#7a766c">Figure ${fixtureIndex + 1}</text>` +
    `</svg>`;
  return svgUrl(svg);
}

function svgUrl(svg: string): string {
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

function pageSvg(blocks: Block[], index: number): string {
  const parts: string[] = [
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${PAGE_WIDTH} ${PAGE_HEIGHT}" width="${PAGE_WIDTH}" height="${PAGE_HEIGHT}">`,
    `<rect width="${PAGE_WIDTH}" height="${PAGE_HEIGHT}" fill="#fbfaf6"/>`,
  ];
  blocks.forEach((b, n) => parts.push(blockSvg(b, index * 31 + n)));
  parts.push(
    `<text x="${PAGE_WIDTH / 2}" y="${PAGE_HEIGHT - 50}" font-family="sans-serif" font-size="20" text-anchor="middle" fill="#a29e93">Sample page ${index + 1}</text>`,
    `</svg>`,
  );
  return parts.join("");
}

function blockSvg(b: Block, seed: number): string {
  const [x1, y1, x2, y2] = b.bbox;
  const w = x2 - x1;
  const h = y2 - y1;
  switch (b.type) {
    case "figure":
      return (
        `<rect x="${x1}" y="${y1}" width="${w}" height="${h}" fill="#e9e6df" stroke="#cfcabd"/>` +
        `<rect x="${x1 + 60}" y="${y1 + 250}" width="${w - 120}" height="110" fill="#c9c4b8"/>` +
        `<rect x="${x1 + 110}" y="${y1 + 120}" width="140" height="130" fill="#a9a398"/>` +
        `<rect x="${x1 + 300}" y="${y1 + 90}" width="180" height="160" fill="#b8b2a5"/>`
      );
    case "table": {
      const rows = b.table?.rows ?? 4;
      const cols = b.table?.cols ?? 3;
      const out = [
        `<rect x="${x1}" y="${y1}" width="${w}" height="${h}" fill="none" stroke="#6b675d" stroke-width="2"/>`,
      ];
      for (let r = 1; r < rows; r++) {
        const y = y1 + (h / rows) * r;
        out.push(`<line x1="${x1}" y1="${y}" x2="${x2}" y2="${y}" stroke="#8a867b"/>`);
      }
      for (let c = 1; c < cols; c++) {
        const x = x1 + (w / cols) * c;
        out.push(`<line x1="${x}" y1="${y1}" x2="${x}" y2="${y2}" stroke="#8a867b"/>`);
      }
      for (let r = 0; r < rows; r++) {
        for (let c = 0; c < cols; c++) {
          const cx = x1 + (w / cols) * c + 24;
          const cy = y1 + (h / rows) * r + h / rows / 2 - 5;
          out.push(
            `<rect x="${cx}" y="${cy}" width="${(w / cols) * 0.4}" height="10" rx="3" fill="#6b675d"/>`,
          );
        }
      }
      return out.join("");
    }
    case "formula": {
      const mid = y1 + h / 2;
      const left = x1 + w * 0.2;
      const right = x1 + w * 0.78;
      return (
        `<rect x="${left}" y="${mid - 6}" width="${right - left}" height="12" rx="5" fill="#59564d"/>` +
        `<rect x="${left + 40}" y="${mid - 36}" width="90" height="10" rx="4" fill="#79756a"/>` +
        `<rect x="${left + 40}" y="${mid + 26}" width="90" height="10" rx="4" fill="#79756a"/>` +
        `<rect x="${x2 - 70}" y="${mid - 7}" width="60" height="14" rx="5" fill="#59564d"/>`
      );
    }
    case "heading":
      return `<rect x="${x1}" y="${y1 + h / 2 - 10}" width="${w * 0.6}" height="20" rx="6" fill="#33312c"/>`;
    case "header":
    case "page_number":
      return `<rect x="${x1}" y="${y1 + h / 2 - 5}" width="${w}" height="10" rx="4" fill="#8a867b"/>`;
    default:
      return textLines(x1, y1, w, h, seed);
  }
}

function textLines(x: number, y: number, w: number, h: number, seed: number): string {
  const lineHeight = 30;
  const count = Math.max(1, Math.floor(h / lineHeight));
  const out: string[] = [];
  for (let i = 0; i < count; i++) {
    const last = i === count - 1 && count > 1;
    const share = last ? 0.35 + ((seed + i) % 4) * 0.12 : 0.9 + ((seed * 7 + i * 3) % 5) * 0.02;
    out.push(
      `<rect x="${x}" y="${y + i * lineHeight + 8}" width="${Math.round(w * share)}" height="11" rx="4" fill="#6b675d"/>`,
    );
  }
  return out.join("");
}
