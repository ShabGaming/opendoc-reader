import { CaretDown } from "@phosphor-icons/react";
import { useEffect, useId, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { Button } from "./Button";
import styles from "./MenuButton.module.css";

export interface MenuItem {
  id: string;
  label: string;
  onSelect(): void;
}

interface MenuButtonProps {
  label: string;
  items: MenuItem[];
  variant?: "primary" | "secondary" | "tertiary" | "ghost";
  icon?: ReactNode;
  disabled?: boolean;
}

/** A button that opens a small menu of actions. Arrow keys move, Enter picks, Escape closes. */
export function MenuButton({
  label,
  items,
  variant = "secondary",
  icon,
  disabled,
}: MenuButtonProps) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLSpanElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const menuId = useId();

  useEffect(() => {
    if (!open) return;
    root.current?.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
    const onPointer = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", onPointer);
    return () => document.removeEventListener("pointerdown", onPointer);
  }, [open]);

  const close = (refocus: boolean) => {
    setOpen(false);
    if (refocus) trigger.current?.focus();
  };

  const onMenuKey = (event: KeyboardEvent<HTMLDivElement>) => {
    const entries = [...event.currentTarget.querySelectorAll<HTMLElement>('[role="menuitem"]')];
    const index = entries.indexOf(document.activeElement as HTMLElement);
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      close(true);
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      entries[(index + 1) % entries.length]?.focus();
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      entries[(index - 1 + entries.length) % entries.length]?.focus();
    } else if (event.key === "Tab") {
      setOpen(false);
    }
  };

  return (
    <span ref={root} className={styles.root}>
      <Button
        ref={trigger}
        variant={variant}
        icon={icon}
        disabled={disabled}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? menuId : undefined}
        onClick={() => setOpen((v) => !v)}
      >
        {label}
        <CaretDown size={12} aria-hidden="true" />
      </Button>
      {open && (
        <div
          id={menuId}
          role="menu"
          aria-label={label}
          className={styles.menu}
          onKeyDown={onMenuKey}
        >
          {items.map((item) => (
            <button
              key={item.id}
              type="button"
              role="menuitem"
              className={styles.item}
              onClick={() => {
                close(true);
                item.onSelect();
              }}
            >
              {item.label}
            </button>
          ))}
        </div>
      )}
    </span>
  );
}
