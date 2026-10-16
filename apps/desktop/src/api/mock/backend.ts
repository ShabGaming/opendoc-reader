import type {
  AppError,
  AppErrorCode,
  CommandName,
  Commands,
  ConvertOptions,
  EventPayloads,
  ExportFormat,
  ModelInfo,
  OutputFile,
  PageStatus,
  QueueItem,
  ReviewPage,
  SaveLocation,
  Settings,
  SetupComponent,
  SetupStatus,
  Stage,
} from "../../types/commands";
import type { Transport } from "../api";
import { FILE_EXTENSIONS } from "../platform";
import { baseName, fileName, parentDir } from "../../util/format";
import {
  ACCURATE_ID,
  ENGINE_BYTES,
  MODEL_CATALOG,
  RUNTIME_BYTES,
  hardwareFor,
  recommendationFor,
  type HardwareProfile,
} from "./catalog";
import { FIXTURE_PAGE_COUNT, MOCK_ENGINE, fixturePage, pageImageUrl } from "./fixtures";

export type MockScenario = "fresh" | "downloading" | "ready-empty" | "demo";

export interface MockOptions {
  scenario?: MockScenario;
  hardware?: HardwareProfile;
  /** Milliseconds a command takes to answer. */
  latencyMs?: number;
  /** Milliseconds of mock time one page takes to convert. */
  pageMs?: number;
  /** Fail the first setup download with this error, then behave. */
  failDownload?: AppErrorCode;
  theme?: Settings["theme"];
}

const STAGES: Stage[] = ["render", "prepare", "layout", "recognize", "normalize", "crops"];
const SECONDS_PER_PAGE = 4.8;
const CALIBRATION_MS = 1800;
const VERIFY_MS = 400;
const EXPORT_MS = 500;
const INSPECT_MS = 500;
const SETUP_EMIT_MS = 250;
const INTERNAL_RUNTIME_ID = "llama-server";
const INTERNAL_ENGINE_ID = "engine";

interface Component extends SetupComponent {
  rate: number;
  verifyLeft: number;
}

interface Job {
  /** Milliseconds spent on the page in flight. */
  pageElapsed: number;
  /** Milliseconds left of the final export step, null while pages remain. */
  exportLeft: number | null;
}

export function appError(code: AppErrorCode, reason: string | null = null): AppError {
  return { code, message: errorMessages[code] ?? "The command failed.", reason, detail: null };
}

const errorMessages: Partial<Record<AppErrorCode, string>> = {
  invalidArgument: "An argument was not valid.",
  notFound: "Not found.",
  badInput: "The file cannot be converted.",
  modelNotReady: "The model is not installed.",
  outOfMemory: "Out of memory.",
  recognitionFailed: "Recognition failed.",
  offline: "No network connection.",
  downloadFailed: "The download failed.",
  checksumMismatch: "A file does not match its checksum.",
  diskFull: "The disk is full.",
};

function hash(text: string): number {
  let h = 0;
  for (const ch of text) h = (h * 31 + ch.charCodeAt(0)) >>> 0;
  return h;
}

/**
 * A stand-in for the Rust core with the same commands and events. Time only moves when
 * `advance` is called, which `start` does on a timer. Tests call `advance` themselves.
 *
 * File names steer a few failures: "corrupt" and "password" fail when inspected, "huge" runs
 * out of memory on its second page the first time it is converted, "text" and "invoice"
 * mark a PDF as having a text layer.
 */
export class MockBackend implements Transport {
  readonly hardwareProfile: HardwareProfile;
  /** Every path passed to reveal_in_folder, newest last. */
  readonly revealed: string[] = [];
  /** Every path passed to open_output, newest last. */
  readonly openedOutputs: string[] = [];
  /** Every copy written by export_to, newest last. */
  readonly savedCopies: Array<{ fileId: string; format: ExportFormat; path: string }> = [];
  /** Every command call in order, for tests. */
  readonly calls: Array<{ command: string; args: unknown }> = [];

