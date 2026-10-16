import {
  Check,
  CheckCircle,
  Cpu,
  GraphicsCard,
  HardDrives,
  Memory,
  Warning,
} from "@phosphor-icons/react";
import type { ReactNode } from "react";
import { Button } from "../../components/Button";
import { Panel, PanelRow } from "../../components/Panel";
import { t } from "../../i18n";
import { useApp } from "../../state/AppContext";
import type { HardwareInfo, Recommendation } from "../../types/commands";
import { driveOf, formatDiskMB, formatMemoryMB } from "../../util/format";
import styles from "./Setup.module.css";
import { StepFrame } from "./StepFrame";
import type { ProbeState } from "./useSetupProbe";

function graphicsText(hw: HardwareInfo): string {
  if (hw.gpus.length === 0) return t("setup.computer.noGpu");
  return hw.gpus
    .map((gpu) => {
      const memory = gpu.vramMB ?? hw.unifiedMemoryMB;
      if (gpu.vramMB === null && hw.unifiedMemoryMB !== null) {
        return t("setup.computer.unified", {
          name: gpu.name,
          size: formatMemoryMB(hw.unifiedMemoryMB),
        });
      }
      return memory === null
        ? gpu.name
        : t("setup.computer.gpu", { name: gpu.name, size: formatMemoryMB(memory) });
    })
    .join(", ");
}

function spaceText(hw: HardwareInfo): string {
  const size = formatDiskMB(hw.freeDiskMB);
  const drive = driveOf(hw.modelsDir);
  return drive
    ? t("setup.computer.free", { size, drive })
    : t("setup.computer.freeNoDrive", { size });
}

function summaryText(recommendation: Recommendation): string {
  const badges = recommendation.fits.map((f) => f.badge).filter((b) => b !== "hidden");
  if (badges.every((b) => b === "runsWell")) return t("setup.computer.summaryAll");
  if (badges.some((b) => b === "wontFit")) return t("setup.computer.summaryLimited");
  return t("setup.computer.summarySome");
}

interface ComputerStepProps {
  probe: ProbeState;
  onRetry: () => void;
}

export function ComputerStep({ probe, onRetry }: ComputerStepProps) {
  const { actions } = useApp();
  const ready = probe.status === "ready";

  let content: ReactNode;
  if (probe.status === "loading") {
    content = <p role="status">{t("setup.computer.checking")}</p>;
  } else if (probe.status === "failed") {
    content = (
      <div className={styles.problem} role="alert">
        <p>{t("setup.computer.failed")}</p>
        <Button onClick={onRetry}>{t("common.retry")}</Button>
      </div>
    );
  } else {
    const hw = probe.hardware;
    const rows: Array<[ReactNode, string, string]> = [
      [
        <GraphicsCard key="g" size={16} aria-hidden="true" />,
        t("setup.computer.graphics"),
        graphicsText(hw),
      ],
      [
        <Memory key="m" size={16} aria-hidden="true" />,
        t("setup.computer.memory"),
        formatMemoryMB(hw.ramMB),
      ],
      [
        <Cpu key="c" size={16} aria-hidden="true" />,
        t("setup.computer.processor"),
        t("setup.computer.cpu", { name: hw.cpuName, cores: hw.cpuCores }),
      ],
      [
        <HardDrives key="d" size={16} aria-hidden="true" />,
        t("setup.computer.space"),
        spaceText(hw),
      ],
    ];
    const lowSpace = hw.freeDiskMB < 6000;
    content = (
      <>
        <Panel>
          {rows.map(([icon, key, value], i) => (
            <PanelRow
              key={key}
              label={
                <div className={styles.hardwareRow}>
                  {icon}
                  <span className={styles.hardwareKey}>{key}</span>
                  <span className={styles.hardwareValue}>{value}</span>
                </div>
              }
            >
              {lowSpace && i === 3 ? (
                <Warning className={styles.check} size={16} aria-hidden="true" />
              ) : (
                <Check className={styles.check} size={16} aria-hidden="true" />
              )}
              <span className="visually-hidden">
                {lowSpace && i === 3 ? t("setup.computer.lowSpace") : t("setup.computer.checked")}
              </span>
            </PanelRow>
          ))}
        </Panel>
        <div className={styles.summary}>
          <CheckCircle size={16} aria-hidden="true" />
          <span>{summaryText(probe.recommendation)}</span>
        </div>
      </>
    );
  }

  return (
    <StepFrame
      step={2}
      title={t("setup.computer.title")}
      lede={t("setup.computer.lede")}
      footer={
        <>
          <Button variant="tertiary" onClick={() => actions.setStep(1)}>
            {t("common.back")}
          </Button>
          <span className={styles.spacer} />
          <Button variant="primary" disabled={!ready} onClick={() => actions.setStep(3)}>
            {t("setup.continue")}
          </Button>
        </>
      }
    >
      {content}
    </StepFrame>
  );
}
