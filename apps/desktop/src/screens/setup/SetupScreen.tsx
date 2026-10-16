import { Check, GearSix, Scan } from "@phosphor-icons/react";
import { t, type MessageKey } from "../../i18n";
import { useApp } from "../../state/AppContext";
import { cx } from "../../util/cx";
import { ComputerStep } from "./ComputerStep";
import { DownloadStep } from "./DownloadStep";
import { ModelsStep } from "./ModelsStep";
import styles from "./Setup.module.css";
import { useSetupProbe } from "./useSetupProbe";
import { WelcomeStep } from "./WelcomeStep";

const STEPS = [1, 2, 3, 4] as const;

/** First run: welcome, a look at the computer, a model choice, then the download. */
export function SetupScreen() {
  const { state, actions } = useApp();
  const { probe, retry } = useSetupProbe();
  const step = state.setupUi.step;
  const phase = state.setup?.phase;
  // Once the download starts there is no going back, except to fix a failure.
  const locked =
    step === 4 && (phase === "downloading" || phase === "paused" || phase === "calibrating");

  return (
    <div className={styles.setup}>
      <nav className={styles.rail} aria-label={t("setup.rail")}>
        <div className={styles.brand}>
          <Scan size={20} aria-hidden="true" />
          <span>{t("app.name")}</span>
        </div>
        <ol className={styles.steps}>
          {STEPS.map((n) => {
            const complete = n < step;
            return (
              <li key={n}>
                <button
                  type="button"
                  className={cx(
                    styles.step,
                    n === step && styles.current,
                    complete && styles.complete,
                  )}
                  aria-current={n === step ? "step" : undefined}
                  disabled={!complete || locked}
                  onClick={() => actions.setStep(n)}
                >
                  <span className={styles.stepNumber}>
                    {complete ? <Check size={12} aria-hidden="true" /> : n}
                  </span>
                  <span>{t(`setup.step.${n}` as MessageKey)}</span>
                </button>
              </li>
            );
          })}
        </ol>
        <div className={styles.railSpacer} />
        <div className={styles.railFooter}>
          <button
            type="button"
            className={styles.settingsButton}
            onClick={() => actions.openSettings()}
          >
            <GearSix size={16} aria-hidden="true" />
            {t("settings.open")}
          </button>
        </div>
      </nav>
      {step === 1 && <WelcomeStep />}
      {step === 2 && <ComputerStep probe={probe} onRetry={retry} />}
      {step === 3 && <ModelsStep probe={probe} onRetry={retry} />}
      {step === 4 && <DownloadStep />}
    </div>
  );
}
