import { t } from "../../i18n";
import { useApp } from "../../state/AppContext";
import type { Page } from "../../types/ir";
import { cx } from "../../util/cx";
import { isFurniture } from "./blockInfo";
import styles from "./OriginalPane.module.css";

interface OriginalPaneProps {
  page: Page;
  imagePath: string;
  zoom: number;
  showBlocks: boolean;
  activeId: string | null;
  onActivate(id: string | null): void;
  /** Clicking a box brings the matching result block into view. */
  onJump(id: string): void;
}

/** The scanned page with a box over every detected block. Boxes are placed in percent. */
export function OriginalPane({
  page,
  imagePath,
  zoom,
  showBlocks,
  activeId,
  onActivate,
  onJump,
}: OriginalPaneProps) {
  const { platform } = useApp();
  return (
    <div
      className={styles.scan}
      style={{ width: `${zoom}%`, aspectRatio: `${page.width} / ${page.height}` }}
    >
      <img src={platform.assetUrl(imagePath)} alt={t("review.scanAlt", { n: page.index + 1 })} />
      {showBlocks &&
        page.blocks.map((block) => {
          const [x1, y1, x2, y2] = block.bbox;
          return (
            <div
              key={block.id}
              data-overlay-id={block.id}
              data-active={activeId === block.id || undefined}
              role="presentation"
              className={cx(
                styles.overlay,
                isFurniture(block) && styles.furniture,
                activeId === block.id && styles.active,
              )}
              style={{
                left: `${(x1 / page.width) * 100}%`,
                top: `${(y1 / page.height) * 100}%`,
                width: `${((x2 - x1) / page.width) * 100}%`,
                height: `${((y2 - y1) / page.height) * 100}%`,
              }}
              onMouseEnter={() => onActivate(block.id)}
              onMouseLeave={() => onActivate(null)}
              onClick={() => onJump(block.id)}
            />
          );
        })}
    </div>
  );
}
