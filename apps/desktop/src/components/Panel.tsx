import type { ReactNode } from "react";
import { cx } from "../util/cx";
import styles from "./Panel.module.css";

/** A bordered group of rows. Used in Setup and Settings, and nowhere else. */
export function Panel({ children, className }: { children: ReactNode; className?: string }) {
  return <div className={cx(styles.panel, className)}>{children}</div>;
}

interface PanelRowProps {
  label?: ReactNode;
  /** Id for the label, so a switch can name itself with it. */
  labelId?: string;
  description?: ReactNode;
  /** The control, shown at the right. */
  children?: ReactNode;
  align?: "center" | "start";
}

export function PanelRow({
  label,
  labelId,
  description,
  children,
  align = "center",
}: PanelRowProps) {
  return (
    <div className={cx(styles.row, align === "start" && styles.start)}>
      <div className={styles.text}>
        {label !== undefined && (
          <div id={labelId} className={styles.label}>
            {label}
          </div>
        )}
        {description !== undefined && <div className={styles.description}>{description}</div>}
      </div>
      {children !== undefined && <div className={styles.control}>{children}</div>}
    </div>
  );
}
