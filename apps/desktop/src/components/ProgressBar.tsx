import type { CSSProperties } from "react";
import { cx } from "../util/cx";
import styles from "./ProgressBar.module.css";

interface ProgressBarProps {
  /** 0 to 1. null draws a bar with no known end. */
  value: number | null;
  label: string;
  /** Text for assistive technology, for example "31 of 48". */
  valueText?: string;
}

/** A 4 px bar. The numbers belong next to it in text; the bar is never the only source. */
export function ProgressBar({ value, label, valueText }: ProgressBarProps) {
  const known = value !== null;
  const percent = known ? Math.max(0, Math.min(100, value * 100)) : 0;
  return (
    <div
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={known ? Math.round(percent) : undefined}
      aria-valuetext={valueText}
      className={cx(
        styles.bar,
        known && percent >= 100 && styles.complete,
        !known && styles.unknown,
      )}
      style={{ "--p": `${percent.toFixed(1)}%` } as CSSProperties}
    >
      <i className="transition" />
    </div>
  );
}
