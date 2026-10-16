// Types for the Tauri command and event contract. Hand-written to match docs/protocol.md (Part
// 2) and src-tauri/src/contract.rs; src/types/contract.test.ts checks them against the JSON
// examples in docs/protocol-examples.json. The IR types come from ./ir, which is generated.
//
// Conventions: JSON keys are camelCase, enum values are camelCase strings, optional values are
// `null` rather than missing, and every file path is an absolute path in the native OS format.

import type { EngineInfo, Page } from "./ir";

export type { Block, BlockType, Document, EngineInfo, Page } from "./ir";

// ---------------------------------------------------------------------------------------------
// Errors

/** What a rejected command promise carries. The UI picks its copy from `code` (and `reason`). */
export type AppErrorCode =
  | "invalidArgument" // a command argument was malformed or out of range
  | "notFound" // unknown file id, model id or page
  | "badInput" // the file cannot be converted; `reason` says why (see below)
  | "modelNotReady" // model files missing or the model failed to load
  | "outOfMemory" // GPU or system memory ran out; suggest a smaller model
  | "llamaServerUnreachable"
  | "recognitionFailed" // the model produced unusable output for a page
  | "engineUnavailable" // the engine process is not running and could not be restarted
  | "cancelled"
  | "offline" // a download could not reach the network
  | "downloadFailed" // any other download failure
  | "checksumMismatch" // a downloaded or imported file does not match the manifest
  | "diskFull"
  | "io" // any other file system failure
  | "internal";

export interface AppError {
  code: AppErrorCode;
  /** Short English text for logs and as a last-resort fallback. Not meant to be shown as is. */
  message: string;
  /**
   * Narrows `code`. For `badInput`: `file_not_found`, `unreadable`, `corrupt`,
   * `password_protected`, `unsupported_format`, `page_out_of_range`. Otherwise a free-form slug
   * or null.
   */
  reason: string | null;
  /** Technical detail for the log and the diagnostics text. Never shown as the message. */
  detail: string | null;
}

// ---------------------------------------------------------------------------------------------
// Hardware, recommendation and models

export type Tier = "fast" | "accurate" | "max";
export type GpuVendor = "nvidia" | "amd" | "intel" | "apple" | "other";
/** The llama.cpp runtime build family a machine can use. `cpu` also covers the Fast tier. */
export type Backend = "cuda" | "vulkan" | "metal" | "cpu";

export interface GpuInfo {
  vendor: GpuVendor;
  name: string;
  /** Dedicated video memory in MB. null for Apple GPUs (see unifiedMemoryMB) or when unknown. */
  vramMB: number | null;
}

export interface HardwareInfo {
  os: "windows" | "macos";
  gpus: GpuInfo[];
  /** The best backend this machine can run, as the manifest rules see it. */
  backend: Backend;
  ramMB: number;
  /** Apple Silicon unified memory in MB, null elsewhere. */
  unifiedMemoryMB: number | null;
  cpuName: string;
  cpuCores: number;
  /** The folder whose drive `freeDiskMB` was measured on (the models folder). */
  modelsDir: string;
  freeDiskMB: number;
  /** NVIDIA driver version string, null when there is no NVIDIA GPU. */
  cudaDriverVersion: string | null;
}

/** The manifest's fit badges. `hidden` means "do not list this model". */
export type FitBadge = "runsWell" | "slow" | "wontFit" | "hidden";
/** Where a speed figure comes from. `estimated` means unmeasured: show it as "about". */
export type SpeedBasis = "measured" | "estimated" | "calibrated";

export interface SpeedEstimate {
  secondsPerPage: number;
  basis: SpeedBasis;
}

export interface ModelFit {
  modelId: string;
  badge: FitBadge;
  speed: SpeedEstimate | null;
}

export interface Recommendation {
  /** The suggester's pick. At most one model carries the Recommended tag. */
  modelId: string;
  /** Plain-language reason with the placeholders filled in. */
  reason: string;
  fits: ModelFit[];
  /** Download size in bytes of the llama-server build and the engine environment (estimate). */
  runtimeDownloadBytes: number;
}

