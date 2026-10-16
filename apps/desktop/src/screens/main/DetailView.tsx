import { CheckCircle, Clock, Columns, Pause, Play, Prohibit, X } from "@phosphor-icons/react";
import { Button } from "../../components/Button";
import { OutputActions } from "../../components/OutputActions";
import { InlineNotice } from "../../components/InlineNotice";
import { ProgressBar } from "../../components/ProgressBar";
import { t, tp } from "../../i18n";
import { useApp } from "../../state/AppContext";
import { useOutputs } from "../../state/useOutputs";
import {
  adviceText,
  conversionErrorAdvice,
  conversionErrorText,
  isRetryable,
} from "../../state/errorCopy";
import {
  effectiveModelId,
  hasStatus,
  modelShortName,
  percentDone,
  smallerInstalledModel,
} from "../../state/selectors";
import type { QueueItem } from "../../types/commands";
import {
  fileName,
  formatBytes,
  formatDuration,
  formatDurationMs,
  parentDir,
} from "../../util/format";
import styles from "./DetailView.module.css";

export function DetailView({ item }: { item: QueueItem }) {
  const { state, platform } = useApp();
  const modelId = item.modelId ?? effectiveModelId(state);
  const model = state.models.find((m) => m.id === modelId);
  const kind = item.kind === "pdf" ? t("detail.kind.pdf") : imageKind(item.name);

  return (
    <div className={styles.scroll}>
      <div className={styles.view}>
        <div className={styles.preview}>
          {item.previewPath ? (
            <img
              src={platform.assetUrl(item.previewPath)}
              alt={t("detail.previewAlt", { name: item.name })}
            />
          ) : (
            <div className={styles.noPreview}>{t("detail.noPreview")}</div>
          )}
        </div>
        <div className={styles.info}>
          <div>
            <h2 className={styles.name}>{item.name}</h2>
            {item.pageCount !== null && (
              <div className={styles.meta}>
                {t("detail.meta", {
                  pages: tp("detail.pages", item.pageCount),
                  size: formatBytes(item.sizeBytes),
                })}
              </div>
            )}
          </div>
          <StatusBlock item={item} />
          <dl className={styles.kv}>
            <dt>{t("detail.type")}</dt>
            <dd>{kind}</dd>
            {item.pageCount !== null && (
              <>
                <dt>{t("detail.pagesLabel")}</dt>
                <dd>{item.pageCount}</dd>
              </>
            )}
            <dt>{t("detail.size")}</dt>
            <dd>{formatBytes(item.sizeBytes)}</dd>
            <dt>{t("detail.model")}</dt>
            <dd>{model ? modelShortName(model) : t("detail.modelNone")}</dd>
          </dl>
        </div>
      </div>
    </div>
  );
}

function imageKind(name: string): string {
  const ext = name.split(".").pop()?.toUpperCase() ?? "";
  return ext
    ? `${ext === "JPG" ? "JPEG" : ext} ${t("detail.kind.image").toLowerCase()}`
    : t("detail.kind.image");
}

