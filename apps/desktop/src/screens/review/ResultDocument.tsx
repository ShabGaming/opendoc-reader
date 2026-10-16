import type { KeyboardEvent, ReactNode } from "react";
import { t } from "../../i18n";
import { useApp } from "../../state/AppContext";
import type { Block, ListItem, Table, TableCell } from "../../types/ir";
import { cx } from "../../util/cx";
import { blockLabel, blockTooltip, isFurniture } from "./blockInfo";
import { InlineText } from "./inline";
import { MathText } from "./math";
import styles from "./ResultDocument.module.css";

interface ResultDocumentProps {
  blocks: Block[];
  /** The block that is hovered or focused, in either pane. */
  activeId: string | null;
  /** The Blocks toggle: labels on page furniture. */
  showBlocks: boolean;
  onActivate(id: string | null): void;
  /** Clicking a block, or pressing Enter on it, brings its box into view on the original. */
  onJump(id: string): void;
}

/** The page rebuilt from the IR: headings, paragraphs, lists, tables, formulas and figures. */
export function ResultDocument({
  blocks,
  activeId,
  showBlocks,
  onActivate,
  onJump,
}: ResultDocumentProps) {
  return (
    <div className={styles.doc}>
      {blocks.map((block) => {
        const onKeyDown = (event: KeyboardEvent) => {
          if (event.target !== event.currentTarget) return;
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            onJump(block.id);
          }
        };
        return (
          <div
            key={block.id}
            data-block-id={block.id}
            data-block-type={block.type}
            data-active={activeId === block.id || undefined}
            className={cx(styles.block, activeId === block.id && styles.active)}
            role="group"
            aria-label={blockLabel(block)}
            title={blockTooltip(block)}
            tabIndex={0}
            onMouseEnter={() => onActivate(block.id)}
            onMouseLeave={() => onActivate(null)}
            onFocus={() => onActivate(block.id)}
            onBlur={() => onActivate(null)}
            onClick={() => onJump(block.id)}
            onKeyDown={onKeyDown}
          >
            <BlockContent block={block} showBlocks={showBlocks} />
          </div>
        );
      })}
    </div>
  );
}

function BlockContent({ block, showBlocks }: { block: Block; showBlocks: boolean }) {
  switch (block.type) {
    case "heading":
      return (
        <div
          role="heading"
          aria-level={Math.min(6, (block.level ?? 1) + 1)}
          className={(block.level ?? 1) <= 1 ? styles.title : styles.subtitle}
        >
          <InlineText text={block.text ?? ""} />
        </div>
      );
    case "list":
      return <ListView items={block.items ?? []} />;
    case "table":
      return block.table ? <TableView table={block.table} /> : null;
    case "formula":
      return (
        <div className={styles.formula}>
          <div className={styles.tex}>
            <MathText latex={block.latex ?? ""} display />
          </div>
          {block.number && <div className={styles.number}>{block.number}</div>}
        </div>
      );
    case "figure":
      return <Figure block={block} />;
    case "caption":
      return (
        <div className={cx(styles.small, styles.centered)}>
          <InlineText text={block.text ?? ""} />
        </div>
      );
    case "footnote":
    case "other":
      return (
        <div className={styles.small}>
          <InlineText text={block.text ?? ""} />
        </div>
      );
    case "header":
    case "footer":
    case "page_number":
      return (
        <div className={cx(styles.small, styles.furniture)}>
          {showBlocks && isFurniture(block) && (
            <span className={styles.furnitureLabel}>{furnitureName(block)}</span>
          )}
          <span>
            <InlineText text={block.text ?? ""} />
          </span>
        </div>
      );
    default:
      return (
        <p className={styles.paragraph}>
          <InlineText text={block.text ?? ""} />
        </p>
      );
  }
}

function furnitureName(block: Block): string {
  if (block.type === "page_number") return t("review.pageNumber");
  if (block.type === "footer") return t("review.blockType.footer");
  return t("review.runningHead");
}

function ListView({ items }: { items: ListItem[] }): ReactNode {
  if (items.length === 0) return null;
  const Tag = items[0]!.ordered ? "ol" : "ul";
  return (
    <Tag className={styles.list}>
      {items.map((item, i) => (
        <li key={i}>
          <InlineText text={item.text} />
          {item.children && item.children.length > 0 && <ListView items={item.children} />}
        </li>
      ))}
    </Tag>
  );
}

function Figure({ block }: { block: Block }) {
  const { platform } = useApp();
  const [x1, y1, x2, y2] = block.bbox;
  const crop = block.figure?.crop;
  if (!crop) {
    return (
      <div
        className={styles.figureEmpty}
        style={{ aspectRatio: `${x2 - x1} / ${y2 - y1}` }}
        role="img"
        aria-label={t("review.figureAlt")}
      />
    );
  }
  return (
    <img className={styles.figure} src={platform.assetUrl(crop)} alt={t("review.figureAlt")} />
  );
}

const NUMERIC = /^[\s$€£(+\-−]*[\d][\d.,\s]*%?\)?$/;

function cellText(cell: TableCell): string {
  return cell.blocks.map((b) => b.text ?? "").join(" ");
}

function TableView({ table }: { table: Table }) {
  const byRow = new Map<number, TableCell[]>();
  for (const cell of table.cells) {
    const row = byRow.get(cell.row) ?? [];
    row.push(cell);
    byRow.set(cell.row, row);
  }
  const rows = Array.from({ length: table.rows }, (_, r) =>
    (byRow.get(r) ?? []).slice().sort((a, b) => a.col - b.col),
  );
  // The leading rows made only of header cells form the table head.
  let headRows = 0;
  while (
    headRows < rows.length &&
    rows[headRows]!.length > 0 &&
    rows[headRows]!.every((c) => c.header)
  ) {
    headRows += 1;
  }

  const renderRow = (cells: TableCell[], r: number) => (
    <tr key={r}>
      {cells.map((cell) => {
        const Tag = cell.header ? "th" : "td";
        const text = cellText(cell);
        return (
          <Tag
            key={`${cell.row}-${cell.col}`}
            rowSpan={cell.rowSpan > 1 ? cell.rowSpan : undefined}
            colSpan={cell.colSpan > 1 ? cell.colSpan : undefined}
            scope={cell.header ? (cell.colSpan > 1 ? "colgroup" : "col") : undefined}
            className={!cell.header && NUMERIC.test(text) ? styles.numeric : undefined}
          >
            <InlineText text={text} />
          </Tag>
        );
      })}
    </tr>
  );

  return (
    <div className={styles.tableScroll}>
      <table className={styles.table}>
        {headRows > 0 && (
          <thead>{rows.slice(0, headRows).map((cells, r) => renderRow(cells, r))}</thead>
        )}
        <tbody>{rows.slice(headRows).map((cells, r) => renderRow(cells, r + headRows))}</tbody>
      </table>
    </div>
  );
}