export type ModelState = "notInstalled" | "downloading" | "installed";

export interface ModelInfo {
  /** The manifest model key, for example `accurate-paddleocr-vl-1.6`. */
  id: string;
  tier: Tier;
  displayName: string;
  /** The engine id that runs this model (the manifest `engine` field). */
  engineId: string;
  experimental: boolean;
  license: string;
  downloadSizeBytes: number;
  /** Size on disk when installed, else null. */
  installedSizeBytes: number | null;
  state: ModelState;
  /** The model's public page, for the "Source" link. */
  sourceUrl: string | null;
}

// ---------------------------------------------------------------------------------------------
// Setup and downloads

export type SetupPhase =
  | "notStarted" // no model installed and nothing downloading
  | "downloading"
  | "paused"
  | "calibrating"
  | "ready" // at least one model installed and its speed measured
  | "failed";

export type ComponentKind = "runtime" | "engine" | "model";
export type ComponentState = "pending" | "downloading" | "verifying" | "done" | "failed";

export interface SetupComponent {
  /** `llama-server`, `engine`, or a model id. */
  id: string;
  kind: ComponentKind;
  name: string;
  state: ComponentState;
  bytesDone: number;
  /** 0 when the total is unknown (progress is then indeterminate). */
  bytesTotal: number;
  bytesPerSecond: number | null;
  secondsLeft: number | null;
}

export interface Calibration {
  modelId: string;
  secondsPerPage: number;
}

export interface SetupStatus {
  phase: SetupPhase;
  modelsDir: string;
  /** Every runtime, engine and model download of the current or last run, in display order. */
  components: SetupComponent[];
  /** Result of the most recent calibration run. */
  calibration: Calibration | null;
  /** Set when phase is `failed`. */
  error: AppError | null;
}

// ---------------------------------------------------------------------------------------------
// Settings

export type ExportFormat = "docx" | "markdown";
export type Theme = "system" | "light" | "dark";

export type SaveLocation = { kind: "original" } | { kind: "folder"; path: string };

export interface Settings {
  defaultFormats: ExportFormat[];
  /** null means "use the recommendation". */
  defaultModelId: string | null;
  saveTo: SaveLocation;
  theme: Theme;
  checkForUpdates: boolean;
  /** Changing it is only accepted while no model is installed or downloading. */
  modelsDir: string;
  /** Opt-in experimental Vulkan runtime for AMD and Intel GPUs on Windows. */
  experimentalVulkan: boolean;
}

/** Only the fields to change. A missing field and a null both mean "leave as is". */
export type SettingsPatch = Partial<Settings>;

// ---------------------------------------------------------------------------------------------
// Queue

export type FileKind = "pdf" | "image";

export type QueueStatus =
  | "waiting" // added, or started and not reached yet
  | "converting"
  | "paused"
  | "needsAttention"
  | "done"
  | "failed"
  | "cancelled";

/** Why an item is `needsAttention`. */
export type AttentionReason = "hasTextLayer";

/** How a PDF with a text layer is read. null until the user decides. */
export type TextMode = "ocr" | "existingText";

/** The step an item is in. `render` and `export` run in the Rust core, the rest in the engine. */
export type Stage =
  "render" | "prepare" | "layout" | "recognize" | "normalize" | "crops" | "export";

export interface OutputFile {
  format: ExportFormat;
  path: string;
  /** Markdown only: the folder holding the figure files, if there are any. */
  assetsDir: string | null;
}

export interface QueueItem {
  fileId: string;
  path: string;
  name: string;
  kind: FileKind;
  sizeBytes: number;
  /** null until the file has been inspected. */
  pageCount: number | null;
  /** PDFs only: at least half of the pages have a usable text layer. null until inspected. */
  hasTextLayer: boolean | null;
  /** First page image for the detail view, once rendered. */
  previewPath: string | null;
  status: QueueStatus;
  attention: AttentionReason | null;
  textMode: TextMode | null;
  /** Set when the item is started. */
  modelId: string | null;
  formats: ExportFormat[];
  pagesDone: number;
  /** 0-based page being processed, null when none is. */
  currentPage: number | null;
  stage: Stage | null;
  secondsLeft: number | null;
  /** Wall-clock time of the finished conversion, for "Finished in 14 s". */
  durationMs: number | null;
  outputs: OutputFile[];
  /** Set when status is `failed`. */
  error: AppError | null;
}

