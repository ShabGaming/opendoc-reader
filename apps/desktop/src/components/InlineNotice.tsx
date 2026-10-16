import { Warning, WarningCircle } from "@phosphor-icons/react";
import type { ReactNode } from "react";
import { cx } from "../util/cx";
import styles from "./InlineNotice.module.css";

interface InlineNoticeProps {
  tone?: "warn" | "err";
  /** The status line, in semibold with the warning icon. */
  title: string;
  /** One line that explains it. */
  body?: string;
  /** Secondary buttons. */
  children?: ReactNode;
  /** Announce the notice as it appears, for a failure the user did not ask about. */
  alert?: boolean;
}

/** A notice about one item that needs a decision. It does not time out. */
export function InlineNotice({ tone = "warn", title, body, children, alert }: InlineNoticeProps) {
  const Icon = tone === "err" ? WarningCircle : Warning;
  return (
    <div className={cx(styles.notice, styles[tone])} role={alert ? "alert" : undefined}>
      <div className={styles.title}>
        <Icon size={16} aria-hidden="true" />
        <span>{title}</span>
      </div>
      {body && <div className={styles.body}>{body}</div>}
      {children && <div className={styles.actions}>{children}</div>}
    </div>
  );
}
