import type {
  ExportFormat,
  ModelInfo,
  PageStatus,
  QueueItem,
  QueueProgress,
  SaveLocation,
  Settings,
  SetupStatus,
} from "../types/commands";

export type Screen = "setup" | "main" | "settings";
export type SettingsSection = "general" | "models" | "privacy" | "about";

export interface Toast {
  id: number;
  text: string;
}

/** What the user has chosen for the next batch (the action bar). */
export interface ConvertChoice {
  formats: ExportFormat[];
  /** null until the first installed model is known. */
  modelId: string | null;
  saveTo: SaveLocation;
}

export interface SetupUi {
  step: 1 | 2 | 3 | 4;
  /** Model ids ticked in step 3, null until the recommendation has been applied. */
  selected: string[] | null;
  advancedOpen: boolean;
  /** The ids sent to start_setup_download, kept for "Retry". */
  started: string[];
}

export interface AppState {
  boot: "loading" | "ready" | "failed";
  screen: Screen;
  /** Where Back in Settings goes. */
  settingsReturn: "main" | "setup";
  settingsSection: SettingsSection;
  setup: SetupStatus | null;
  settings: Settings | null;
  models: ModelInfo[];
  queue: QueueItem[];
  /** Per-page status for the files whose pages were loaded or reported by an event. */
  pages: Record<string, PageStatus[]>;
  selectedId: string | null;
  /** The file open in Review, null on the Main detail view. */
  reviewId: string | null;
  setupUi: SetupUi;
  choice: ConvertChoice;
  toasts: Toast[];
  /** A problem the user should see that is not tied to one file. */
  banner: string | null;
}

export const initialState: AppState = {
  boot: "loading",
  screen: "main",
  settingsReturn: "main",
  settingsSection: "general",
  setup: null,
  settings: null,
  models: [],
  queue: [],
  pages: {},
  selectedId: null,
  reviewId: null,
  setupUi: { step: 1, selected: null, advancedOpen: false, started: [] },
  choice: { formats: ["docx", "markdown"], modelId: null, saveTo: { kind: "original" } },
  toasts: [],
  banner: null,
};

export type Action =
  | {
      type: "booted";
      setup: SetupStatus;
      settings: Settings;
      models: ModelInfo[];
      queue: QueueItem[];
    }
  | { type: "bootFailed" }
  | { type: "setupProgress"; status: SetupStatus }
  | { type: "modelsLoaded"; models: ModelInfo[] }
  | { type: "settingsLoaded"; settings: Settings }
  | { type: "queueLoaded"; queue: QueueItem[] }
  | { type: "queueProgress"; progress: QueueProgress }
  | { type: "queueAdded"; items: QueueItem[] }
  | { type: "queueRemoved"; fileIds: string[] }
  | { type: "pagesLoaded"; fileId: string; pages: PageStatus[] }
  | { type: "select"; fileId: string | null }
  | { type: "openReview"; fileId: string }
  | { type: "closeReview" }
  | { type: "openSettings"; section?: SettingsSection }
  | { type: "closeSettings" }
  | { type: "settingsSection"; section: SettingsSection }
  | { type: "finishSetup" }
  | { type: "setupStep"; step: SetupUi["step"] }
  | { type: "setupSelect"; selected: string[] }
  | { type: "setupAdvanced"; open: boolean }
  | { type: "setupStarted"; modelIds: string[] }
  | { type: "choice"; patch: Partial<ConvertChoice> }
  | { type: "toast"; id: number; text: string }
  | { type: "dismissToast"; id: number }
  | { type: "banner"; text: string | null };

function upsert(queue: QueueItem[], item: QueueItem): QueueItem[] {
  const i = queue.findIndex((q) => q.fileId === item.fileId);
  if (i < 0) return [...queue, item];
  const next = queue.slice();
  next[i] = item;
  return next;
}

function mergePage(pages: PageStatus[] | undefined, page: PageStatus): PageStatus[] | undefined {
  if (!pages) return pages;
  const next = pages.slice();
  next[page.index] = page;
  return next;
}

/** The step Setup opens on: 4 when a download has already started, 1 on a fresh install. */
export function setupStepFor(status: SetupStatus): SetupUi["step"] {
  return status.phase === "notStarted" ? 1 : 4;
}

