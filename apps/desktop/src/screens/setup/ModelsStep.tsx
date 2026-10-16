import { Check, CaretDown, CaretRight } from "@phosphor-icons/react";
import { useEffect, useId, useRef, useState } from "react";
import { Button } from "../../components/Button";
import { Tag } from "../../components/Tag";
import { t, type MessageKey } from "../../i18n";
import { useApp } from "../../state/AppContext";
import { modelShortName } from "../../state/selectors";
import type { FitBadge, ModelFit, ModelInfo, Recommendation } from "../../types/commands";
import { formatBytes, formatPageTime } from "../../util/format";
import { cx } from "../../util/cx";
import styles from "./Setup.module.css";
import { StepFrame } from "./StepFrame";
import type { ProbeState } from "./useSetupProbe";

function FitTag({ badge }: { badge: FitBadge }) {
  if (badge === "runsWell") {
    return (
      <Tag tone="ok" icon={<Check size={14} aria-hidden="true" />}>
        {t("fit.runsWell")}
      </Tag>
    );
  }
  if (badge === "slow") return <Tag tone="warn">{t("fit.slow")}</Tag>;
  if (badge === "wontFit") return <Tag tone="err">{t("fit.wontFit")}</Tag>;
  return null;
}

function bestFor(model: ModelInfo): string {
  return t(
    (model.experimental
      ? "setup.models.best.experimental"
      : `setup.models.best.${model.tier}`) as MessageKey,
  );
}

interface OptionRowProps {
  model: ModelInfo;
  fit: ModelFit;
  checked: boolean;
  recommended: boolean;
  reason: string;
  onToggle(checked: boolean): void;
}

