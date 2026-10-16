import {
  CheckCircle,
  Clock,
  PauseCircle,
  Prohibit,
  Warning,
  WarningCircle,
} from "@phosphor-icons/react";
import { t, type MessageKey } from "../i18n";
import { percentDone } from "../state/selectors";
import type { QueueItem, QueueStatus } from "../types/commands";
import styles from "./StatusGlyph.module.css";

export function statusLabel(status: QueueStatus): string {
  return t(`queue.status.${status}` as MessageKey);
}

/**
 * The compact status shown in the queue sidebar: an icon (or a percentage while converting)
 * with the word as a tooltip and visually hidden text.
 */
export function StatusGlyph({ item }: { item: QueueItem }) {
  const label = statusLabel(item.status);
  let glyph;
  switch (item.status) {
    case "done":
      glyph = <CheckCircle className={styles.ok} size={16} aria-hidden="true" />;
      break;
    case "converting":
      glyph = <span className={styles.accent}>{percentDone(item)}%</span>;
      break;
    case "needsAttention":
      glyph = <Warning className={styles.warn} size={16} aria-hidden="true" />;
      break;
    case "failed":
      glyph = <WarningCircle className={styles.err} size={16} aria-hidden="true" />;
      break;
    case "paused":
      glyph = <PauseCircle className={styles.muted} size={16} aria-hidden="true" />;
      break;
    case "cancelled":
      glyph = <Prohibit className={styles.muted} size={16} aria-hidden="true" />;
      break;
    default:
      glyph = <Clock className={styles.muted} size={16} aria-hidden="true" />;
  }
  return (
    <span className={styles.glyph} title={label}>
      {glyph}
      <span className="visually-hidden"> {label}</span>
    </span>
  );
}
