import { useEffect, useState, type MouseEvent, type ReactNode } from "react";
import { Panel, PanelRow } from "../../components/Panel";
import { t } from "../../i18n";
import { useApp } from "../../state/AppContext";
import contributorStyles from "./Contributors.module.css";
import { ContributorList } from "./ContributorList";
import { bundledContributors } from "./contributors";
import styles from "./Settings.module.css";

const SOURCE_URL = "https://github.com/ShabGaming/opendoc-reader";
const LICENSE_URL = "https://github.com/ShabGaming/opendoc-reader/blob/main/THIRD_PARTY_NOTICES.md";

function ExternalLink({ href, children }: { href: string; children: ReactNode }) {
  const { platform } = useApp();
  return (
    <a
      href={href}
      onClick={(event: MouseEvent) => {
        event.preventDefault();
        void platform.openUrl(href);
      }}
    >
      {children}
    </a>
  );
}

export function AboutSection() {
  const { api } = useApp();
  const [version, setVersion] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    api
      .appVersion()
      .then((v) => {
        if (!cancelled) setVersion(v);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [api]);

  return (
    <>
      <h1 className={styles.title}>{t("settings.section.about")}</h1>
      <Panel>
        <PanelRow
          label={<strong>{t("app.name")}</strong>}
          description={version ? t("settings.about.version", { version }) : undefined}
        />
        <PanelRow label={t("settings.about.source")}>
          <ExternalLink href={SOURCE_URL}>{SOURCE_URL.replace("https://", "")}</ExternalLink>
        </PanelRow>
        <PanelRow
          label={t("settings.about.licenses")}
          description={t("settings.about.licensesDesc")}
        >
          <ExternalLink href={LICENSE_URL}>{t("settings.about.viewLicenses")}</ExternalLink>
        </PanelRow>
      </Panel>
      {bundledContributors.length > 0 && (
        <>
          <h2 className={styles.heading}>{t("settings.about.contributors")}</h2>
          <Panel>
            <PanelRow description={t("settings.about.contributorsDesc")} />
            <div className={contributorStyles.wrap}>
              <ContributorList contributors={bundledContributors} />
            </div>
          </Panel>
        </>
      )}
    </>
  );
}