export type PageState = "pending" | "converting" | "done" | "failed";

export interface PageStatus {
  index: number;
  state: PageState;
  /** The rendered page image, once it exists. */
  imagePath: string | null;
  error: AppError | null;
}

export interface ConvertOptions {
  /** At least one. */
  formats: ExportFormat[];
  modelId: string;
  saveTo: SaveLocation;
}

/** Payload of the `queue://progress` event. */
export interface QueueProgress {
  /** The item's full current state. Replace the item with the same fileId. */
  item: QueueItem;
  /** The page whose state just changed, null for item-level changes. */
  page: PageStatus | null;
}

/** Result of `get_review`. */
export interface ReviewPage {
  fileId: string;
  pageIndex: number;
  pageCount: number;
  /** Same as `page.sourceImage`. */
  imagePath: string;
  /** The IR page, read from the job's page file. */
  page: Page;
  /** Which engine and model produced it, for "Result . Accurate (PaddleOCR-VL 1.6)". */
  engine: EngineInfo;
}

// ---------------------------------------------------------------------------------------------
// Commands and events

export const EVENT_QUEUE_PROGRESS = "queue://progress";
export const EVENT_SETUP_PROGRESS = "setup://progress";

/** Event name to payload type. `setup://progress` carries a full SetupStatus snapshot. */
export interface EventPayloads {
  "queue://progress": QueueProgress;
  "setup://progress": SetupStatus;
}

/**
 * Command name to argument object (camelCase, as passed to `invoke`) and resolved value. Every
 * command rejects with an AppError. Commands marked long-running resolve when the work has been
 * accepted; the outcome arrives on the event stream named in the docs.
 */
export interface Commands {
  probe_hardware: { args: Record<string, never>; result: HardwareInfo };
  get_recommendation: { args: Record<string, never>; result: Recommendation };
  start_setup_download: { args: { modelIds: string[] }; result: null };
  setup_pause: { args: Record<string, never>; result: null };
  setup_resume: { args: Record<string, never>; result: null };
  setup_status: { args: Record<string, never>; result: SetupStatus };

  queue_add: { args: { paths: string[] }; result: QueueItem[] };
  queue_list: { args: Record<string, never>; result: QueueItem[] };
  queue_start: { args: { options: ConvertOptions }; result: null };
  queue_pause: { args: Record<string, never>; result: null };
  queue_resume: { args: Record<string, never>; result: null };
  queue_cancel: { args: { fileId: string | null }; result: null };
  queue_remove: { args: { fileId: string }; result: null };
  queue_clear_done: { args: Record<string, never>; result: string[] };
  queue_retry: { args: { fileId: string }; result: null };
  queue_set_text_mode: { args: { fileId: string; mode: TextMode }; result: null };
  get_pages: { args: { fileId: string }; result: PageStatus[] };
  get_review: { args: { fileId: string; page: number }; result: ReviewPage };
  export: { args: { fileId: string; format: ExportFormat }; result: OutputFile };
  reveal_in_folder: { args: { path: string }; result: null };

  settings_get: { args: Record<string, never>; result: Settings };
  settings_set: { args: { patch: SettingsPatch }; result: Settings };

  models_list: { args: Record<string, never>; result: ModelInfo[] };
  models_install: { args: { modelId: string }; result: null };
  models_remove: { args: { modelId: string }; result: null };
  models_import: { args: { modelId: string; folder: string }; result: null };

  app_version: { args: Record<string, never>; result: string };
  get_diagnostics: { args: Record<string, never>; result: string };
  open_logs_folder: { args: Record<string, never>; result: null };
}

export type CommandName = keyof Commands;
