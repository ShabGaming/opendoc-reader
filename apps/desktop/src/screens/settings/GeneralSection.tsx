import { useId } from "react";
import { Panel, PanelRow } from "../../components/Panel";
import { Select } from "../../components/Select";
import { Switch } from "../../components/Switch";
import { Tag } from "../../components/Tag";
import { Button } from "../../components/Button";
import { t } from "../../i18n";
import { useApp } from "../../state/AppContext";
import { installedModels, modelShortName, preferredModelId } from "../../state/selectors";
import type { ExportFormat, Settings, Theme } from "../../types/commands";
import styles from "./Settings.module.css";

const FORMATS: Array<{ id: ExportFormat; label: "action.formatWord" | "action.formatMarkdown" }> = [
  { id: "docx", label: "action.formatWord" },
  { id: "markdown", label: "action.formatMarkdown" },
];

const THEMES: Array<{
  id: Theme;
  label:
    "settings.general.themeSystem" | "settings.general.themeLight" | "settings.general.themeDark";
}> = [
  { id: "system", label: "settings.general.themeSystem" },
  { id: "light", label: "settings.general.themeLight" },
  { id: "dark", label: "settings.general.themeDark" },
];

export function GeneralSection({ settings }: { settings: Settings }) {
  const { state, dispatch, actions } = useApp();
  const updatesId = useId();
  const vulkanId = useId();
  const saveName = useId();
  const installed = installedModels(state.models);
  const modelValue = settings.defaultModelId ?? preferredModelId(state) ?? "";
  const folder = settings.saveTo.kind === "folder" ? settings.saveTo.path : null;

  const toggleFormat = (id: ExportFormat) => {
    const has = settings.defaultFormats.includes(id);
    const next = has
      ? settings.defaultFormats.filter((f) => f !== id)
      : [...settings.defaultFormats, id];
    if (next.length === 0) return;
    void actions.updateSettings({ defaultFormats: next });
    dispatch({ type: "choice", patch: { formats: next } });
  };

  const chooseFolder = async () => {
    const picked = await actions.pickFolder(folder ?? undefined);
    if (!picked) return;
    const saveTo = { kind: "folder", path: picked } as const;
    if (await actions.updateSettings({ saveTo })) dispatch({ type: "choice", patch: { saveTo } });
  };

  const chooseOriginal = async () => {
    const saveTo = { kind: "original" } as const;
    if (await actions.updateSettings({ saveTo })) dispatch({ type: "choice", patch: { saveTo } });
  };

  return (
    <>
      <h1 className={styles.title}>{t("settings.section.general")}</h1>
      <Panel>
        <PanelRow
          label={t("settings.general.defaultFormat")}
          description={t("settings.general.defaultFormatDesc")}
        >
          {FORMATS.map((format) => (
            <label key={format.id} className={styles.check}>
              <input
                type="checkbox"
                checked={settings.defaultFormats.includes(format.id)}
                onChange={() => toggleFormat(format.id)}
              />
              {t(format.label)}
            </label>
          ))}
        </PanelRow>
        <PanelRow
          label={<label htmlFor="default-model">{t("settings.general.defaultModel")}</label>}
          description={t("settings.general.defaultModelDesc")}
        >
          <Select
            id="default-model"
            value={modelValue}
            disabled={installed.length === 0}
            onChange={(e) => {
              void actions.updateSettings({ defaultModelId: e.target.value });
              dispatch({ type: "choice", patch: { modelId: e.target.value } });
            }}
          >
            {installed.length === 0 && <option value="">{t("action.noModel")}</option>}
            {installed.map((m) => (
              <option key={m.id} value={m.id}>
                {modelShortName(m)}
              </option>
            ))}
          </Select>
        </PanelRow>
        <PanelRow
          align="start"
          label={t("settings.general.saveLocation")}
          description={t("settings.general.saveLocationDesc")}
        >
          <div className={styles.radios}>
            <label className={styles.radio}>
              <input
                type="radio"
                name={saveName}
                checked={settings.saveTo.kind === "original"}
                onChange={() => void chooseOriginal()}
              />
              {t("settings.general.saveOriginal")}
            </label>
            <label className={styles.radio}>
              <input
                type="radio"
                name={saveName}
                checked={settings.saveTo.kind === "folder"}
                onChange={() => (folder ? undefined : void chooseFolder())}
              />
              {folder ? (
                <span>
                  {t("settings.general.saveFolder")} <span className={styles.mono}>{folder}</span>
                </span>
              ) : (
                t("settings.general.saveFolderNone")
              )}
            </label>
            <div>
              <Button onClick={() => void chooseFolder()}>{t("common.change")}</Button>
            </div>
          </div>
        </PanelRow>
        <PanelRow
          label={<label htmlFor="theme">{t("settings.general.theme")}</label>}
          description={t("settings.general.themeDesc")}
        >
          <Select
            id="theme"
            value={settings.theme}
            onChange={(e) => void actions.updateSettings({ theme: e.target.value as Theme })}
          >
            {THEMES.map((theme) => (
              <option key={theme.id} value={theme.id}>
                {t(theme.label)}
              </option>
            ))}
          </Select>
        </PanelRow>
        <PanelRow
          labelId={updatesId}
          label={t("settings.general.updates")}
          description={t("settings.general.updatesDesc")}
        >
          <Switch
            labelledBy={updatesId}
            checked={settings.checkForUpdates}
            onChange={(checked) => void actions.updateSettings({ checkForUpdates: checked })}
          />
        </PanelRow>
        <PanelRow
          labelId={vulkanId}
          label={
            <span className={styles.labelWithTag}>
              {t("settings.general.vulkan")}
              <Tag tone="warn">{t("tag.experimental")}</Tag>
            </span>
          }
          description={t("settings.general.vulkanDesc")}
        >
          <Switch
            labelledBy={vulkanId}
            checked={settings.experimentalVulkan}
            onChange={(checked) => void actions.updateSettings({ experimentalVulkan: checked })}
          />
        </PanelRow>
      </Panel>
    </>
  );
}
