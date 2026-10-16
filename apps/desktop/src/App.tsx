import { useState } from "react";
import styles from "./App.module.css";
import { ActionBar, type ActionBarSettings } from "./components/ActionBar";
import { DropTarget } from "./components/DropTarget";
import { Sidebar } from "./components/Sidebar";

const defaultSettings: ActionBarSettings = {
  word: true,
  markdown: true,
  model: "accurate",
  saveTo: "original",
};

export function App() {
  const [settings, setSettings] = useState<ActionBarSettings>(defaultSettings);

  return (
    <div className={styles.shell}>
      <Sidebar />
      <div className={styles.main}>
        <main className={styles.content}>
          <DropTarget />
        </main>
        <ActionBar settings={settings} onChange={setSettings} />
      </div>
    </div>
  );
}
