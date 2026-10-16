import { ArrowLeft, Cube, GearSix, Info, ShieldCheck, type Icon } from "@phosphor-icons/react";
import { Button } from "../../components/Button";
import { t, type MessageKey } from "../../i18n";
import { useApp } from "../../state/AppContext";
import type { SettingsSection } from "../../state/reducer";
import { AboutSection } from "./AboutSection";
import { GeneralSection } from "./GeneralSection";
import { ModelsSection } from "./ModelsSection";
import { PrivacySection } from "./PrivacySection";
import styles from "./Settings.module.css";

const SECTIONS: Array<{ id: SettingsSection; icon: Icon }> = [
  { id: "general", icon: GearSix },
  { id: "models", icon: Cube },
  { id: "privacy", icon: ShieldCheck },
  { id: "about", icon: Info },
];

export function SettingsScreen() {
  const { state, dispatch } = useApp();
  const section = state.settingsSection;
  const settings = state.settings;

  return (
    <div className={styles.settings}>
      <nav className={styles.nav} aria-label={t("settings.sections")}>
        <div className={styles.navHead}>
          <Button
            variant="ghost"
            icon={<ArrowLeft size={16} aria-hidden="true" />}
            onClick={() => dispatch({ type: "closeSettings" })}
          >
            {t("settings.back")}
          </Button>
        </div>
        {SECTIONS.map(({ id, icon: SectionIcon }) => (
          <button
            key={id}
            type="button"
            className={styles.navItem}
            aria-current={section === id ? "true" : undefined}
            onClick={() => dispatch({ type: "settingsSection", section: id })}
          >
            <SectionIcon size={16} aria-hidden="true" />
            <span>{t(`settings.section.${id}` as MessageKey)}</span>
          </button>
        ))}
      </nav>
      <main className={styles.body}>
        <div className={styles.inner}>
          {state.banner && (
            <div className={styles.banner} role="alert">
              {state.banner}
            </div>
          )}
          {section === "general" && settings && <GeneralSection settings={settings} />}
          {section === "models" && <ModelsSection />}
          {section === "privacy" && <PrivacySection />}
          {section === "about" && <AboutSection />}
        </div>
      </main>
    </div>
  );
}