  private readonly latencyMs: number;
  private readonly pageMs: number;
  private failDownload: AppErrorCode | null;
  private listeners = new Map<string, Set<(payload: never) => void>>();
  private timer: ReturnType<typeof setInterval> | null = null;

  private phase: SetupStatus["phase"] = "notStarted";
  private components: Component[] = [];
  private calibration: SetupStatus["calibration"] = null;
  private setupError: AppError | null = null;
  private installed = new Set<string>();
  private pendingModels: string[] = [];
  private calibrationLeft = 0;
  private sinceSetupEmit = 0;

  private settings: Settings;
  private queue: QueueItem[] = [];
  private jobs = new Map<string, Job>();
  private inspectLeft = new Map<string, number>();
  private oomOnce = new Set<string>();
  private paused = false;
  private usedNames = new Set<string>();
  /** Output files the user "moved or deleted" (see markOutputsGone). */
  private gone = new Set<string>();
  private saveTo = new Map<string, SaveLocation>();
  private idCounter = 0;

  constructor(options: MockOptions = {}) {
    this.hardwareProfile = options.hardware ?? "gpu";
    this.latencyMs = options.latencyMs ?? 0;
    this.pageMs = options.pageMs ?? 1000;
    this.failDownload = options.failDownload ?? null;
    this.settings = {
      defaultFormats: ["docx", "markdown"],
      defaultModelId: null,
      saveTo: { kind: "original" },
      theme: options.theme ?? "system",
      checkForUpdates: true,
      modelsDir: hardwareFor(this.hardwareProfile).modelsDir,
      experimentalVulkan: false,
    };
    this.seed(options.scenario ?? "demo");
  }

  // -------------------------------------------------------------------------------------------
  // Transport

  async invoke<C extends CommandName>(
    command: C,
    args: Commands[C]["args"],
  ): Promise<Commands[C]["result"]> {
    this.calls.push({ command, args });
    if (this.latencyMs > 0) await new Promise((r) => setTimeout(r, this.latencyMs));
    const handler = this.handlers[command] as (a: Commands[C]["args"]) => Commands[C]["result"];
    return structuredClone(handler(args));
  }

  async listen<E extends keyof EventPayloads>(
    event: E,
    handler: (payload: EventPayloads[E]) => void,
  ): Promise<() => void> {
    let set = this.listeners.get(event);
    if (!set) this.listeners.set(event, (set = new Set()));
    const fn = handler as (payload: never) => void;
    set.add(fn);
    return () => set.delete(fn);
  }

  private emit<E extends keyof EventPayloads>(event: E, payload: EventPayloads[E]) {
    this.listeners.get(event)?.forEach((fn) => fn(structuredClone(payload) as never));
  }

  // -------------------------------------------------------------------------------------------
  // Clock

  /** Runs the clock in real time, `tickMs` of mock time every `tickMs`. */
  start(tickMs = 100): void {
    this.stop();
    this.timer = setInterval(() => this.advance(tickMs), tickMs);
  }

  stop(): void {
    if (this.timer) clearInterval(this.timer);
    this.timer = null;
  }

  /** Moves mock time forward in steps of at most 100 ms. */
  advance(ms: number): void {
    let left = ms;
    while (left > 0) {
      const step = Math.min(left, 100);
      left -= step;
      this.advanceSetup(step);
      this.advanceQueue(step);
    }
  }

  // -------------------------------------------------------------------------------------------
  // Scenarios

  private seed(scenario: MockScenario): void {
    if (scenario === "fresh") return;
    if (scenario === "downloading") {
      this.beginDownload([ACCURATE_ID]);
      return;
    }
    this.installed.add(ACCURATE_ID);
    this.phase = "ready";
    this.components = [
      this.makeComponent(INTERNAL_RUNTIME_ID, "runtime", "llama-server (CUDA)", RUNTIME_BYTES, 1),
      this.makeComponent(INTERNAL_ENGINE_ID, "engine", "Document engine", ENGINE_BYTES, 1),
      this.modelComponent(ACCURATE_ID, 1),
    ].map((c) => ({ ...c, state: "done" as const, bytesDone: c.bytesTotal }));
    this.calibration = { modelId: ACCURATE_ID, secondsPerPage: SECONDS_PER_PAGE };
    if (scenario === "demo") this.seedQueue();
  }

