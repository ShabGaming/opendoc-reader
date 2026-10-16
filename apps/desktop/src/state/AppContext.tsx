import {
  createContext,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useReducer,
  useRef,
  type Dispatch,
  type ReactNode,
} from "react";
import type { Api } from "../api/api";
import type { Platform } from "../api/platform";
import type { DevParams } from "../api/runtime";
import { t, tp } from "../i18n";
import type { AppError, ExportFormat, ModelInfo, SettingsPatch, TextMode } from "../types/commands";
import { baseName, fileName, parentDir } from "../util/format";
import { isAppError } from "./errorCopy";
import {
  initialState,
  reducer,
  type Action,
  type AppState,
  type SettingsSection,
  type SetupUi,
} from "./reducer";
import { convertibleItems, effectiveModelId } from "./selectors";

export interface Actions {
  toast(text: string): void;
  setBanner(text: string | null): void;
  addPaths(paths: string[]): Promise<void>;
  chooseFiles(): Promise<void>;
  startConversion(): Promise<void>;
  pauseAll(): Promise<void>;
  resumeAll(): Promise<void>;
  cancelAll(): Promise<void>;
  clearDone(): Promise<void>;
  removeItem(fileId: string): Promise<void>;
  retry(fileId: string): Promise<void>;
  addAgainWith(fileId: string, modelId: string): Promise<void>;
  setTextMode(fileId: string, mode: TextMode): Promise<void>;
  /** Opens a saved output with the OS default app. "missing" when the file is gone. */
  openOutput(path: string): Promise<"opened" | "missing" | "failed">;
  /** Asks where to save a copy, then writes that format there. False when cancelled or failed. */
  saveCopy(fileId: string, format: ExportFormat): Promise<boolean>;
  reveal(path: string): Promise<void>;
  /** The folder picker. Null when it was cancelled or could not open. */
  pickFolder(defaultPath?: string): Promise<string | null>;
  chooseSaveFolder(): Promise<void>;
  updateSettings(patch: SettingsPatch): Promise<boolean>;
  refreshModels(): Promise<ModelInfo[]>;
  installModel(modelId: string): Promise<void>;
  removeModel(modelId: string): Promise<void>;
  importModel(modelId: string): Promise<void>;
  startDownload(modelIds: string[]): Promise<void>;
  pauseDownload(): Promise<void>;
  resumeDownload(): Promise<void>;
  changeModelsDir(): Promise<boolean>;
  finishSetup(): void;
  openReview(fileId: string): void;
  closeReview(): void;
  openSettings(section?: SettingsSection): void;
  setStep(step: SetupUi["step"]): void;
}

interface AppContextValue {
  state: AppState;
  dispatch: Dispatch<Action>;
  api: Api;
  platform: Platform;
  dev: DevParams | null;
  actions: Actions;
}

const AppContext = createContext<AppContextValue | null>(null);

export function useApp(): AppContextValue {
  const value = useContext(AppContext);
  if (!value) throw new Error("useApp must be used inside AppProvider");
  return value;
}

interface ProviderProps {
  api: Api;
  platform: Platform;
  dev?: DevParams | null;
  /** Used by tests to start from a prepared state instead of loading it. */
  children: ReactNode;
}

function describe(error: unknown): AppError | null {
  return isAppError(error) ? error : null;
}

