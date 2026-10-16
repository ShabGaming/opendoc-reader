import type {
  CommandName,
  Commands,
  ConvertOptions,
  EventPayloads,
  ExportFormat,
  QueueProgress,
  SettingsPatch,
  SetupStatus,
  TextMode,
} from "../types/commands";
import { EVENT_QUEUE_PROGRESS, EVENT_SETUP_PROGRESS } from "../types/commands";

/**
 * The two things the Rust core offers: commands that resolve or reject with an AppError, and
 * events. The real transport is Tauri, the mock one lives in ./mock.
 */
export interface Transport {
  invoke<C extends CommandName>(
    command: C,
    args: Commands[C]["args"],
  ): Promise<Commands[C]["result"]>;
  listen<E extends keyof EventPayloads>(
    event: E,
    handler: (payload: EventPayloads[E]) => void,
  ): Promise<() => void>;
}

const none = {} as Record<string, never>;

/** One typed method per command in docs/protocol.md, Part 2. */
export function createApi(transport: Transport) {
  const call = transport.invoke.bind(transport);
  return {
    probeHardware: () => call("probe_hardware", none),
    getRecommendation: () => call("get_recommendation", none),
    startSetupDownload: (modelIds: string[]) => call("start_setup_download", { modelIds }),
    setupPause: () => call("setup_pause", none),
    setupResume: () => call("setup_resume", none),
    setupStatus: () => call("setup_status", none),

    queueAdd: (paths: string[]) => call("queue_add", { paths }),
    queueList: () => call("queue_list", none),
    queueStart: (options: ConvertOptions) => call("queue_start", { options }),
    queuePause: () => call("queue_pause", none),
    queueResume: () => call("queue_resume", none),
    queueCancel: (fileId: string | null) => call("queue_cancel", { fileId }),
    queueRemove: (fileId: string) => call("queue_remove", { fileId }),
    queueClearDone: () => call("queue_clear_done", none),
    queueRetry: (fileId: string) => call("queue_retry", { fileId }),
    queueSetTextMode: (fileId: string, mode: TextMode) =>
      call("queue_set_text_mode", { fileId, mode }),
    getPages: (fileId: string) => call("get_pages", { fileId }),
    getReview: (fileId: string, page: number) => call("get_review", { fileId, page }),
    exportFile: (fileId: string, format: ExportFormat) => call("export", { fileId, format }),
    revealInFolder: (path: string) => call("reveal_in_folder", { path }),

    settingsGet: () => call("settings_get", none),
    settingsSet: (patch: SettingsPatch) => call("settings_set", { patch }),

    modelsList: () => call("models_list", none),
    modelsInstall: (modelId: string) => call("models_install", { modelId }),
    modelsRemove: (modelId: string) => call("models_remove", { modelId }),
    modelsImport: (modelId: string, folder: string) => call("models_import", { modelId, folder }),

    appVersion: () => call("app_version", none),
    getDiagnostics: () => call("get_diagnostics", none),
    openLogsFolder: () => call("open_logs_folder", none),

    onQueueProgress: (handler: (payload: QueueProgress) => void) =>
      transport.listen(EVENT_QUEUE_PROGRESS, handler),
    onSetupProgress: (handler: (payload: SetupStatus) => void) =>
      transport.listen(EVENT_SETUP_PROGRESS, handler),
  };
}

export type Api = ReturnType<typeof createApi>;
