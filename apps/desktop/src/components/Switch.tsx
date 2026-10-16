import styles from "./Switch.module.css";

interface SwitchProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  /** The id of the element that names the switch. */
  labelledBy: string;
  disabled?: boolean;
}

/** For a setting that applies the moment it is flipped. */
export function Switch({ checked, onChange, labelledBy, disabled }: SwitchProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-labelledby={labelledBy}
      disabled={disabled}
      className={styles.switch}
      onClick={() => onChange(!checked)}
    />
  );
}
