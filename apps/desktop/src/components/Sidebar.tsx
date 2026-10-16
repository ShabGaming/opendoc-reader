import { FileImage, FilePdf, GearSix, Plus } from "@phosphor-icons/react";
import { useRef, useState, type KeyboardEvent } from "react";
import { t, tp } from "../i18n";
import { useApp } from "../state/AppContext";
import { cx } from "../util/cx";
import { Button } from "./Button";
import styles from "./Sidebar.module.css";
import { StatusGlyph } from "./StatusGlyph";

/** The queue. One list: Up and Down move between files, Enter opens a finished one. */
export function Sidebar() {
  const { state, dispatch, actions } = useApp();
  const listRef = useRef<HTMLUListElement>(null);
  const [focusId, setFocusId] = useState<string | null>(null);

  const current = state.reviewId ?? state.selectedId;
  const doneCount = state.queue.filter(
    (q) => q.status === "done" || q.status === "cancelled",
  ).length;
  // Exactly one row is in the tab order: the focused one, else the current one, else the first.
  const tabId =
    [focusId, current].find((id) => id && state.queue.some((q) => q.fileId === id)) ??
    state.queue[0]?.fileId;

  const move = (event: KeyboardEvent<HTMLUListElement>) => {
    const keys = ["ArrowDown", "ArrowUp", "Home", "End"];
    if (!keys.includes(event.key)) return;
    const rows = Array.from(listRef.current?.querySelectorAll<HTMLButtonElement>("button") ?? []);
    const at = rows.findIndex((r) => r === document.activeElement);
    if (rows.length === 0 || at < 0) return;
    event.preventDefault();
    const next =
      event.key === "Home"
        ? 0
        : event.key === "End"
          ? rows.length - 1
          : Math.max(0, Math.min(rows.length - 1, at + (event.key === "ArrowDown" ? 1 : -1)));
    rows[next]?.focus();
  };

  return (
    <aside className={styles.sidebar} aria-label={t("queue.label")}>
      <header className={styles.header}>
        <h2 className={styles.title}>{t("queue.title")}</h2>
        <span className={styles.count}>{tp("queue.count", state.queue.length)}</span>
        <span className={styles.spacer} />
        <Button variant="tertiary" small disabled={doneCount === 0} onClick={actions.clearDone}>
          {t("queue.clearDone")}
        </Button>
        <Button
          variant="icon"
          aria-label={t("queue.addFiles")}
          title={t("queue.addFiles")}
          onClick={actions.chooseFiles}
          icon={<Plus size={16} aria-hidden="true" />}
        />
      </header>
      {state.queue.length === 0 ? (
        <div className={styles.list}>
          <p className={styles.empty}>{t("queue.empty")}</p>
        </div>
      ) : (
        <ul className={cx(styles.list, styles.rows)} ref={listRef} onKeyDown={move}>
          {state.queue.map((item) => {
            const Icon = item.kind === "pdf" ? FilePdf : FileImage;
            return (
              <li key={item.fileId}>
                <button
                  type="button"
                  className={styles.row}
                  tabIndex={item.fileId === tabId ? 0 : -1}
                  aria-current={item.fileId === current ? "true" : undefined}
                  onFocus={() => setFocusId(item.fileId)}
                  onClick={() => dispatch({ type: "select", fileId: item.fileId })}
                  onDoubleClick={() => item.status === "done" && actions.openReview(item.fileId)}
                  onKeyDown={(event) => {
                    if (event.key !== "Enter") return;
                    event.preventDefault();
                    if (item.status === "done") actions.openReview(item.fileId);
                    else dispatch({ type: "select", fileId: item.fileId });
                  }}
                >
                  <Icon className={styles.kind} size={16} aria-hidden="true" />
                  <span className={styles.name}>{item.name}</span>
                  <span className={styles.status}>
                    <StatusGlyph item={item} />
                  </span>
                </button>
              </li>
            );
          })}
        </ul>
      )}
      <footer className={styles.footer}>
        <button type="button" className={styles.settings} onClick={() => actions.openSettings()}>
          <GearSix size={16} aria-hidden="true" />
          {t("settings.open")}
        </button>
      </footer>
    </aside>
  );
}
