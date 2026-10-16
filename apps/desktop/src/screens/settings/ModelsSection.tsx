import { useEffect, useRef, useState } from "react";
import { Button } from "../../components/Button";
import { InlineNotice } from "../../components/InlineNotice";
import { ConfirmDialog } from "../../components/ConfirmDialog";
import { Panel, PanelRow } from "../../components/Panel";
import { ProgressBar } from "../../components/ProgressBar";
import { Select } from "../../components/Select";
import { Tag } from "../../components/Tag";
import { t } from "../../i18n";
import { useApp } from "../../state/AppContext";
import { setupErrorText } from "../../state/errorCopy";
import { modelShortName } from "../../state/selectors";
import type { ModelInfo, Recommendation } from "../../types/commands";
import { driveOf, formatBytes } from "../../util/format";
import styles from "./Settings.module.css";

/** Toasts "X is installed" when a model's state goes from downloading to installed. */
function useInstalledToast(models: ModelInfo[], toast: (text: string) => void) {
  const previous = useRef<Map<string, ModelInfo["state"]>>(new Map());
  useEffect(() => {
    for (const model of models) {
      const before = previous.current.get(model.id);
      if (before === "downloading" && model.state === "installed") {
        toast(t("toast.installed", { name: modelShortName(model) }));
      }
    }
    previous.current = new Map(models.map((m) => [m.id, m.state]));
  }, [models, toast]);
}

function ModelAction({
  model,
  wontFit,
  onRemove,
}: {
  model: ModelInfo;
  wontFit: boolean;
  onRemove(model: ModelInfo): void;
}) {
  const { state, actions } = useApp();
  const name = modelShortName(model);
  if (model.state === "installed") {
    return (
      <Button
        variant="danger"
        small
        aria-label={t("settings.models.removeLabel", { name })}
        onClick={() => onRemove(model)}
      >
        {t("common.remove")}
      </Button>
    );
  }
  if (model.state === "downloading") {
    const component = state.setup?.components.find((c) => c.id === model.id);
    const value =
      component && component.bytesTotal > 0 ? component.bytesDone / component.bytesTotal : null;
    return (
      <span className={styles.installing}>
        <span className={styles.installingBar}>
          <ProgressBar value={value} label={t("setup.download.progress", { name })} />
        </span>
        <span className={styles.installingText}>{t("common.installing")}</span>
      </span>
    );
  }
  return (
    <Button
      small
      disabled={wontFit}
      aria-label={t("settings.models.installLabel", { name })}
      onClick={() => void actions.installModel(model.id)}
    >
      {t("common.install")}
    </Button>
  );
}

