import { File } from "@phosphor-icons/react";
import { useEffect, useRef } from "react";
import { t } from "../../i18n";
import { useApp } from "../../state/AppContext";
import type { PageStatus } from "../../types/commands";
import styles from "./ThumbStrip.module.css";

interface ThumbStripProps {
  pageCount: number;
  pages: PageStatus[] | undefined;
  page: number;
  onPage(page: number): void;
}

/** A horizontal strip of page thumbnails. It scrolls and keeps the current page in view. */
export function ThumbStrip({ pageCount, pages, page, onPage }: ThumbStripProps) {
  const { platform } = useApp();
  const currentRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    currentRef.current?.scrollIntoView?.({ inline: "center", block: "nearest" });
  }, [page]);

  return (
    <div className={styles.strip} role="group" aria-label={t("review.pages")}>
      {Array.from({ length: pageCount }, (_, i) => {
        const image = pages?.[i]?.imagePath;
        const current = i === page;
        return (
          <button
            key={i}
            ref={current ? currentRef : undefined}
            type="button"
            className={styles.thumb}
            aria-label={t("review.pageN", { n: i + 1 })}
            aria-current={current ? "true" : undefined}
            onClick={() => onPage(i)}
          >
            <span className={styles.image}>
              {image ? (
                <img src={platform.assetUrl(image)} alt="" loading="lazy" />
              ) : (
                <File size={14} aria-hidden="true" />
              )}
            </span>
            <span>{i + 1}</span>
          </button>
        );
      })}
    </div>
  );
}