function OptionRow({ model, fit, checked, recommended, reason, onToggle }: OptionRowProps) {
  const { platform } = useApp();
  const checkId = useId();
  const describedBy = useId();
  const popoverId = useId();
  const [open, setOpen] = useState(false);
  const rowRef = useRef<HTMLDivElement>(null);
  const name = modelShortName(model);
  const disabled = fit.badge === "wontFit";

  // The popover closes on a click outside it or on Escape.
  useEffect(() => {
    if (!open) return;
    const onPointer = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!rowRef.current?.contains(target)) setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("pointerdown", onPointer);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onPointer);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const speed = fit.speed
    ? t(fit.speed.basis === "estimated" ? "setup.models.speedAbout" : "setup.models.speedExact", {
        time: formatPageTime(fit.speed.secondsPerPage),
      })
    : null;

  return (
    <div ref={rowRef} className={cx(styles.option, recommended && styles.recommended)}>
      <input
        id={checkId}
        type="checkbox"
        checked={checked}
        disabled={disabled}
        aria-describedby={describedBy}
        onChange={(e) => onToggle(e.target.checked)}
      />
      <div className={styles.optionBody}>
        <label htmlFor={checkId} className={styles.optionName}>
          {name}
          {!model.experimental && <small>{model.displayName}</small>}
          {recommended && <Tag tone="accent">{t("tag.recommended")}</Tag>}
          {model.experimental && <Tag tone="warn">{t("tag.experimental")}</Tag>}
          {model.experimental && <Tag>{t("tag.community")}</Tag>}
        </label>
        <div id={describedBy}>
          <div className={styles.bestFor}>{bestFor(model)}</div>
          <div className={styles.meta}>
            <span>
              {t("setup.models.download", { size: formatBytes(model.downloadSizeBytes) })}
            </span>
            {speed && <span>{speed}</span>}
          </div>
          {recommended && <div className={styles.why}>{reason}</div>}
        </div>
      </div>
      <div className={styles.optionSide}>
        <FitTag badge={fit.badge} />
        <Button
          variant="tertiary"
          small
          aria-expanded={open}
          aria-controls={open ? popoverId : undefined}
          onClick={() => setOpen((o) => !o)}
        >
          {t("setup.models.details")}
        </Button>
      </div>
      {open && (
        <div
          id={popoverId}
          role="dialog"
          aria-label={t("setup.models.detailsOf", { name })}
          className={styles.popover}
        >
          <dl className={styles.details}>
            <dt>{t("setup.details.model")}</dt>
            <dd>{model.displayName}</dd>
            <dt>{t("setup.details.license")}</dt>
            <dd>{model.license}</dd>
            {model.sourceUrl && (
              <>
                <dt>{t("setup.details.source")}</dt>
                <dd>
                  <a
                    href={model.sourceUrl}
                    onClick={(event) => {
                      event.preventDefault();
                      void platform.openUrl(model.sourceUrl!);
                    }}
                  >
                    {model.sourceUrl.replace(/^https?:\/\//, "")}
                  </a>
                </dd>
              </>
            )}
            <dt>{t("setup.details.download")}</dt>
            <dd>{formatBytes(model.downloadSizeBytes)}</dd>
          </dl>
        </div>
      )}
    </div>
  );
}

interface ModelsStepProps {
  probe: ProbeState;
  onRetry: () => void;
}

const TIER_ORDER = { fast: 0, accurate: 1, max: 2 } as const;

export function ModelsStep({ probe, onRetry }: ModelsStepProps) {
  const { state, dispatch, actions } = useApp();
  const selected = state.setupUi.selected ?? [];
  const advancedOpen = state.setupUi.advancedOpen;
  const modelsDir = state.settings?.modelsDir ?? "";

  const rec: Recommendation | null = probe.status === "ready" ? probe.recommendation : null;
  const visible = (rec?.fits ?? [])
    .filter((f) => f.badge !== "hidden")
    .map((fit) => ({ fit, model: state.models.find((m) => m.id === fit.modelId) }))
    .filter((x): x is { fit: ModelFit; model: ModelInfo } => x.model !== undefined);
  const standard = visible
    .filter((x) => !x.model.experimental)
    .sort((a, b) => TIER_ORDER[a.model.tier] - TIER_ORDER[b.model.tier]);
  const advanced = visible.filter((x) => x.model.experimental);

  const modelBytes = visible
    .filter((x) => selected.includes(x.model.id))
    .reduce((sum, x) => sum + x.model.downloadSizeBytes, 0);
  const runtimeBytes = rec?.runtimeDownloadBytes ?? 0;
  const none = selected.length === 0;

  const toggle = (id: string, checked: boolean) => {
    const next = checked ? [...selected, id] : selected.filter((s) => s !== id);
    dispatch({ type: "setupSelect", selected: next });
  };

  const row = ({ fit, model }: { fit: ModelFit; model: ModelInfo }) => (
    <OptionRow
      key={model.id}
      model={model}
      fit={fit}
      checked={selected.includes(model.id)}
      recommended={rec?.modelId === model.id}
      reason={rec?.reason ?? ""}
      onToggle={(checked) => toggle(model.id, checked)}
    />
  );

  let content;
  if (probe.status === "loading") {
    content = <p role="status">{t("common.checking")}</p>;
  } else if (probe.status === "failed") {
    content = (
      <div className={styles.problem} role="alert">
        <p>{t("setup.models.loadFailed")}</p>
        <Button onClick={onRetry}>{t("common.retry")}</Button>
      </div>
    );
  } else {
    content = (
      <>
        {standard.map(row)}
        {advanced.length > 0 && (
          <>
            <button
              type="button"
              className={styles.disclose}
              aria-expanded={advancedOpen}
              onClick={() => dispatch({ type: "setupAdvanced", open: !advancedOpen })}
            >
              {advancedOpen ? (
                <CaretDown size={12} aria-hidden="true" />
              ) : (
                <CaretRight size={12} aria-hidden="true" />
              )}
              {t("setup.models.advanced")}
            </button>
            {advancedOpen && (
              <div className={styles.advanced}>
                {advanced.map(row)}
                <div className={styles.legend}>
                  <div className={styles.legendTitle}>{t("fit.legendTitle")}</div>
                  <div className={styles.legendRow}>
                    <FitTag badge="runsWell" />
                    <span>{t("fit.runsWellText")}</span>
                  </div>
                  <div className={styles.legendRow}>
                    <FitTag badge="slow" />
                    <span>{t("fit.slowText")}</span>
                  </div>
                  <div className={styles.legendRow}>
                    <FitTag badge="wontFit" />
                    <span>{t("fit.wontFitText")}</span>
                  </div>
                </div>
              </div>
            )}
          </>
        )}
        <div className={styles.storage}>
          <span>
            {t("setup.models.stored")} <span className={styles.path}>{modelsDir}</span>
          </span>
          <span className={styles.spacer} />
          <Button onClick={() => void actions.changeModelsDir()}>{t("common.change")}</Button>
        </div>
      </>
    );
  }

  return (
    <StepFrame
      step={3}
      title={t("setup.models.title")}
      lede={t("setup.models.lede")}
      footer={
        <>
          <Button variant="tertiary" onClick={() => actions.setStep(2)}>
            {t("common.back")}
          </Button>
          <span className={styles.spacer} />
          <span className={styles.footerNote}>
            {none
              ? t("setup.models.none")
              : t("setup.models.breakdown", {
                  model: formatBytes(modelBytes),
                  runtime: formatBytes(runtimeBytes),
                })}
          </span>
          <Button
            variant="primary"
            disabled={none || probe.status !== "ready"}
            onClick={() => {
              void actions.startDownload(selected);
              actions.setStep(4);
            }}
          >
            {none
              ? t("setup.models.chooseButton")
              : t("setup.models.downloadButton", { size: formatBytes(modelBytes + runtimeBytes) })}
          </Button>
        </>
      }
    >
      {content}
    </StepFrame>
  );
}