  private seedQueue(): void {
    const docs = "C:\\Users\\Demo\\Documents";
    const base = {
      attention: null,
      textMode: null,
      modelId: null,
      formats: ["docx", "markdown"] as ExportFormat[],
      pagesDone: 0,
      currentPage: null,
      stage: null,
      secondsLeft: null,
      durationMs: null,
      outputs: [] as OutputFile[],
      error: null,
    };
    const item = (over: Partial<QueueItem> & Pick<QueueItem, "name" | "kind" | "sizeBytes">) => {
      const full: QueueItem = {
        fileId: this.nextId(),
        path: `${docs}\\${over.name}`,
        pageCount: 1,
        hasTextLayer: false,
        previewPath: null,
        status: "waiting",
        ...base,
        ...over,
      };
      return full;
    };
    const done = item({
      name: "Library scans.pdf",
      kind: "pdf",
      sizeBytes: 2_400_000,
      pageCount: 3,
      previewPath: pageImageUrl(0),
      status: "done",
      modelId: ACCURATE_ID,
      pagesDone: 3,
      durationMs: 14_000,
    });
    done.outputs = [
      { format: "docx", path: `${docs}\\Library scans.docx`, assetsDir: null },
      {
        format: "markdown",
        path: `${docs}\\Library scans.md`,
        assetsDir: `${docs}\\Library scans_assets`,
      },
    ];
    done.outputs.forEach((o) => this.usedNames.add(o.path));
    const converting = item({
      name: "Statistical abstract 1920.pdf",
      kind: "pdf",
      sizeBytes: 41_800_000,
      pageCount: 48,
      previewPath: pageImageUrl(1),
      status: "converting",
      modelId: ACCURATE_ID,
      pagesDone: 30,
      currentPage: 30,
      stage: "recognize",
      secondsLeft: 18 * SECONDS_PER_PAGE,
    });
    this.jobs.set(converting.fileId, { pageElapsed: 0, exportLeft: null });
    const waiting = item({
      name: "calculus-notes-p77.jpg",
      kind: "image",
      sizeBytes: 412_000,
      previewPath: pageImageUrl(2),
    });
    const attention = item({
      name: "Invoice 2026-0143.pdf",
      kind: "pdf",
      sizeBytes: 96_000,
      pageCount: 2,
      hasTextLayer: true,
      previewPath: pageImageUrl(1),
      status: "needsAttention",
      attention: "hasTextLayer",
    });
    const receipt = item({ name: "receipt.heic", kind: "image", sizeBytes: 2_400_000 });
    const failed = item({
      name: "Large atlas.pdf",
      kind: "pdf",
      sizeBytes: 88_000_000,
      pageCount: 12,
      previewPath: pageImageUrl(0),
      status: "failed",
      modelId: ACCURATE_ID,
      pagesDone: 4,
      error: appError("outOfMemory", "gpu"),
    });
    this.queue = [done, converting, waiting, attention, receipt, failed];
  }

  // -------------------------------------------------------------------------------------------
  // Setup and downloads

  private makeComponent(
    id: string,
    kind: SetupComponent["kind"],
    name: string,
    bytesTotal: number,
    seconds: number,
  ): Component {
    return {
      id,
      kind,
      name,
      state: "pending",
      bytesDone: 0,
      bytesTotal,
      bytesPerSecond: null,
      secondsLeft: null,
      rate: bytesTotal / seconds,
      verifyLeft: VERIFY_MS,
    };
  }

  private modelComponent(modelId: string, scale = 1): Component {
    const model = MODEL_CATALOG.find((m) => m.id === modelId)!;
    // Larger models take longer, and the whole run takes about eight seconds.
    const seconds = (1.5 + (model.downloadSizeBytes / 3.6e9) * 5) * scale;
    return this.makeComponent(
      modelId,
      "model",
      model.displayName,
      model.downloadSizeBytes,
      seconds,
    );
  }

