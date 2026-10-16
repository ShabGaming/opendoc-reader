import type { ReactNode } from "react";
import { t } from "../../i18n";
import styles from "./Setup.module.css";

interface StepFrameProps {
  step: 1 | 2 | 3 | 4;
  title: string;
  lede?: string;
  children: ReactNode;
  /** The buttons: Back at the left, the one primary button at the right. */
  footer: ReactNode;
}

/** The right pane of Setup: a scrolling body and a fixed footer. */
export function StepFrame({ step, title, lede, children, footer }: StepFrameProps) {
  return (
    <main className={styles.main}>
      <div className={styles.body}>
        <div className={styles.inner}>
          <div className={styles.stepOf}>{t("setup.stepOf", { n: step })}</div>
          <h1 className={styles.title}>{title}</h1>
          {lede && <p className={styles.lede}>{lede}</p>}
          {children}
        </div>
      </div>
      <div className={styles.footer}>
        <div className={styles.footerInner}>{footer}</div>
      </div>
    </main>
  );
}
