import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { createRuntime } from "./api/runtime";
import { AppProvider } from "./state/AppContext";
import "./styles/tokens.css";
import "./styles/base.css";

const root = document.getElementById("root");
if (!root) throw new Error("Missing #root element");

// Inside Tauri this is the real core. In a browser, or with ?mock=1, it is the mock.
const runtime = createRuntime();

createRoot(root).render(
  <StrictMode>
    <AppProvider
      api={runtime.api}
      platform={runtime.platform}
      dev={runtime.mock ? runtime.dev : null}
    >
      <App />
    </AppProvider>
  </StrictMode>,
);