  private beginDownload(modelIds: string[]): void {
    const keep = this.components.filter((c) => c.state === "done");
    const have = new Set(keep.map((c) => c.id));
    const next: Component[] = [...keep];
    if (!have.has(INTERNAL_RUNTIME_ID)) {
      next.push(
        this.makeComponent(INTERNAL_RUNTIME_ID, "runtime", "llama-server (CUDA)", RUNTIME_BYTES, 2),
      );
    }
    if (!have.has(INTERNAL_ENGINE_ID)) {
      next.push(
        this.makeComponent(INTERNAL_ENGINE_ID, "engine", "Document engine", ENGINE_BYTES, 2.4),
      );
    }
    for (const id of modelIds)
      if (!have.has(id) && !this.installed.has(id)) next.push(this.modelComponent(id));
    this.components = next;
    this.pendingModels = modelIds;
    this.setupError = null;
    this.phase = "downloading";
    this.emitSetup();
  }

  private advanceSetup(ms: number): void {
    this.sinceSetupEmit += ms;
    if (this.phase === "calibrating") {
      this.calibrationLeft -= ms;
      if (this.calibrationLeft <= 0) {
        const modelId = this.pendingModels[0] ?? ACCURATE_ID;
        const fit = recommendationFor(hardwareFor(this.hardwareProfile)).fits.find(
          (f) => f.modelId === modelId,
        );
        this.calibration = {
          modelId,
          secondsPerPage: fit?.speed?.secondsPerPage ?? SECONDS_PER_PAGE,
        };
        this.phase = "ready";
        this.emitSetup();
      }
      return;
    }
    if (this.phase !== "downloading") return;

    const seconds = ms / 1000;
    for (const c of this.components) {
      if (c.state === "done" || c.state === "failed") continue;
      if (c.state === "verifying") {
        c.verifyLeft -= ms;
        if (c.verifyLeft <= 0) this.finishComponent(c);
        continue;
      }
      c.state = "downloading";
      c.bytesDone = Math.min(c.bytesTotal, c.bytesDone + c.rate * seconds);
      c.bytesPerSecond = c.rate;
      c.secondsLeft = (c.bytesTotal - c.bytesDone) / c.rate;
      if (this.failDownload && c.kind === "model" && c.bytesDone >= c.bytesTotal * 0.4) {
        this.failComponent(c, this.failDownload);
        return;
      }
      if (c.bytesDone >= c.bytesTotal) {
        c.state = "verifying";
        c.verifyLeft = VERIFY_MS;
        c.bytesPerSecond = null;
        c.secondsLeft = null;
      }
    }
    if (this.components.every((c) => c.state === "done")) {
      if (this.calibration) {
        this.phase = "ready";
      } else {
        this.phase = "calibrating";
        this.calibrationLeft = CALIBRATION_MS;
      }
      this.emitSetup();
    } else if (this.sinceSetupEmit >= SETUP_EMIT_MS) {
      this.emitSetup();
    }
  }

  private finishComponent(c: Component): void {
    c.state = "done";
    c.bytesDone = c.bytesTotal;
    c.bytesPerSecond = null;
    c.secondsLeft = null;
    if (c.kind === "model") this.installed.add(c.id);
  }

  private failComponent(c: Component, code: AppErrorCode): void {
    c.state = "failed";
    this.failDownload = null;
    for (const other of this.components) {
      other.bytesPerSecond = null;
      other.secondsLeft = null;
    }
    this.setupError = appError(code, null);
    this.phase = "failed";
    this.emitSetup();
  }

  private setupSnapshot(): SetupStatus {
    return {
      phase: this.phase,
      modelsDir: this.settings.modelsDir,
      components: this.components.map((c) => ({
        id: c.id,
        kind: c.kind,
        name: c.name,
        state: c.state,
        bytesDone: c.bytesDone,
        bytesTotal: c.bytesTotal,
        bytesPerSecond: c.bytesPerSecond,
        secondsLeft: c.secondsLeft,
      })),
      calibration: this.calibration,
      error: this.setupError,
    };
  }

  private emitSetup(): void {
    this.sinceSetupEmit = 0;
    this.emit("setup://progress", this.setupSnapshot());
  }

