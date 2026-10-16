import { useEffect, useId, useRef, type KeyboardEvent } from "react";
import { t } from "../i18n";
import { Button } from "./Button";
import styles from "./ConfirmDialog.module.css";

interface ConfirmDialogProps {
  title: string;
  body: string;
  confirmLabel: string;
  onConfirm: () => void;
  onCancel: () => void;
}

const FOCUSABLE = "button:not(:disabled), [href], input:not(:disabled), select:not(:disabled)";

/**
 * A destructive confirmation. Focus starts on Cancel, Tab stays inside, Escape cancels, and
 * focus returns to whatever opened it.
 */
export function ConfirmDialog({
  title,
  body,
  confirmLabel,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  const titleId = useId();
  const dialogRef = useRef<HTMLDivElement>(null);
  const cancelRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    cancelRef.current?.focus();
    return () => opener?.focus();
  }, []);

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      event.stopPropagation();
      onCancel();
      return;
    }
    if (event.key !== "Tab") return;
    const items = Array.from(dialogRef.current?.querySelectorAll<HTMLElement>(FOCUSABLE) ?? []);
    if (items.length === 0) return;
    const first = items[0]!;
    const last = items[items.length - 1]!;
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  };

  return (
    <>
      <div className={styles.scrim} onClick={onCancel} aria-hidden="true" />
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        className={styles.dialog}
        onKeyDown={onKeyDown}
      >
        <h2 id={titleId} className={styles.title}>
          {title}
        </h2>
        <p className={styles.body}>{body}</p>
        <div className={styles.actions}>
          <Button ref={cancelRef} onClick={onCancel}>
            {t("common.cancel")}
          </Button>
          <Button variant="danger" onClick={onConfirm}>
            {confirmLabel}
          </Button>
        </div>
      </div>
    </>
  );
}
