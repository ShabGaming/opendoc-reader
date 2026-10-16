import { CaretDown } from "@phosphor-icons/react";
import type { SelectHTMLAttributes } from "react";
import styles from "./Select.module.css";

/** Native select with the system arrow replaced by a caret. */
export function Select({ children, className, ...rest }: SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <span className={styles.wrap}>
      <select className={[styles.select, className ?? ""].filter(Boolean).join(" ")} {...rest}>
        {children}
      </select>
      <CaretDown className={styles.caret} size={12} aria-hidden="true" />
    </span>
  );
}