  private modelInfos(): ModelInfo[] {
    return MODEL_CATALOG.map((m) => {
      const component = this.components.find((c) => c.id === m.id);
      const installed = this.installed.has(m.id);
      const busy = !!component && component.state !== "done" && component.state !== "failed";
      return {
        ...m,
        state: installed ? "installed" : busy ? "downloading" : "notInstalled",
        installedSizeBytes: installed ? m.downloadSizeBytes : null,
      };
    });
  }

  // -------------------------------------------------------------------------------------------
  // Queue

  private nextId(): string {
    this.idCounter += 1;
    return `f-${String(this.idCounter).padStart(4, "0")}`;
  }

  private find(fileId: string): QueueItem {
    const item = this.queue.find((q) => q.fileId === fileId);
    if (!item) throw appError("notFound");
    return item;
  }

  private emitItem(item: QueueItem, page: PageStatus | null = null): void {
    this.emit("queue://progress", { item, page });
  }

  private pageStatus(item: QueueItem, index: number): PageStatus {
    const state =
      index < item.pagesDone ? "done" : item.currentPage === index ? "converting" : "pending";
    return {
      index,
      state,
      imagePath: state === "done" || index === 0 ? pageImageUrl(index % FIXTURE_PAGE_COUNT) : null,
      error: null,
    };
  }

  private expandPath(path: string): string[] {
    if (/\.[A-Za-z0-9]{1,5}$/.test(fileName(path))) return [path];
    // A folder: pretend it holds two scans.
    return [`${path}\\scan-1.png`, `${path}\\scan-2.pdf`];
  }

  private add(paths: string[]): QueueItem[] {
    const known = new Set(
      this.queue
        .filter((q) => q.status === "waiting" || q.status === "converting")
        .map((q) => q.path),
    );
    const added: QueueItem[] = [];
    for (const path of paths.flatMap((p) => this.expandPath(p))) {
      const ext = path.split(".").pop()!.toLowerCase();
      if (!FILE_EXTENSIONS.includes(ext) || known.has(path)) continue;
      known.add(path);
      const item: QueueItem = {
        fileId: this.nextId(),
        path,
        name: fileName(path),
        kind: ext === "pdf" ? "pdf" : "image",
        sizeBytes: 100_000 + (hash(path) % 4_000_000),
        pageCount: null,
        hasTextLayer: null,
        previewPath: null,
        status: "waiting",
        attention: null,
        textMode: null,
        modelId: null,
        formats: [],
        pagesDone: 0,
        currentPage: null,
        stage: null,
        secondsLeft: null,
        durationMs: null,
        outputs: [],
        error: null,
      };
      this.queue.push(item);
      this.inspectLeft.set(item.fileId, INSPECT_MS);
      added.push(item);
    }
    if (added.length === 0) throw appError("badInput", "unsupported_format");
    return added;
  }

  private inspect(item: QueueItem): void {
    const name = item.name.toLowerCase();
    if (name.includes("corrupt") || name.includes("password")) {
      item.status = "failed";
      item.error = appError(
        "badInput",
        name.includes("password") ? "password_protected" : "corrupt",
      );
      this.emitItem(item);
      return;
    }
    const h = hash(item.name);
    item.pageCount = item.kind === "pdf" ? 2 + (h % 11) : name.includes("multi") ? 3 : 1;
    item.hasTextLayer = item.kind === "pdf" ? /text|invoice|report/.test(name) : null;
    item.previewPath = pageImageUrl(h % FIXTURE_PAGE_COUNT);
    if (item.hasTextLayer) {
      item.status = "needsAttention";
      item.attention = "hasTextLayer";
    }
    this.emitItem(item);
  }

  private converting(): QueueItem | undefined {
    return this.queue.find((q) => q.status === "converting");
  }

  private startNext(): void {
    if (this.paused || this.converting()) return;
    const next = this.queue.find(
      (q) => q.status === "waiting" && q.modelId !== null && q.pageCount,
    );
    if (!next) return;
    next.status = "converting";
    next.currentPage = next.pagesDone;
    next.stage = STAGES[0]!;
    this.jobs.set(next.fileId, { pageElapsed: 0, exportLeft: null });
    this.emitItem(next);
  }

