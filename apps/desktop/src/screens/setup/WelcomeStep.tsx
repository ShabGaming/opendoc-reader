import { Button } from "../../components/Button";
import { t } from "../../i18n";
import { useApp } from "../../state/AppContext";
import styles from "./Setup.module.css";
import { StepFrame } from "./StepFrame";

export function WelcomeStep() {
  const { actions } = useApp();
  const next = ["setup.welcome.next1", "setup.welcome.next2", "setup.welcome.next3"] as const;
  return (
    <StepFrame
      step={1}
      title={t("setup.welcome.title")}
      lede={t("setup.welcome.lede")}
      footer={
        <>
          <span className={styles.spacer} />
          <Button variant="primary" onClick={() => actions.setStep(2)}>
            {t("setup.welcome.start")}
          </Button>
        </>
      }
    >
      <p className={styles.emphasis}>{t("privacy.sentence")}</p>
      <ol className={styles.facts}>
        {next.map((key, i) => (
          <li key={key}>
            <span className={styles.factNumber}>{i + 1}</span>
            <span>{t(key)}</span>
          </li>
        ))}
      </ol>
    </StepFrame>
  );
}
