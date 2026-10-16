import { useEffect } from "react";
import styles from "./App.module.css";
import { ToastHost } from "./components/ToastHost";
import { t } from "./i18n";
import { MainScreen } from "./screens/main/MainScreen";
import { SettingsScreen } from "./screens/settings/SettingsScreen";
import { SetupScreen } from "./screens/setup/SetupScreen";
import { useApp } from "./state/AppContext";

/** Ctrl or Cmd with O adds files, and with a comma opens Settings. */
function useGlobalShortcuts() {
  const { state, actions } = useApp();
  const screen = state.screen;
  const { chooseFiles, openSettings } = actions;
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || event.altKey || event.shiftKey) return;
      if (event.key === "o" && screen === "main") {
        event.preventDefault();
        void chooseFiles();
      } else if (event.key === ",") {
        event.preventDefault();
        if (screen !== "settings") openSettings();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [screen, chooseFiles, openSettings]);
}

export function App() {
  const { state } = useApp();
  useGlobalShortcuts();

  let screen;
  if (state.boot === "loading") {
    screen = (
      <p className={styles.message} role="status">
        {t("app.loading")}
      </p>
    );
  } else if (state.boot === "failed") {
    screen = (
      <div className={styles.message} role="alert">
        <h1 className={styles.failedTitle}>{t("app.startFailed")}</h1>
        <p>{t("app.startFailedBody")}</p>
      </div>
    );
  } else if (state.screen === "setup") {
    screen = <SetupScreen />;
  } else if (state.screen === "settings") {
    screen = <SettingsScreen />;
  } else {
    screen = <MainScreen />;
  }

  return (
    <div className={styles.app}>
      {screen}
      <ToastHost />
    </div>
  );
}
