import { CheckCircle, Hourglass, Pause, Play } from "@phosphor-icons/react";
import { Button } from "../../components/Button";
import { InlineNotice } from "../../components/InlineNotice";
import { Panel } from "../../components/Panel";
import { ProgressBar } from "../../components/ProgressBar";
import { t } from "../../i18n";
import { useApp } from "../../state/AppContext";
import { setupErrorText } from "../../state/errorCopy";
import type { SetupComponent, SetupStatus } from "../../types/commands";
import {
  formatDuration,
  formatPageTime,
  formatProgressBytes,
  formatSpeed,
} from "../../util/format";
import styles from "./Setup.module.css";
import { StepFrame } from "./StepFrame";

function stateText(component: SetupComponent, phase: SetupStatus["phase"]): string {
  if (component.state === "done") return t("setup.download.done");
  if (component.state === "failed") return t("setup.download.failed");
  if (component.state === "verifying") return t("setup.download.verifying");
  if (component.state === "pending") return t("setup.download.pending");
  if (phase === "paused") return t("setup.download.paused");
  if (phase === "failed") return t("setup.download.stopped");
  return component.bytesPerSecond ? formatSpeed(component.bytesPerSecond) : "";
}

function ComponentRow({
  component,
  phase,
}: {
  component: SetupComponent;
  phase: SetupStatus["phase"];
}) {
  const known = component.bytesTotal > 0;
  const done = component.state === "done";
  const value = done ? 1 : known ? component.bytesDone / component.bytesTotal : null;
  const numbers = known
    ? formatProgressBytes(done ? component.bytesTotal : component.bytesDone, component.bytesTotal)
    : "";
  return (
    <div className={styles.component}>
      <div className={styles.componentTop}>
        <span className={styles.componentName}>{component.name}</span>
        <span className={styles.componentState}>{stateText(component, phase)}</span>
      </div>
      <ProgressBar
        value={value}
        label={t("setup.download.progress", { name: component.name })}
        valueText={numbers}
      />
      <div className={styles.componentNumbers}>
        <span>{numbers}</span>
        <span>
          {component.secondsLeft !== null &&
          component.state === "downloading" &&
          phase === "downloading"
            ? t("setup.download.left", { time: formatDuration(component.secondsLeft) })
            : ""}
        </span>
      </div>
    </div>
  );
}

function Calibration({ setup }: { setup: SetupStatus }) {
  let content = null;
  if (setup.phase === "calibrating") {
    content = (
      <span className={styles.calibrationBusy}>
        <Hourglass size={16} aria-hidden="true" />
        {t("setup.download.calibrating")}
      </span>
    );
  } else if (setup.phase === "ready" && setup.calibration) {
    content = (
      <span className={styles.calibrationDone}>
        <CheckCircle size={16} aria-hidden="true" />
        {t("setup.download.calibrated", { time: formatPageTime(setup.calibration.secondsPerPage) })}
      </span>
    );
  }
  return (
    <div className={styles.calibration} aria-live="polite">
      {content}
    </div>
  );
}

export function DownloadStep() {
  const { state, actions } = useApp();
  const setup = state.setup;
  if (!setup) return null;

  const active = setup.phase === "downloading" || setup.phase === "paused";
  const busy = active || setup.phase === "calibrating";
  const paused = setup.phase === "paused";
  const finished = setup.phase === "ready" && setup.calibration !== null;
  const retryIds =
    state.setupUi.started.length > 0 ? state.setupUi.started : (state.setupUi.selected ?? []);

  return (
    <StepFrame
      step={4}
      title={t("setup.download.title")}
      lede={t("setup.download.lede")}
      footer={
        <>
          <Button variant="tertiary" disabled={busy || finished} onClick={() => actions.setStep(3)}>
            {t("common.back")}
          </Button>
          <span className={styles.spacer} />
          <Button variant="primary" disabled={!finished} onClick={actions.finishSetup}>
            {t("setup.download.finish")}
          </Button>
        </>
      }
    >
      {setup.phase === "failed" && setup.error && (
        <div className={styles.noticeSlot}>
          <InlineNotice
            tone="err"
            alert
            title={t("setup.download.failedTitle")}
            body={setupErrorText(setup.error, setup.modelsDir)}
          >
            <Button onClick={() => void actions.startDownload(retryIds)}>
              {t("common.retry")}
            </Button>
          </InlineNotice>
        </div>
      )}
      <Panel>
        {setup.components.map((component) => (
          <ComponentRow key={component.id} component={component} phase={setup.phase} />
        ))}
      </Panel>
      {active && (
        <div className={styles.downloadActions}>
          <Button
            icon={
              paused ? (
                <Play size={16} aria-hidden="true" />
              ) : (
                <Pause size={16} aria-hidden="true" />
              )
            }
            onClick={paused ? actions.resumeDownload : actions.pauseDownload}
          >
            {paused ? t("setup.download.resume") : t("setup.download.pause")}
          </Button>
        </div>
      )}
      <Calibration setup={setup} />
    </StepFrame>
  );
}
