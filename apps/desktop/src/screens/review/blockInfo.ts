import { t, type MessageKey } from "../../i18n";
import type { Block } from "../../types/ir";

/** Header, footer and page number are left out of the Word and Markdown exports. */
export function isFurniture(block: Block): boolean {
  return block.type === "header" || block.type === "footer" || block.type === "page_number";
}

/** "Paragraph", "Table, 5 rows". Used as the accessible name and the tooltip. */
export function blockLabel(block: Block): string {
  const key = `review.blockType.${block.type}` as MessageKey;
  return t(key, { rows: block.table?.rows ?? 0 });
}

export function blockTooltip(block: Block): string {
  const label = blockLabel(block);
  return isFurniture(block) ? `${label}. ${t("review.leftOut")}` : label;
}