  private pageDuration(item: QueueItem): number {
    return item.textMode === "existingText" ? Math.min(150, this.pageMs) : this.pageMs;
  }

  private advanceQueue(ms: number): void {
    for (const [fileId, left] of this.inspectLeft) {
      if (left - ms <= 0) {
        this.inspectLeft.delete(fileId);
        const item = this.queue.find((q) => q.fileId === fileId);
        if (item && item.status === "waiting") this.inspect(item);
      } else {
        this.inspectLeft.set(fileId, left - ms);
      }
    }

    this.startNext();
    const item = this.converting();
    if (!item || this.paused) return;
    const job = this.jobs.get(item.fileId)!;

    if (job.exportLeft !== null) {
      job.exportLeft -= ms;
      if (job.exportLeft <= 0) this.finish(item);
      return;
    }

    const total = this.pageDuration(item);
    job.pageElapsed += ms;
    const index = item.currentPage ?? 0;

    if (item.name.toLowerCase().includes("huge") && index === 1 && !this.oomOnce.has(item.fileId)) {
      if (job.pageElapsed >= total / 2) {
        this.oomOnce.add(item.fileId);
        item.status = "failed";
        item.stage = null;
        item.currentPage = null;
        item.secondsLeft = null;
        item.error = appError("outOfMemory", "gpu");
        this.emitItem(item);
        return;
      }
    }

    const stage =
      STAGES[Math.min(STAGES.length - 1, Math.floor((job.pageElapsed / total) * STAGES.length))]!;
    if (job.pageElapsed < total) {
      if (stage !== item.stage) {
        item.stage = stage;
        this.emitItem(item);
      }
      return;
    }

    job.pageElapsed = 0;
    item.pagesDone += 1;
    const pageCount = item.pageCount ?? 1;
    item.secondsLeft = (pageCount - item.pagesDone) * SECONDS_PER_PAGE;
    const doneStatus: PageStatus = {
      index,
      state: "done",
      imagePath: pageImageUrl(index % FIXTURE_PAGE_COUNT),
      error: null,
    };
    if (item.pagesDone >= pageCount) {
      item.currentPage = null;
      item.stage = "export";
      job.exportLeft = EXPORT_MS;
    } else {
      item.currentPage = item.pagesDone;
      item.stage = STAGES[0]!;
    }
    this.emitItem(item, doneStatus);
  }

  private finish(item: QueueItem): void {
    item.status = "done";
    item.stage = null;
    item.secondsLeft = null;
    item.currentPage = null;
    item.durationMs = Math.round((item.pageCount ?? 1) * this.pageDuration(item));
    item.outputs = item.formats.map((format) => this.writeOutput(item, format));
    this.jobs.delete(item.fileId);
    this.emitItem(item);
    this.startNext();
  }

  /** Pretends the user moved or deleted the saved files of this format (or all of them). */
  markOutputsGone(format: ExportFormat | "all"): void {
    for (const item of this.queue) {
      for (const o of item.outputs) {
        if (format === "all" || o.format === format) this.gone.add(o.path);
      }
    }
  }

  private outputDir(item: QueueItem): string {
    const target = this.saveTo.get(item.fileId) ?? this.settings.saveTo;
    return target.kind === "folder" ? target.path : parentDir(item.path);
  }

  private writeOutput(item: QueueItem, format: ExportFormat): OutputFile {
    const dir = this.outputDir(item);
    const sep = dir.includes("/") ? "/" : "\\";
    const ext = format === "docx" ? "docx" : "md";
    const base = baseName(item.name);
    let n = 1;
    let path: string;
    do {
      const suffix = n === 1 ? "" : ` (${n})`;
      path = `${dir}${sep}${base}${suffix}.${ext}`;
      n += 1;
    } while (this.usedNames.has(path));
    this.usedNames.add(path);
    return {
      format,
      path,
      assetsDir: format === "markdown" ? path.replace(/\.md$/, "_assets") : null,
    };
  }