function StatusBlock({ item }: { item: QueueItem }) {
  const { state, actions } = useApp();
  const paused = hasStatus(state.queue, "paused");

  switch (item.status) {
    case "converting":
    case "paused": {
      const isPaused = item.status === "paused";
      const total = item.pageCount ?? 0;
      const headline =
        item.currentPage !== null
          ? t("detail.converting", { page: item.currentPage + 1, total })
          : t("detail.stage.export");
      return (
        <>
          <div>
            <div className={styles.headlineAccent}>{isPaused ? t("detail.paused") : headline}</div>
            <div className={styles.bar}>
              <ProgressBar
                value={total > 0 ? item.pagesDone / total : null}
                label={t("detail.progress")}
                valueText={`${percentDone(item)}%`}
              />
            </div>
            <div className={styles.muted}>
              {isPaused
                ? t("detail.pausedBody")
                : item.secondsLeft !== null
                  ? t("detail.eta", { time: formatDuration(item.secondsLeft) })
                  : ""}
            </div>
          </div>
          <div className={styles.actions}>
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
          </div>
        </>
      );
    }
    case "done":
      return <DoneBlock item={item} />;
    case "needsAttention":
      return (
        <>
          <InlineNotice title={t("notice.hasText")} body={t("notice.hasTextBody")}>
            <Button onClick={() => actions.setTextMode(item.fileId, "existingText")}>
              {t("notice.useExisting")}
            </Button>
            <Button onClick={() => actions.setTextMode(item.fileId, "ocr")}>
              {t("notice.runOcr")}
            </Button>
          </InlineNotice>
          <RemoveButton item={item} />
        </>
      );
    case "failed":
      return <FailedBlock item={item} />;
    case "cancelled":
      return (
        <>
          <div>
            <div className={styles.headlineMuted}>
              <Prohibit size={16} aria-hidden="true" />
              {t("detail.cancelled")}
            </div>
            <div className={styles.muted}>{t("detail.cancelledBody")}</div>
          </div>
          <div className={styles.actions}>
            <Button onClick={() => actions.retry(item.fileId)}>{t("detail.addAgain")}</Button>
            <RemoveButton item={item} />
          </div>
        </>
      );
    default: {
      const inspecting = item.pageCount === null;
      const started = item.modelId !== null;
      return (
        <>
          <div>
            <div className={styles.headlineMuted}>
              <Clock size={16} aria-hidden="true" />
              {inspecting
                ? t("detail.inspecting")
                : started
                  ? t("detail.waiting")
                  : t("detail.ready")}
            </div>
            {!inspecting && (
              <div className={styles.muted}>
                {started ? t("detail.waitingBody") : t("detail.readyBody")}
              </div>
            )}
          </div>
          <RemoveButton item={item} />
        </>
      );
    }
  }
}

function DoneBlock({ item }: { item: QueueItem }) {
  const { actions } = useApp();
  const outputs = useOutputs(item);
  const first = item.outputs[0];
  const folder = first ? parentDir(first.path) : null;
  const nextToOriginal = folder === parentDir(item.path);
  return (
    <>
      <div>
        <div className={styles.headlineOk}>
          <CheckCircle size={16} aria-hidden="true" />
          {item.durationMs !== null
            ? t("detail.done", { time: formatDurationMs(item.durationMs) })
            : t("detail.doneNoTime")}
        </div>
        {first && (
          <>
            <div className={styles.savedLabel}>
              {nextToOriginal
                ? t("detail.savedNextTo")
                : t("detail.savedIn", { folder: folder ?? "" })}
            </div>
            <div className={styles.mono}>
              {item.outputs.map((o) => fileName(o.path)).join(", ")}
            </div>
          </>
        )}
      </div>
      {outputs.notice && <InlineNotice title={outputs.notice} />}
      <div className={styles.actions}>
        <OutputActions outputs={outputs} onReveal={(path) => void actions.reveal(path)}>
          <Button
            icon={<Columns size={16} aria-hidden="true" />}
            onClick={() => actions.openReview(item.fileId)}
          >
            {t("detail.review")}
          </Button>
        </OutputActions>
      </div>
    </>
  );
}

function RemoveButton({ item }: { item: QueueItem }) {
  const { actions } = useApp();
  return (
    <div className={styles.actions}>
      <Button variant="tertiary" onClick={() => actions.removeItem(item.fileId)}>
        {t("detail.removeFromQueue")}
      </Button>
    </div>
  );
}

function FailedBlock({ item }: { item: QueueItem }) {
  const { state, actions } = useApp();
  const error = item.error;
  const advice = error ? conversionErrorAdvice(error) : "retry";
  const smaller =
    advice === "smallerModel" ? smallerInstalledModel(state.models, item.modelId) : null;
  return (
    <>
      <InlineNotice
        tone="err"
        alert
        title={error ? conversionErrorText(error) : t("error.convert.failed")}
        body={adviceText(advice)}
      >
        {(!error || isRetryable(error)) && (
          <Button onClick={() => actions.retry(item.fileId)}>{t("common.retry")}</Button>
        )}
        {smaller && (
          <Button onClick={() => actions.addAgainWith(item.fileId, smaller.id)}>
            {t("error.addAgainWith", { model: modelShortName(smaller) })}
          </Button>
        )}
      </InlineNotice>
      <RemoveButton item={item} />
    </>
  );
}
