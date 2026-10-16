import { GearSix, Plus } from "@phosphor-icons/react";
import { t } from "../i18n";
import { Button } from "./Button";
import styles from "./Sidebar.module.css";

/** Queue sidebar. The queue is always empty until file handling exists. */
export function Sidebar() {
  return (
    <aside className={styles.sidebar} aria-label={t("queue.label")}>
      <header className={styles.header}>
        <h2 className={styles.title}>{t("queue.title")}</h2>
        <Button variant="tertiary" small disabled>
          {t("queue.clearDone")}
        </Button>
        <Button
          variant="icon"
          aria-label={t("queue.addFiles")}
          title={t("queue.addFiles")}
          icon={<Plus size={16} aria-hidden="true" />}
        />
      </header>
      <div className={styles.list}>
        <p className={styles.empty}>{t("queue.empty")}</p>
      </div>
      <footer className={styles.footer}>
        <button type="button" className={styles.settings}>
          <GearSix size={16} aria-hidden="true" />
          {t("settings.open")}
        </button>
      </footer>
    </aside>
  );
}