export function AppProvider({ api, platform, dev = null, children }: ProviderProps) {
  const [state, dispatch] = useReducer(reducer, initialState);
  const stateRef = useRef(state);
  useLayoutEffect(() => {
    stateRef.current = state;
  });
  const toastId = useRef(0);

  // Boot: subscribe first, then load, so no event slips between the two.
  useEffect(() => {
    let cancelled = false;
    const unsubscribe: Array<() => void> = [];
    (async () => {
      try {
        const offQueue = await api.onQueueProgress((progress) =>
          dispatch({ type: "queueProgress", progress }),
        );
        const offSetup = await api.onSetupProgress((status) =>
          dispatch({ type: "setupProgress", status }),
        );
        unsubscribe.push(offQueue, offSetup);
        if (cancelled) return;
        const [setup, settings, models, queue] = await Promise.all([
          api.setupStatus(),
          api.settingsGet(),
          api.modelsList(),
          api.queueList(),
        ]);
        if (cancelled) return;
        dispatch({ type: "booted", setup, settings, models, queue });
        if (dev) applyDevParams(dev, dispatch, queue);
      } catch {
        if (!cancelled) dispatch({ type: "bootFailed" });
      }
    })();
    return () => {
      cancelled = true;
      unsubscribe.forEach((off) => off());
    };
  }, [api, dev]);

  // Theme: System removes the attribute so the media query decides.
  const theme = state.settings?.theme;
  useEffect(() => {
    if (!theme) return;
    const root = document.documentElement;
    if (theme === "system") root.removeAttribute("data-theme");
    else root.setAttribute("data-theme", theme);
  }, [theme]);

  // The models list follows the setup status: a finished download changes it.
  const setupSignature = state.setup
    ? `${state.setup.phase}|${state.setup.components.map((c) => `${c.id}:${c.state}`).join(",")}`
    : "";
  useEffect(() => {
    if (!setupSignature) return;
    let cancelled = false;
    api
      .modelsList()
      .then((models) => {
        if (!cancelled) dispatch({ type: "modelsLoaded", models });
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [api, setupSignature]);

  const actions = useMemo<Actions>(() => {
    const fail = (text: string) => dispatch({ type: "banner", text });
    const toast = (text: string) => {
      toastId.current += 1;
      dispatch({ type: "toast", id: toastId.current, text });
    };
    const guard = async (fn: () => Promise<void>, onError?: (e: AppError | null) => string) => {
      try {
        await fn();
      } catch (error) {
        fail(onError ? onError(describe(error)) : t("error.actionFailed"));
      }
    };
    const refreshQueue = async () => {
      dispatch({ type: "queueLoaded", queue: await api.queueList() });
    };

    const a: Actions = {
      toast,
      setBanner: (text) => dispatch({ type: "banner", text }),

      async addPaths(paths) {
        if (paths.length === 0) return;
        await guard(
          async () => {
            const items = await api.queueAdd(paths);
            dispatch({ type: "queueAdded", items });
            dispatch({ type: "banner", text: null });
            toast(tp("toast.added", items.length));
          },
          (e) =>
            e?.code === "badInput" && e.reason === "unsupported_format"
              ? t("error.addFailedUnsupported")
              : t("error.addFailed"),
        );
      },
      async chooseFiles() {
        let paths: string[];
        try {
          paths = await platform.pickFiles();
        } catch {
          fail(t("error.pickerFailed"));
          return;
        }
        await a.addPaths(paths);
      },
      async startConversion() {
        const s = stateRef.current;
        const modelId = effectiveModelId(s);
        const count = convertibleItems(s.queue).length;
        if (!modelId || count === 0) return;
        await guard(async () => {
          await api.queueStart({
            formats: s.choice.formats,
            modelId,
            saveTo: s.choice.saveTo,
          });
          dispatch({ type: "banner", text: null });
          toast(tp("toast.queued", count));
        });
      },
      pauseAll: () => guard(() => api.queuePause().then(() => {})),
      resumeAll: () => guard(() => api.queueResume().then(() => {})),
      cancelAll: () => guard(() => api.queueCancel(null).then(() => {})),
      async clearDone() {
        await guard(async () => {
          const ids = await api.queueClearDone();
          dispatch({ type: "queueRemoved", fileIds: ids });
        });
      },
      async removeItem(fileId) {
        await guard(async () => {
          await api.queueRemove(fileId);
          dispatch({ type: "queueRemoved", fileIds: [fileId] });
        });
      },
      retry: (fileId) => guard(() => api.queueRetry(fileId).then(() => {})),
      async addAgainWith(fileId, modelId) {
        const item = stateRef.current.queue.find((q) => q.fileId === fileId);
        if (!item) return;
        await guard(async () => {
          await api.queueRemove(fileId);
          dispatch({ type: "queueRemoved", fileIds: [fileId] });
          const added = await api.queueAdd([item.path]);
          dispatch({ type: "queueAdded", items: added });
          dispatch({ type: "choice", patch: { modelId } });
        });
      },
      setTextMode: (fileId, mode) =>
        guard(async () => {
          await api.queueSetTextMode(fileId, mode);
          await refreshQueue();
          if (mode === "existingText") toast(t("toast.usedText"));
        }),
      async openOutput(path) {
        try {
          await api.openOutput(path);
          return "opened";
        } catch (error) {
          if (describe(error)?.reason === "file_missing") return "missing";
          fail(t("error.openFailed"));
          return "failed";
        }
      },
      async saveCopy(fileId, format) {
        const item = stateRef.current.queue.find((q) => q.fileId === fileId);
        if (!item) return false;
        const extension = format === "docx" ? "docx" : "md";
        const sep = item.path.includes("/") ? "/" : "\\";
        const defaultPath = `${parentDir(item.path)}${sep}${baseName(item.name)}.${extension}`;
        let chosen: string | null;
        try {
          chosen = await platform.pickSavePath({
            defaultPath,
            extension,
            filterName: t(format === "docx" ? "format.word" : "format.markdown"),
          });
        } catch {
          fail(t("error.pickerFailed"));
          return false;
        }
        if (!chosen) return false;
        const dest = chosen.toLowerCase().endsWith(`.${extension}`)
          ? chosen
          : `${chosen}.${extension}`;
        try {
          // The save dialog has already asked about replacing an existing file.
          const output = await api.exportTo(fileId, format, dest, true);
          const name = fileName(output.path);
          const folder = parentDir(output.path);
          toast(
            folder === parentDir(item.path)
              ? t("review.exported", { name })
              : t("review.exportedIn", { name, folder }),
          );
          return true;
        } catch (error) {
          fail(
            describe(error)?.code === "diskFull"
              ? t("error.exportDiskFull")
              : t("error.exportFailed"),
          );
          return false;
        }
      },
      reveal: (path) => guard(() => api.revealInFolder(path).then(() => {})),
      async pickFolder(defaultPath) {
        try {
          return await platform.pickFolder(defaultPath);
        } catch {
          fail(t("error.pickerFailed"));
          return null;
        }
      },
      async chooseSaveFolder() {
        const folder = await a.pickFolder();
        if (folder)
          dispatch({ type: "choice", patch: { saveTo: { kind: "folder", path: folder } } });
      },
      async updateSettings(patch) {
        try {
          const settings = await api.settingsSet(patch);
          dispatch({ type: "settingsLoaded", settings });
          return true;
        } catch {
          fail(t("error.setting.invalid"));
          return false;
        }
      },
      async refreshModels() {
        const models = await api.modelsList();
        dispatch({ type: "modelsLoaded", models });
        return models;
      },
      installModel: (modelId) =>
        guard(
          () => api.modelsInstall(modelId).then(() => {}),
          () => t("settings.models.installFailed"),
        ),
      removeModel: (modelId) =>
        guard(
          async () => {
            await api.modelsRemove(modelId);
            await a.refreshModels();
          },
          () => t("settings.models.removeFailed"),
        ),
      async importModel(modelId) {
        const folder = await a.pickFolder();
        if (!folder) return;
        await guard(
          () => api.modelsImport(modelId, folder).then(() => {}),
          (e) =>
            e?.code === "checksumMismatch"
              ? t("settings.models.importFailedChecksum")
              : t("settings.models.importFailed"),
        );
      },
      async startDownload(modelIds) {
        dispatch({ type: "setupStarted", modelIds });
        await guard(() => api.startSetupDownload(modelIds).then(() => {}));
      },
      pauseDownload: () => guard(() => api.setupPause().then(() => {})),
      resumeDownload: () => guard(() => api.setupResume().then(() => {})),
      async changeModelsDir() {
        const folder = await a.pickFolder(stateRef.current.settings?.modelsDir);
        if (!folder) return false;
        try {
          const settings = await api.settingsSet({ modelsDir: folder });
          dispatch({ type: "settingsLoaded", settings });
          return true;
        } catch {
          fail(t("setup.folderChangeFailed"));
          return false;
        }
      },
      finishSetup: () => dispatch({ type: "finishSetup" }),
      openReview: (fileId) => dispatch({ type: "openReview", fileId }),
      closeReview: () => dispatch({ type: "closeReview" }),
      openSettings: (section) => dispatch({ type: "openSettings", section }),
      setStep: (step) => dispatch({ type: "setupStep", step }),
    };
    return a;
  }, [api, platform]);

  const value = useMemo(
    () => ({ state, dispatch, api, platform, dev, actions }),
    [state, api, platform, dev, actions],
  );
  return <AppContext.Provider value={value}>{children}</AppContext.Provider>;
}

/** Opens the screen a URL parameter asks for. Only used with the mock, for screenshots. */
function applyDevParams(
  dev: DevParams,
  dispatch: Dispatch<Action>,
  queue: AppState["queue"],
): void {
  const byName = dev.file
    ? queue.find((q) => baseName(q.name) === dev.file || q.name === dev.file)
    : null;
  if (dev.step && dev.step >= 1 && dev.step <= 4) {
    dispatch({ type: "setupStep", step: dev.step as SetupUi["step"] });
  }
  switch (dev.screen) {
    case "settings": {
      const sections = ["general", "models", "privacy", "about"];
      const section = sections.includes(dev.section ?? "")
        ? (dev.section as SettingsSection)
        : undefined;
      dispatch({ type: "openSettings", section });
      break;
    }
    case "review": {
      const target = byName ?? queue.find((q) => q.status === "done");
      if (target) dispatch({ type: "openReview", fileId: target.fileId });
      break;
    }
    case "main":
      if (byName) dispatch({ type: "select", fileId: byName.fileId });
      break;
    default:
      break;
  }
}