  // -------------------------------------------------------------------------------------------
  // Commands

  private readonly handlers: {
    [C in CommandName]: (args: Commands[C]["args"]) => Commands[C]["result"];
  } = {
    probe_hardware: () => hardwareFor(this.hardwareProfile),
    get_recommendation: () => recommendationFor(hardwareFor(this.hardwareProfile)),
    start_setup_download: ({ modelIds }) => {
      if (modelIds.length === 0) throw appError("invalidArgument");
      this.beginDownload(modelIds);
      return null;
    },
    setup_pause: () => {
      if (this.phase === "downloading") {
        this.phase = "paused";
        this.components.forEach((c) => {
          c.bytesPerSecond = null;
          c.secondsLeft = null;
        });
        this.emitSetup();
      }
      return null;
    },
    setup_resume: () => {
      if (this.phase === "paused") {
        this.phase = "downloading";
        this.emitSetup();
      }
      return null;
    },
    setup_status: () => this.setupSnapshot(),

    queue_add: ({ paths }) => this.add(paths),
    queue_list: () => this.queue,
    queue_start: ({ options }) => this.startBatch(options),
    queue_pause: () => {
      const item = this.converting();
      this.paused = true;
      if (item) {
        item.status = "paused";
        this.emitItem(item);
      }
      return null;
    },
    queue_resume: () => {
      this.paused = false;
      const item = this.queue.find((q) => q.status === "paused");
      if (item) {
        item.status = "converting";
        this.emitItem(item);
      }
      return null;
    },
    queue_cancel: ({ fileId }) => {
      const targets = fileId
        ? [this.find(fileId)]
        : this.queue.filter((q) => ["waiting", "converting", "paused"].includes(q.status));
      for (const item of targets) {
        if (!["waiting", "converting", "paused"].includes(item.status)) continue;
        item.status = "cancelled";
        item.stage = null;
        item.currentPage = null;
        item.secondsLeft = null;
        this.jobs.delete(item.fileId);
        this.emitItem(item);
      }
      if (!this.queue.some((q) => q.status === "paused")) this.paused = false;
      return null;
    },
    queue_remove: ({ fileId }) => {
      const item = this.find(fileId);
      if (item.status === "converting") throw appError("invalidArgument");
      this.queue = this.queue.filter((q) => q.fileId !== fileId);
      return null;
    },
    queue_clear_done: () => {
      const gone = this.queue.filter((q) => q.status === "done" || q.status === "cancelled");
      this.queue = this.queue.filter((q) => !gone.includes(q));
      return gone.map((q) => q.fileId);
    },
    queue_retry: ({ fileId }) => {
      const item = this.find(fileId);
      if (item.status !== "failed" && item.status !== "cancelled")
        throw appError("invalidArgument");
      const name = item.name.toLowerCase();
      if (name.includes("corrupt") || name.includes("password")) {
        item.error = appError(
          "badInput",
          name.includes("password") ? "password_protected" : "corrupt",
        );
        this.emitItem(item);
        return null;
      }
      item.status = "waiting";
      item.error = null;
      item.modelId ??= this.firstInstalled();
      this.emitItem(item);
      return null;
    },
    queue_set_text_mode: ({ fileId, mode }) => {
      const item = this.find(fileId);
      item.textMode = mode;
      item.attention = null;
      item.status = "waiting";
      this.emitItem(item);
      return null;
    },
    get_pages: ({ fileId }) => {
      const item = this.find(fileId);
      return Array.from({ length: item.pageCount ?? 0 }, (_, i) => this.pageStatus(item, i));
    },
    get_review: ({ fileId, page }): ReviewPage => {
      const item = this.find(fileId);
      if (page < 0 || page >= (item.pageCount ?? 0) || page >= item.pagesDone) {
        throw appError("notFound");
      }
      const ir = fixturePage(page, page);
      return {
        fileId,
        pageIndex: page,
        pageCount: item.pageCount ?? 1,
        imagePath: ir.sourceImage,
        page: ir,
        engine: MOCK_ENGINE,
      };
    },
    export: ({ fileId, format }) => {
      const item = this.find(fileId);
      if (item.status !== "done") throw appError("invalidArgument");
      const output = this.writeOutput(item, format);
      item.outputs.push(output);
      return output;
    },
    reveal_in_folder: ({ path }) => {
      this.revealed.push(path);
      return null;
    },
    export_to: ({ fileId, format, destPath, overwrite }) => {
      const item = this.find(fileId);
      if (item.status !== "done") throw appError("invalidArgument", "not_done");
      const ext = destPath.split(".").pop()?.toLowerCase() ?? "";
      const okExt = format === "docx" ? ext === "docx" : ext === "md" || ext === "markdown";
      if (!okExt) throw appError("invalidArgument", "bad_extension");
      if (this.usedNames.has(destPath) && !this.gone.has(destPath) && !overwrite) {
        throw appError("io", "already_exists");
      }
      this.usedNames.add(destPath);
      this.gone.delete(destPath);
      this.savedCopies.push({ fileId, format, path: destPath });
      return {
        format,
        path: destPath,
        assetsDir: format === "markdown" ? destPath.replace(/.[^.]+$/, "_assets") : null,
      };
    },
    check_outputs: ({ fileId }) =>
      this.find(fileId).outputs.map((o) => ({
        format: o.format,
        path: o.path,
        exists: !this.gone.has(o.path),
      })),
    open_output: ({ path }) => {
      const known = this.queue.some((i) => i.outputs.some((o) => o.path === path));
      if (!known) throw appError("invalidArgument", "not_an_output");
      if (this.gone.has(path)) throw appError("notFound", "file_missing");
      this.openedOutputs.push(path);
      return null;
    },

    settings_get: () => this.settings,
    settings_set: ({ patch }) => {
      if (patch.modelsDir && patch.modelsDir !== this.settings.modelsDir) {
        const busy = this.installed.size > 0 || this.phase === "downloading";
        if (busy) throw appError("invalidArgument");
      }
      const next = { ...this.settings };
      for (const [key, value] of Object.entries(patch)) {
        if (value !== null && value !== undefined) (next as Record<string, unknown>)[key] = value;
      }
      this.settings = next;
      return next;
    },

    models_list: () => this.modelInfos(),
    models_install: ({ modelId }) => {
      const model = MODEL_CATALOG.find((m) => m.id === modelId);
      if (!model) throw appError("notFound");
      this.beginDownload([modelId]);
      return null;
    },
    models_remove: ({ modelId }) => {
      if (this.queue.some((q) => q.modelId === modelId && q.status === "converting")) {
        throw appError("invalidArgument");
      }
      this.installed.delete(modelId);
      this.components = this.components.filter((c) => c.id !== modelId);
      this.emitSetup();
      return null;
    },
    models_import: ({ modelId, folder }) => {
      if (!MODEL_CATALOG.some((m) => m.id === modelId)) throw appError("notFound");
      if (folder.toLowerCase().includes("corrupt")) throw appError("checksumMismatch");
      const component = this.modelComponent(modelId, 0.3);
      this.components = [...this.components.filter((c) => c.id !== modelId), component];
      this.pendingModels = [modelId];
      this.phase = "downloading";
      this.emitSetup();
      return null;
    },

    app_version: () => "0.1.0",
    get_diagnostics: () =>
      "OpenDoc Reader 0.1.0 (mock)\nOS: Windows 11\nInstalled models: " +
      [...this.installed].join(", "),
    open_logs_folder: () => null,
  };

  private firstInstalled(): string {
    return [...this.installed][0] ?? ACCURATE_ID;
  }

  private startBatch(options: ConvertOptions): null {
    if (!this.installed.has(options.modelId)) throw appError("modelNotReady", "files_missing");
    if (options.formats.length === 0) throw appError("invalidArgument");
    for (const item of this.queue) {
      if (item.status !== "waiting" || item.modelId !== null) continue;
      item.modelId = options.modelId;
      item.formats = options.formats;
      this.saveTo.set(item.fileId, options.saveTo);
      this.emitItem(item);
    }
    this.startNext();
    return null;
  }
}
