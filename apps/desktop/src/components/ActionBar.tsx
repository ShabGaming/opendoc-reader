import { Pause, Play, X } from "@phosphor-icons/react";
import { useId } from "react";
import { t, tp } from "../i18n";
import { useApp } from "../state/AppContext";
import {
  convertibleItems,
  effectiveModelId,
  hasStatus,
  installedModels,
  modelShortName,
} from "../state/selectors";
import type { ExportFormat } from "../types/commands";
import { Button } from "./Button";
import styles from "./ActionBar.module.css";
import { Select } from "./Select";

const FORMATS: Array<{ id: ExportFormat; label: "action.formatWord" | "action.formatMarkdown" }> = [
  { id: "docx", label: "action.formatWord" },
  { id: "markdown", label: "action.formatMarkdown" },
];

/** Format, model and save location for the next batch, plus the queue controls. */
export function ActionBar() {
  const { state, dispatch, actions } = useApp();
  const formatId = useId();
  const modelId = useId();
  const saveId = useId();

  const { choice, models, queue } = state;
  const installed = installedModels(models);
  const current = effectiveModelId(state);
  const count = convertibleItems(queue).length;
  const running = hasStatus(queue, "converting", "paused");
  const paused = hasStatus(queue, "paused");
  const noModel = installed.length === 0;
  const saveFolder = choice.saveTo.kind === "folder" ? choice.saveTo.path : null;

  // At least one format stays checked.
  const toggleFormat = (id: ExportFormat) => {
    const has = choice.formats.includes(id);
    const next = has ? choice.formats.filter((f) => f !== id) : [...choice.formats, id];
    if (next.length > 0) dispatch({ type: "choice", patch: { formats: next } });
  };

  const modelOptions = models.filter((m) => !m.experimental || m.state === "installed");

  return (
    <div className={styles.bar}>
      <div className={styles.group} role="group" aria-labelledby={formatId}>
        <span id={formatId} className={styles.label}>
          {t("action.format")}
        </span>
        {FORMATS.map((format) => (
          <label key={format.id} className={styles.check}>
            <input
              type="checkbox"
              checked={choice.formats.includes(format.id)}
              onChange={() => toggleFormat(format.id)}
            />
            {t(format.label)}
          </label>
        ))}
      </div>

      <div className={styles.group}>
        <label htmlFor={modelId} className={styles.label}>
          {t("action.model")}
        </label>
        <Select
          id={modelId}
          className={styles.compact}
          value={current ?? ""}
          disabled={noModel}
          onChange={(e) => dispatch({ type: "choice", patch: { modelId: e.target.value } })}
        >
          {noModel && <option value="">{t("action.noModel")}</option>}
          {modelOptions.map((m) => (
            <option key={m.id} value={m.id} disabled={m.state !== "installed"}>
              {m.state === "installed"
                ? modelShortName(m)
                : t("action.modelNotInstalled", { name: modelShortName(m) })}
            </option>
          ))}
        </Select>
      </div>

      <div className={styles.group}>
        <label htmlFor={saveId} className={styles.label}>
          {t("action.saveTo")}
        </label>
        <Select
          id={saveId}
          className={styles.compact}
          value={saveFolder ? "folder" : "original"}
          onChange={(e) => {
            if (e.target.value === "choose") void actions.chooseSaveFolder();
            else if (e.target.value === "original") {
              dispatch({ type: "choice", patch: { saveTo: { kind: "original" } } });
            }
          }}
        >
          <option value="original">{t("action.saveNextToOriginal")}</option>
          {saveFolder && <option value="folder">{saveFolder}</option>}
          <option value="choose">{t("action.saveChooseFolder")}</option>
        </Select>
      </div>

      <div className={styles.spacer} />

      {running && (
        <>
          <Button
            icon={
              paused ? (
                <Play size={16} aria-hidden="true" />
              ) : (
                <Pause size={16} aria-hidden="true" />
              )
            }
            onClick={paused ? actions.resumeAll : actions.pauseAll}
          >
            {paused ? t("action.resumeAll") : t("action.pauseAll")}
          </Button>
          <Button icon={<X size={16} aria-hidden="true" />} onClick={actions.cancelAll}>
            {t("action.cancelAll")}
          </Button>
        </>
      )}

      <Button
        variant="primary"
        disabled={count === 0 || noModel}
        title={noModel ? t("action.convertNoModel") : undefined}
        onClick={actions.startConversion}
      >
        {count > 0 ? tp("action.convertCount", count) : t("action.convert")}
      </Button>
    </div>
  );
}
