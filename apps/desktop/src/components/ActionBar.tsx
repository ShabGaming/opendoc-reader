import { useId } from "react";
import { t } from "../i18n";
import styles from "./ActionBar.module.css";
import { Button } from "./Button";
import { Select } from "./Select";

export type ModelId = "fast" | "accurate" | "max";
export type SaveTo = "original" | "choose";

export interface ActionBarSettings {
  word: boolean;
  markdown: boolean;
  model: ModelId;
  saveTo: SaveTo;
}

interface ActionBarProps {
  settings: ActionBarSettings;
  onChange: (next: ActionBarSettings) => void;
}

export function ActionBar({ settings, onChange }: ActionBarProps) {
  const formatId = useId();
  const modelId = useId();
  const saveId = useId();

  // At least one format stays checked.
  const toggleFormat = (key: "word" | "markdown") => {
    const next = { ...settings, [key]: !settings[key] };
    if (next.word || next.markdown) onChange(next);
  };

  return (
    <div className={styles.bar}>
      <div className={styles.group} role="group" aria-labelledby={formatId}>
        <span id={formatId} className={styles.label}>
          {t("action.format")}
        </span>
        <label className={styles.check}>
          <input type="checkbox" checked={settings.word} onChange={() => toggleFormat("word")} />
          {t("action.formatWord")}
        </label>
        <label className={styles.check}>
          <input
            type="checkbox"
            checked={settings.markdown}
            onChange={() => toggleFormat("markdown")}
          />
          {t("action.formatMarkdown")}
        </label>
      </div>

      <div className={styles.group}>
        <label htmlFor={modelId} className={styles.label}>
          {t("action.model")}
        </label>
        <Select
          id={modelId}
          value={settings.model}
          onChange={(e) => onChange({ ...settings, model: e.target.value as ModelId })}
        >
          <option value="fast">{t("action.modelFast")}</option>
          <option value="accurate">{t("action.modelAccurate")}</option>
          <option value="max">{t("action.modelMax")}</option>
        </Select>
      </div>

      <div className={styles.group}>
        <label htmlFor={saveId} className={styles.label}>
          {t("action.saveTo")}
        </label>
        <Select
          id={saveId}
          value={settings.saveTo}
          onChange={(e) => onChange({ ...settings, saveTo: e.target.value as SaveTo })}
        >
          <option value="original">{t("action.saveNextToOriginal")}</option>
          <option value="choose">{t("action.saveChooseFolder")}</option>
        </Select>
      </div>

      <div className={styles.spacer} />

      <Button variant="primary" disabled>
        {t("action.convert")}
      </Button>
    </div>
  );
}
