import { UploadSimple } from "@phosphor-icons/react";
import { t } from "../i18n";
import { useApp } from "../state/AppContext";
import { Button } from "./Button";
import styles from "./DropTarget.module.css";

/** Empty-state drop target. It shows when the queue has no files. */
export function DropTarget({ over }: { over: boolean }) {
  const { actions } = useApp();
  return (
    <section className={styles.target} aria-labelledby="drop-title" data-over={over}>
      <UploadSimple className={styles.glyph} size={20} aria-hidden="true" />
      <h2 id="drop-title" className={styles.title}>
        {t("drop.title")}
      </h2>
      <Button onClick={actions.chooseFiles}>{t("drop.choose")}</Button>
      <p className={styles.note}>
        {t("drop.formats")} {t("privacy.sentence")}
      </p>
    </section>
  );
}