export function reducer(state: AppState, action: Action): AppState {
  switch (action.type) {
    case "booted": {
      const ready = action.setup.phase === "ready";
      const settings = action.settings;
      return {
        ...state,
        boot: "ready",
        screen: ready ? "main" : "setup",
        setup: action.setup,
        settings,
        models: action.models,
        queue: action.queue,
        selectedId: state.selectedId ?? action.queue[0]?.fileId ?? null,
        setupUi: { ...state.setupUi, step: setupStepFor(action.setup) },
        choice: { ...state.choice, formats: settings.defaultFormats, saveTo: settings.saveTo },
      };
    }
    case "bootFailed":
      return { ...state, boot: "failed" };
    case "setupProgress":
      return { ...state, setup: action.status };
    case "modelsLoaded":
      return { ...state, models: action.models };
    case "settingsLoaded":
      return { ...state, settings: action.settings };
    case "queueLoaded":
      return {
        ...state,
        queue: action.queue,
        selectedId: action.queue.some((q) => q.fileId === state.selectedId)
          ? state.selectedId
          : (action.queue[0]?.fileId ?? null),
      };
    case "queueProgress": {
      const { item, page } = action.progress;
      let pages = state.pages;
      if (page) {
        const merged = mergePage(pages[item.fileId], page);
        if (merged) pages = { ...pages, [item.fileId]: merged };
      }
      return {
        ...state,
        queue: upsert(state.queue, item),
        pages,
        selectedId: state.selectedId ?? item.fileId,
      };
    }
    case "queueAdded": {
      let queue = state.queue;
      for (const item of action.items) queue = upsert(queue, item);
      return {
        ...state,
        queue,
        selectedId: action.items[0]?.fileId ?? state.selectedId,
        reviewId: null,
      };
    }
    case "queueRemoved": {
      const gone = new Set(action.fileIds);
      const queue = state.queue.filter((q) => !gone.has(q.fileId));
      const pages = { ...state.pages };
      gone.forEach((id) => delete pages[id]);
      const selectedId =
        state.selectedId && gone.has(state.selectedId)
          ? (queue[0]?.fileId ?? null)
          : state.selectedId;
      const reviewId = state.reviewId && gone.has(state.reviewId) ? null : state.reviewId;
      return { ...state, queue, pages, selectedId, reviewId };
    }
    case "pagesLoaded":
      return { ...state, pages: { ...state.pages, [action.fileId]: action.pages } };
    case "select": {
      // Picking a file that is not done leaves Review; picking a done file keeps it open on that file.
      const target = state.queue.find((q) => q.fileId === action.fileId);
      const keepReview = state.reviewId !== null && target?.status === "done";
      return {
        ...state,
        selectedId: action.fileId,
        reviewId: keepReview ? action.fileId : null,
      };
    }
    case "openReview":
      return { ...state, selectedId: action.fileId, reviewId: action.fileId, screen: "main" };
    case "closeReview":
      return { ...state, reviewId: null };
    case "openSettings":
      return {
        ...state,
        screen: "settings",
        settingsReturn: state.screen === "setup" ? "setup" : "main",
        settingsSection: action.section ?? state.settingsSection,
      };
    case "closeSettings":
      return { ...state, screen: state.settingsReturn };
    case "settingsSection":
      return { ...state, settingsSection: action.section };
    case "finishSetup":
      return { ...state, screen: "main" };
    case "setupStep":
      return { ...state, setupUi: { ...state.setupUi, step: action.step } };
    case "setupSelect":
      return { ...state, setupUi: { ...state.setupUi, selected: action.selected } };
    case "setupAdvanced":
      return { ...state, setupUi: { ...state.setupUi, advancedOpen: action.open } };
    case "setupStarted":
      return { ...state, setupUi: { ...state.setupUi, started: action.modelIds } };
    case "choice":
      return { ...state, choice: { ...state.choice, ...action.patch } };
    case "toast":
      return { ...state, toasts: [...state.toasts, { id: action.id, text: action.text }] };
    case "dismissToast":
      return { ...state, toasts: state.toasts.filter((x) => x.id !== action.id) };
    case "banner":
      return { ...state, banner: action.text };
  }
}
