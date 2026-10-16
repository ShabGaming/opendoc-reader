import {
  ArrowLeft,
  CaretLeft,
  CaretRight,
  MagnifyingGlassMinus,
  MagnifyingGlassPlus,
  Selection,
} from "@phosphor-icons/react";
import { Button } from "../../components/Button";
import { OutputActions } from "../../components/OutputActions";
import { t } from "../../i18n";
import type { OutputsState } from "../../state/useOutputs";
import { cx } from "../../util/cx";
import styles from "./Toolbar.module.css";

export type ReviewMode = "original" | "split" | "result";
export const ZOOM_MIN = 50;
export const ZOOM_MAX = 200;
export const ZOOM_STEP = 25;

interface ToolbarProps {
  fileName: string;
  /** Side by side and Result need a finished file. */
  done: boolean;
  mode: ReviewMode;
  onMode(mode: ReviewMode): void;
  page: number;
  pageCount: number;
  onPage(page: number): void;
  zoom: number;
  onZoom(zoom: number): void;
  showBlocks: boolean;
  onBlocks(show: boolean): void;
  outputs: OutputsState;
  onBack(): void;
  onReveal(path: string): void;
}

const MODES: Array<{
  id: ReviewMode;
  label: "review.original" | "review.split" | "review.result";
}> = [
  { id: "original", label: "review.original" },
  { id: "split", label: "review.split" },
  { id: "result", label: "review.result" },
];

export function Toolbar(props: ToolbarProps) {
  const { mode, page, pageCount, zoom } = props;
  return (
    <div className={styles.bar}>
      <div className={styles.group}>
        <Button
          variant="ghost"
          icon={<ArrowLeft size={16} aria-hidden="true" />}
          onClick={props.onBack}
        >
          {t("review.back")}
        </Button>
        <span className={styles.name} title={props.fileName}>
          {props.fileName}
        </span>
      </div>

      <div className={styles.segmented} role="group" aria-label={t("review.view")}>
        {MODES.map((m) => {
          const locked = m.id !== "original" && !props.done;
          return (
            <button
              key={m.id}
              type="button"
              aria-pressed={mode === m.id}
              disabled={locked}
              title={locked ? t("review.needsDone") : undefined}
              onClick={() => props.onMode(m.id)}
            >
              {t(m.label)}
            </button>
          );
        })}
      </div>

      <div className={styles.group}>
        <Button
          variant="icon"
          aria-label={t("review.previousPage")}
          title={t("review.previousPage")}
          disabled={page <= 0}
          onClick={() => props.onPage(page - 1)}
          icon={<CaretLeft size={16} aria-hidden="true" />}
        />
        <span className={styles.pageLabel}>
          {t("review.pageOf", { page: page + 1, total: pageCount })}
        </span>
        <Button
          variant="icon"
          aria-label={t("review.nextPage")}
          title={t("review.nextPage")}
          disabled={page >= pageCount - 1}
          onClick={() => props.onPage(page + 1)}
          icon={<CaretRight size={16} aria-hidden="true" />}
        />
      </div>

      <div className={styles.group} role="group" aria-label={t("review.zoomLabel")}>
        <Button
          variant="icon"
          aria-label={t("review.zoomOut")}
          title={t("review.zoomOut")}
          disabled={mode === "result" || zoom <= ZOOM_MIN}
          onClick={() => props.onZoom(zoom - ZOOM_STEP)}
          icon={<MagnifyingGlassMinus size={16} aria-hidden="true" />}
        />
        <span className={styles.zoomValue}>{t("review.zoomValue", { zoom })}</span>
        <Button
          variant="icon"
          aria-label={t("review.zoomIn")}
          title={t("review.zoomIn")}
          disabled={mode === "result" || zoom >= ZOOM_MAX}
          onClick={() => props.onZoom(zoom + ZOOM_STEP)}
          icon={<MagnifyingGlassPlus size={16} aria-hidden="true" />}
        />
      </div>

      <Button
        variant="ghost"
        aria-pressed={props.showBlocks}
        icon={<Selection size={16} aria-hidden="true" />}
        onClick={() => props.onBlocks(!props.showBlocks)}
      >
        {t("review.blocks")}
      </Button>

      {props.done && (
        <div className={cx(styles.group, styles.exports)}>
          <OutputActions outputs={props.outputs} onReveal={props.onReveal} />
        </div>
      )}
    </div>
  );
}
