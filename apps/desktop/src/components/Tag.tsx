import type { ReactNode } from "react";
import { cx } from "../util/cx";
import styles from "./Tag.module.css";

export type TagTone = "neutral" | "accent" | "ok" | "warn" | "err";

interface TagProps {
  tone?: TagTone;
  icon?: ReactNode;
  children: ReactNode;
}

/** An 18 px label. Text always carries the meaning; colour only reinforces it. */
export function Tag({ tone = "neutral", icon, children }: TagProps) {
  return (
    <span className={cx(styles.tag, styles[tone])}>
      {icon}
      {children}
    </span>
  );
}
