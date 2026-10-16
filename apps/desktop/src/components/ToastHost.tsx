import { CheckCircle } from "@phosphor-icons/react";
import { useEffect } from "react";
import { useApp } from "../state/AppContext";
import styles from "./ToastHost.module.css";

const LIFETIME_MS = 3200;

function ToastItem({ id, text }: { id: number; text: string }) {
  const { dispatch } = useApp();
  useEffect(() => {
    const timer = setTimeout(() => dispatch({ type: "dismissToast", id }), LIFETIME_MS);
    return () => clearTimeout(timer);
  }, [dispatch, id]);
  return (
    <div className={styles.toast}>
      <CheckCircle className={styles.icon} size={16} aria-hidden="true" />
      <span>{text}</span>
    </div>
  );
}

/** Confirmations only. Errors and decisions never use a toast. */
export function ToastHost() {
  const { state } = useApp();
  return (
    <div className={styles.host} role="status" aria-live="polite">
      {state.toasts.map((toast) => (
        <ToastItem key={toast.id} id={toast.id} text={toast.text} />
      ))}
    </div>
  );
}