function ModelTable({
  models,
  fits,
  onRemove,
}: {
  models: ModelInfo[];
  fits: Map<string, boolean>;
  onRemove(model: ModelInfo): void;
}) {
  return (
    <Panel>
      <table className={styles.table}>
        <thead>
          <tr>
            <th scope="col">{t("settings.models.name")}</th>
            <th scope="col">{t("settings.models.tier")}</th>
            <th scope="col">{t("settings.models.size")}</th>
            <th scope="col">{t("settings.models.license")}</th>
            <th scope="col" className={styles.right}>
              <span className="visually-hidden">{t("settings.models.actions")}</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {models.map((model) => (
            <tr key={model.id}>
              <td className={styles.strong}>{model.displayName}</td>
              <td>
                <Tag>{t(`tier.${model.tier}`)}</Tag>
              </td>
              <td>{formatBytes(model.installedSizeBytes ?? model.downloadSizeBytes)}</td>
              <td>{model.license}</td>
              <td className={styles.right}>
                <ModelAction
                  model={model}
                  wontFit={fits.get(model.id) === false}
                  onRemove={onRemove}
                />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </Panel>
  );
}

export function ModelsSection() {
  const { state, api, actions } = useApp();
  const [removing, setRemoving] = useState<ModelInfo | null>(null);
  const [recommendation, setRecommendation] = useState<Recommendation | null>(null);
  const [importId, setImportId] = useState<string>("");
  useInstalledToast(state.models, actions.toast);

  useEffect(() => {
    let cancelled = false;
    api
      .getRecommendation()
      .then((r) => {
        if (!cancelled) setRecommendation(r);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [api]);

  const fits = new Map(
    (recommendation?.fits ?? []).map((f) => [f.modelId, f.badge !== "wontFit"] as const),
  );
  const standard = state.models.filter((m) => !m.experimental);
  const advanced = state.models.filter((m) => m.experimental);
  const installed = standard.filter((m) => m.state === "installed");
  const available = standard.filter((m) => m.state !== "installed");
  const importable = state.models.filter((m) => m.state === "notInstalled");
  const importModelId = importable.some((m) => m.id === importId)
    ? importId
    : (importable[0]?.id ?? "");
  const modelsDir = state.settings?.modelsDir ?? "";
  const locked =
    state.models.some((m) => m.state !== "notInstalled") || state.setup?.phase === "downloading";

  // A model download that stopped: say why, and offer to try the same model again.
  const failedComponent =
    state.setup?.phase === "failed" && state.setup.error
      ? state.setup.components.find((c) => c.kind === "model" && c.state === "failed")
      : undefined;

  const confirmRemove = async () => {
    const model = removing;
    setRemoving(null);
    if (!model) return;
    await actions.removeModel(model.id);
    actions.toast(t("toast.removed", { name: modelShortName(model) }));
  };

  const removeBody = removing
    ? driveOf(modelsDir)
      ? t("settings.models.removeBody", {
          size: formatBytes(removing.installedSizeBytes ?? removing.downloadSizeBytes),
          drive: driveOf(modelsDir)!,
        })
      : t("settings.models.removeBodyNoDrive", {
          size: formatBytes(removing.installedSizeBytes ?? removing.downloadSizeBytes),
        })
    : "";

  return (
    <>
      <h1 className={styles.title}>{t("settings.section.models")}</h1>

      {failedComponent && state.setup?.error && (
        <div className={styles.notice}>
          <InlineNotice
            tone="err"
            alert
            title={t("setup.download.failedTitle")}
            body={setupErrorText(state.setup.error, state.setup.modelsDir)}
          >
            <Button onClick={() => void actions.installModel(failedComponent.id)}>
              {t("common.retry")}
            </Button>
          </InlineNotice>
        </div>
      )}

      <h2 className={styles.heading}>{t("settings.models.installed")}</h2>
      {installed.length > 0 ? (
        <ModelTable models={installed} fits={fits} onRemove={setRemoving} />
      ) : (
        <Panel>
          <PanelRow
            label={<span className={styles.muted}>{t("settings.models.noneInstalled")}</span>}
          />
        </Panel>
      )}

      <h2 className={styles.heading}>{t("settings.models.available")}</h2>
      {available.length > 0 ? (
        <ModelTable models={available} fits={fits} onRemove={setRemoving} />
      ) : (
        <Panel>
          <PanelRow
            label={<span className={styles.muted}>{t("settings.models.allInstalled")}</span>}
          />
        </Panel>
      )}

      <h2 className={styles.heading}>{t("settings.models.storage")}</h2>
      <Panel>
        <PanelRow
          label={t("settings.models.folder")}
          description={
            <>
              <span className={styles.mono}>{modelsDir}</span>
              {locked && <div>{t("settings.models.folderLocked")}</div>}
            </>
          }
        >
          <Button disabled={locked} onClick={() => void actions.changeModelsDir()}>
            {t("common.change")}
          </Button>
        </PanelRow>
        <PanelRow
          label={t("settings.models.import")}
          description={
            importable.length > 0
              ? t("settings.models.importDesc")
              : t("settings.models.importNothing")
          }
        >
          <Select
            aria-label={t("settings.models.importWhich")}
            value={importModelId}
            disabled={importable.length === 0}
            onChange={(e) => setImportId(e.target.value)}
          >
            {importable.map((m) => (
              <option key={m.id} value={m.id}>
                {modelShortName(m)}
              </option>
            ))}
          </Select>
          <Button
            disabled={importable.length === 0}
            onClick={() => void actions.importModel(importModelId)}
          >
            {t("settings.models.importButton")}
          </Button>
        </PanelRow>
      </Panel>

      {advanced.length > 0 && (
        <>
          <h2 className={styles.heading}>{t("settings.models.advanced")}</h2>
          <Panel>
            {advanced.map((model) => (
              <PanelRow
                key={model.id}
                align="start"
                label={
                  <span className={styles.labelWithTag}>
                    {modelShortName(model)}
                    <Tag tone="warn">{t("tag.experimental")}</Tag>
                    <Tag>{t("tag.community")}</Tag>
                  </span>
                }
                description={t("settings.models.advancedDesc", {
                  size: formatBytes(model.downloadSizeBytes),
                  license: model.license,
                })}
              >
                <ModelAction
                  model={model}
                  wontFit={fits.get(model.id) === false}
                  onRemove={setRemoving}
                />
              </PanelRow>
            ))}
          </Panel>
        </>
      )}

      {removing && (
        <ConfirmDialog
          title={t("settings.models.removeTitle", { name: modelShortName(removing) })}
          body={removeBody}
          confirmLabel={t("settings.models.removeAction")}
          onConfirm={() => void confirmRemove()}
          onCancel={() => setRemoving(null)}
        />
      )}
    </>
  );
}
