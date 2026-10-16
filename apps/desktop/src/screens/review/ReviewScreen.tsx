import { useCallback, useEffect, useRef, useState } from "react";
import { t } from "../../i18n";
import { useApp } from "../../state/AppContext";
import { isAppError } from "../../state/errorCopy";
import { modelShortName } from "../../state/selectors";
import type { ReviewPage } from "../../types/commands";
import { cx } from "../../util/cx";
import { OriginalPane } from "./OriginalPane";
import styles from "./ReviewScreen.module.css";
import { ResultDocument } from "./ResultDocument";
import { ThumbStrip } from "./ThumbStrip";
import { Toolbar, ZOOM_MAX, ZOOM_MIN, ZOOM_STEP, type ReviewMode } from "./Toolbar";
import { useSyncedScroll } from "./useSyncedScroll";

type Loaded =
  | { key: string; review: ReviewPage; failed: null }
  | { key: string; review: null; failed: "notFound" | "error" };

function clamp(n: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, n));
}

function find(root: HTMLElement | null, attribute: string, id: string): HTMLElement | null {
  return root?.querySelector<HTMLElement>(`[${attribute}="${id.replace(/"/g, '\\"')}"]`) ?? null;
}

/** Review: the scan and the result side by side, with the controls of layout C. */
export function ReviewScreen({ fileId }: { fileId: string }) {
  const { state, dispatch, api, dev, actions } = useApp();
  const item = state.queue.find((q) => q.fileId === fileId) ?? null;
  const pageStatuses = state.pages[fileId];

  const [page, setPage] = useState(() => Math.max(0, (dev?.page ?? 1) - 1));
  const [mode, setMode] = useState<ReviewMode>(dev?.view ?? "split");
  const [zoom, setZoom] = useState(100);
  const [showBlocks, setShowBlocks] = useState(true);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [loaded, setLoaded] = useState<Loaded | null>(null);

  const origRef = useRef<HTMLDivElement>(null);
  const resRef = useRef<HTMLDivElement>(null);
  const { suspend } = useSyncedScroll(origRef, resRef, mode === "split");

  const pageCount = item?.pageCount ?? loaded?.review?.pageCount ?? pageStatuses?.length ?? 1;
  const current = clamp(page, 0, Math.max(0, pageCount - 1));
  const key = `${fileId}:${current}`;

  useEffect(() => {
    let cancelled = false;
    api
      .getReview(fileId, current)
      .then((review) => {
        if (!cancelled) setLoaded({ key, review, failed: null });
      })
      .catch((error: unknown) => {
        if (cancelled) return;
        const failed = isAppError(error) && error.code === "notFound" ? "notFound" : "error";
        setLoaded({ key, review: null, failed });
      });
    return () => {
      cancelled = true;
    };
  }, [api, fileId, current, key]);

  useEffect(() => {
    let cancelled = false;
    api
      .getPages(fileId)
      .then((pages) => {
        if (!cancelled) dispatch({ type: "pagesLoaded", fileId, pages });
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [api, dispatch, fileId]);

  const ready = loaded?.key === key ? loaded : null;
  const review = ready?.review ?? null;

  const jump = useCallback(
    (pane: HTMLElement | null, target: HTMLElement | null) => {
      if (!pane || !target) return;
      const paneBox = pane.getBoundingClientRect();
      const box = target.getBoundingClientRect();
      suspend();
      pane.scrollTop += box.top - paneBox.top - pane.clientHeight / 2 + box.height / 2;
    },
    [suspend],
  );
  const jumpToResult = useCallback(
    (id: string) => jump(resRef.current, find(resRef.current, "data-block-id", id)),
    [jump],
  );
  const jumpToOriginal = useCallback(
    (id: string) => jump(origRef.current, find(origRef.current, "data-overlay-id", id)),
    [jump],
  );

  const goTo = useCallback(
    (next: number) => {
      setPage(clamp(next, 0, Math.max(0, pageCount - 1)));
      setActiveId(null);
    },
    [pageCount],
  );
  const changeZoom = (next: number) => setZoom(clamp(next, ZOOM_MIN, ZOOM_MAX));

  // Left and Right (or Page Up and Down) turn pages, Escape returns to the queue, and
  // Ctrl or Cmd with plus or minus zooms the original.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const typing =
        target &&
        (["INPUT", "SELECT", "TEXTAREA"].includes(target.tagName) || target.isContentEditable);
      if (typing || event.defaultPrevented) return;
      const modifier = event.ctrlKey || event.metaKey;
      if (modifier && (event.key === "+" || event.key === "=")) {
        event.preventDefault();
        if (mode !== "result") setZoom((z) => clamp(z + ZOOM_STEP, ZOOM_MIN, ZOOM_MAX));
      } else if (modifier && event.key === "-") {
        event.preventDefault();
        if (mode !== "result") setZoom((z) => clamp(z - ZOOM_STEP, ZOOM_MIN, ZOOM_MAX));
      } else if (modifier || event.altKey || event.shiftKey) {
        return;
      } else if (event.key === "ArrowLeft" || event.key === "PageUp") {
        goTo(current - 1);
      } else if (event.key === "ArrowRight" || event.key === "PageDown") {
        goTo(current + 1);
      } else if (event.key === "Escape") {
        actions.closeReview();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [actions, current, goTo, mode]);

  const engineModel = review ? state.models.find((m) => m.id === review.engine.model) : undefined;
  const resultSubtitle = review
    ? review.engine.id === "pdf-text"
      ? t("review.existingText")
      : engineModel
        ? `${modelShortName(engineModel)} (${engineModel.displayName})`
        : review.engine.model
    : "";

  const message =
    ready?.failed === "notFound"
      ? t("review.noResult")
      : ready?.failed === "error"
        ? t("review.loadFailed")
        : !ready
          ? t("review.loading")
          : null;

  return (
    <div className={styles.review}>
      <Toolbar
        fileName={item?.name ?? ""}
        done={item?.status === "done"}
        mode={mode}
        onMode={setMode}
        page={current}
        pageCount={pageCount}
        onPage={goTo}
        zoom={zoom}
        onZoom={changeZoom}
        showBlocks={showBlocks}
        onBlocks={setShowBlocks}
        canReveal={!!item && item.outputs.length > 0}
        onBack={actions.closeReview}
        onReveal={() => item?.outputs[0] && actions.reveal(item.outputs[0].path)}
        onExportWord={() => actions.exportFile(fileId, "docx")}
        onExportMarkdown={() => actions.exportFile(fileId, "markdown")}
      />
      <ThumbStrip pageCount={pageCount} pages={pageStatuses} page={current} onPage={goTo} />
      <div className={styles.split} aria-busy={!ready}>
        {mode !== "result" && (
          <section className={styles.column} aria-label={t("review.originalLabel")}>
            <div className={styles.columnHeader}>{t("review.paneOriginal")}</div>
            <div ref={origRef} className={cx(styles.pane, styles.original)}>
              {review ? (
                <OriginalPane
                  page={review.page}
                  imagePath={review.imagePath}
                  zoom={zoom}
                  showBlocks={showBlocks}
                  activeId={activeId}
                  onActivate={setActiveId}
                  onJump={jumpToResult}
                />
              ) : (
                <p className={styles.message}>{message}</p>
              )}
            </div>
          </section>
        )}
        {mode !== "original" && (
          <section className={styles.column} aria-label={t("review.resultLabel")}>
            <div className={styles.columnHeader}>
              <span>{t("review.paneResult")}</span>
              {resultSubtitle && (
                <>
                  <span className={styles.dot} aria-hidden="true">
                    {"·"}
                  </span>
                  <span>{resultSubtitle}</span>
                </>
              )}
            </div>
            <div ref={resRef} className={cx(styles.pane, styles.result)}>
              {review ? (
                <ResultDocument
                  blocks={review.page.blocks}
                  activeId={activeId}
                  showBlocks={showBlocks}
                  onActivate={setActiveId}
                  onJump={jumpToOriginal}
                />
              ) : (
                <p className={styles.message}>{message}</p>
              )}
            </div>
          </section>
        )}
      </div>
    </div>
  );
}
