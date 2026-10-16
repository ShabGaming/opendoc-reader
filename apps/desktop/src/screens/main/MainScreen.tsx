import { Warning, X } from "@phosphor-icons/react";
import { useState } from "react";
import { ActionBar } from "../../components/ActionBar";
import { Button } from "../../components/Button";
import { DropTarget } from "../../components/DropTarget";
import { Sidebar } from "../../components/Sidebar";
import { statusLabel } from "../../components/StatusGlyph";
import { t } from "../../i18n";
import { useApp } from "../../state/AppContext";
import { selectedItem } from "../../state/selectors";
import type { QueueItem } from "../../types/commands";
import { ReviewScreen } from "../review/ReviewScreen";
import { DetailView } from "./DetailView";
import styles from "./MainScreen.module.css";
import { useFileDrop } from "./useFileDrop";

function statusesOf(queue: QueueItem[]): Map<string, QueueItem["status"]> {
  return new Map(queue.map((item) => [item.fileId, item.status]));
}

/**
 * Says, in a polite live region, when a file changes status. Page-by-page progress is not
 * announced, so a long conversion does not chatter.
 */
function useStatusAnnouncement(queue: QueueItem[]): string {
  const [seen, setSeen] = useState(() => ({
    queue,
    statuses: statusesOf(queue),
    message: "",
  }));

  // Derived while rendering: a change of the queue is compared with the last one seen.
  if (seen.queue !== queue) {
    const statuses = statusesOf(queue);
    let message = seen.message;
    for (const item of queue) {
      const before = seen.statuses.get(item.fileId);
      if (before !== undefined && before !== item.status) {
        message = t("detail.live", { name: item.name, status: statusLabel(item.status) });
      }
    }
    setSeen({ queue, statuses, message });
  }
  return seen.message;
}

export function MainScreen() {
  const { state, dispatch } = useApp();
  const over = useFileDrop(true);
  const announcement = useStatusAnnouncement(state.queue);
  const item = selectedItem(state);
  const reviewing = state.reviewId !== null;

  return (
    <div className={styles.shell}>
      <Sidebar />
      <div className={styles.main}>
        {state.banner && (
          <div className={styles.banner} role="alert">
            <Warning size={16} aria-hidden="true" />
            <span className={styles.bannerText}>{state.banner}</span>
            <Button
              variant="icon"
              aria-label={t("app.dismiss")}
              title={t("app.dismiss")}
              onClick={() => dispatch({ type: "banner", text: null })}
              icon={<X size={16} aria-hidden="true" />}
            />
          </div>
        )}
        {reviewing ? (
          <ReviewScreen key={state.reviewId} fileId={state.reviewId!} />
        ) : (
          <>
            <main className={styles.content}>
              {state.queue.length === 0 ? (
                <DropTarget over={over} />
              ) : item ? (
                <DetailView item={item} />
              ) : (
                <p className={styles.hint}>{t("detail.selectFile")}</p>
              )}
              {over && state.queue.length > 0 && (
                <div className={styles.overlay}>{t("drop.overlay")}</div>
              )}
            </main>
            <ActionBar />
          </>
        )}
      </div>
      <div className="visually-hidden" aria-live="polite" aria-atomic="true">
        {announcement}
      </div>
    </div>
  );
}
