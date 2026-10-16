import { Check, Copy, FolderOpen } from "@phosphor-icons/react";
import { useEffect, useRef, useState } from "react";
import { Button } from "../../components/Button";
import { Panel, PanelRow } from "../../components/Panel";
import { t } from "../../i18n";
import { useApp } from "../../state/AppContext";
import styles from "./Settings.module.css";

const COPIED_MS = 2500;

export function PrivacySection() {
  const { api, platform, actions } = useApp();
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );

  const copy = async () => {
    try {
      await platform.copyText(await api.getDiagnostics());
    } catch {
      actions.setBanner(t("settings.privacy.copyFailed"));
      return;
    }
    setCopied(true);
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => setCopied(false), COPIED_MS);
  };

  return (
    <>
      <h1 className={styles.title}>{t("settings.section.privacy")}</h1>
      <Panel>
        <PanelRow
          label={<strong>{t("settings.privacy.title")}</strong>}
          description={t("settings.privacy.body")}
        />
        <PanelRow label={t("settings.privacy.logs")} description={t("settings.privacy.logsDesc")}>
          <Button
            icon={<FolderOpen size={16} aria-hidden="true" />}
            onClick={() => void api.openLogsFolder()}
          >
            {t("settings.privacy.openLogs")}
          </Button>
        </PanelRow>
        <PanelRow
          label={t("settings.privacy.diagnostics")}
          description={t("settings.privacy.diagnosticsDesc")}
        >
          <span className={styles.copied} aria-live="polite">
            {copied && (
              <>
                <Check size={14} aria-hidden="true" />
                {t("settings.privacy.copied")}
              </>
            )}
          </span>
          <Button icon={<Copy size={16} aria-hidden="true" />} onClick={() => void copy()}>
            {t("settings.privacy.copy")}
          </Button>
        </PanelRow>
      </Panel>
    </>
  );
}
