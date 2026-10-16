import { describe, expect, it } from "vitest";
import rawExamples from "../../../../docs/protocol-examples.json?raw";
import {
  EVENT_QUEUE_PROGRESS,
  EVENT_SETUP_PROGRESS,
  type AppError,
  type ConvertOptions,
  type HardwareInfo,
  type ModelInfo,
  type OutputFile,
  type OutputStatus,
  type PageStatus,
  type QueueItem,
  type QueueProgress,
  type Recommendation,
  type ReviewPage,
  type SetupStatus,
  type Settings,
  type SettingsPatch,
} from "./commands";

// docs/protocol-examples.json is the single set of examples that docs/protocol.md quotes and
// that the Rust contract test (src-tauri/src/contract.rs) round-trips. Each typed literal below
// must equal its example, so a field renamed on one side fails here at compile time or at run
// time.
const examples = JSON.parse(rawExamples) as Record<string, unknown>;

const work = "C:\\Users\\Ada\\AppData\\Roaming\\OpenDoc Reader\\work\\f-9c41";
const modelsDir = "G:\\OpenDoc Reader\\models";

const queueItem: QueueItem = {
  fileId: "f-9c41",
  path: "C:\\Users\\Ada\\Documents\\invoice.pdf",
  name: "invoice.pdf",
  kind: "pdf",
  sizeBytes: 1843200,
  pageCount: 12,
  hasTextLayer: false,
  previewPath: `${work}\\pages\\page-0000.png`,
  status: "converting",
  attention: null,
  textMode: "ocr",
  modelId: "accurate-paddleocr-vl-1.6",
  formats: ["docx", "markdown"],
  pagesDone: 3,
  currentPage: 3,
  stage: "recognize",
  secondsLeft: 43.5,
  durationMs: null,
  outputs: [],
  error: null,
};

describe("Tauri contract types match docs/protocol-examples.json", () => {
  it("AppError", () => {
    const value: AppError = {
      code: "badInput",
      message: "The file cannot be converted.",
      reason: "password_protected",
      detail: null,
    };
    expect(value).toEqual(examples["tauri.AppError"]);
  });

  it("HardwareInfo", () => {
    const value: HardwareInfo = {
      os: "windows",
      gpus: [{ vendor: "nvidia", name: "NVIDIA GeForce RTX 3060", vramMB: 12288 }],
      backend: "cuda",
      ramMB: 32768,
      unifiedMemoryMB: null,
      cpuName: "AMD Ryzen 7 5800X 8-Core Processor",
      cpuCores: 16,
      modelsDir,
      freeDiskMB: 412000,
      cudaDriverVersion: "576.80",
    };
    expect(value).toEqual(examples["tauri.HardwareInfo"]);
  });

  it("Recommendation", () => {
    const value: Recommendation = {
      modelId: "max-deepseek-ocr-1-q8",
      reason: "Your NVIDIA GeForce RTX 3060 (12 GB) runs Max on the GPU.",
      fits: [
        {
          modelId: "fast-ppocrv6-tiny",
          badge: "runsWell",
          speed: { secondsPerPage: 8.4, basis: "estimated" },
        },
        {
          modelId: "accurate-paddleocr-vl-1.6",
          badge: "runsWell",
          speed: { secondsPerPage: 4.8, basis: "measured" },
        },
        {
          modelId: "max-deepseek-ocr-1-q8",
          badge: "runsWell",
          speed: { secondsPerPage: 5.2, basis: "measured" },
        },
        { modelId: "experimental-deepseek-ocr-2-q8", badge: "hidden", speed: null },
      ],
      runtimeDownloadBytes: 1500000000,
    };
    expect(value).toEqual(examples["tauri.Recommendation"]);
  });

  it("ModelInfo", () => {
    const value: ModelInfo = {
      id: "accurate-paddleocr-vl-1.6",
      tier: "accurate",
      displayName: "PaddleOCR-VL 1.6",
      engineId: "paddleocr-vl",
      experimental: false,
      license: "Apache-2.0",
      downloadSizeBytes: 1988649560,
      installedSizeBytes: null,
      state: "notInstalled",
      sourceUrl: "https://huggingface.co/PaddlePaddle/PaddleOCR-VL-1.6-GGUF",
    };
    expect(value).toEqual(examples["tauri.ModelInfo"]);
  });

  it("SetupStatus", () => {
    const value: SetupStatus = {
      phase: "downloading",
      modelsDir,
      components: [
        {
          id: "llama-server",
          kind: "runtime",
          name: "llama-server (CUDA)",
          state: "done",
          bytesDone: 578117583,
          bytesTotal: 578117583,
          bytesPerSecond: null,
          secondsLeft: null,
        },
        {
          id: "engine",
          kind: "engine",
          name: "Document engine",
          state: "downloading",
          bytesDone: 0,
          bytesTotal: 0,
          bytesPerSecond: null,
          secondsLeft: null,
        },
        {
          id: "accurate-paddleocr-vl-1.6",
          kind: "model",
          name: "PaddleOCR-VL 1.6",
          state: "downloading",
          bytesDone: 912000000,
          bytesTotal: 1988649560,
          bytesPerSecond: 18500000.5,
          secondsLeft: 56.4,
        },
      ],
      calibration: null,
      error: null,
    };
    expect(value).toEqual(examples["tauri.SetupStatus"]);
  });

  it("Settings and SettingsPatch", () => {
    const settings: Settings = {
      defaultFormats: ["docx", "markdown"],
      defaultModelId: null,
      saveTo: { kind: "original" },
      theme: "system",
      checkForUpdates: true,
      modelsDir,
      experimentalVulkan: false,
    };
    expect(settings).toEqual(examples["tauri.Settings"]);

    const patch: SettingsPatch = {
      theme: "dark",
      saveTo: { kind: "folder", path: "D:\\Converted" },
    };
    expect(patch).toEqual(examples["tauri.SettingsPatch"]);
  });

  it("ConvertOptions", () => {
    const value: ConvertOptions = {
      formats: ["docx", "markdown"],
      modelId: "accurate-paddleocr-vl-1.6",
      saveTo: { kind: "original" },
    };
    expect(value).toEqual(examples["tauri.ConvertOptions"]);
  });

  it("QueueItem, PageStatus and OutputFile", () => {
    expect(queueItem).toEqual(examples["tauri.QueueItem"]);

    const page: PageStatus = {
      index: 3,
      state: "converting",
      imagePath: `${work}\\pages\\page-0003.png`,
      error: null,
    };
    expect(page).toEqual(examples["tauri.PageStatus"]);

    const output: OutputFile = {
      format: "markdown",
      path: "C:\\Users\\Ada\\Documents\\invoice.md",
      assetsDir: "C:\\Users\\Ada\\Documents\\invoice_assets",
    };
    expect(output).toEqual(examples["tauri.OutputFile"]);

    const status: OutputStatus = {
      format: "docx",
      path: "C:\\Users\\Ada\\Documents\\invoice.docx",
      exists: true,
    };
    expect(status).toEqual(examples["tauri.OutputStatus"]);
  });

  it("ReviewPage", () => {
    const value: ReviewPage = {
      fileId: "f-9c41",
      pageIndex: 3,
      pageCount: 12,
      imagePath: `${work}\\pages\\page-0003.png`,
      page: {
        index: 3,
        width: 1654,
        height: 2339,
        sourceImage: `${work}\\pages\\page-0003.png`,
        blocks: [
          {
            id: "p3-b0",
            type: "heading",
            bbox: [165.4, 140.2, 1488.6, 233.9],
            order: 0,
            level: 1,
            text: "3. Results",
          },
          {
            id: "p3-b1",
            type: "paragraph",
            bbox: [165.4, 280.0, 1488.6, 520.0],
            order: 1,
            text: "The energy of a body at rest is given by",
          },
          {
            id: "p3-b2",
            type: "formula",
            bbox: [496.2, 560.0, 1157.8, 640.0],
            order: 2,
            latex: "E = mc^2",
            number: "(1)",
          },
        ],
      },
      engine: { id: "paddleocr-vl", model: "accurate-paddleocr-vl-1.6", version: "0.1.0" },
    };
    expect(value).toEqual(examples["tauri.ReviewPage"]);
  });

  it("event payloads", () => {
    const progress: QueueProgress = {
      item: {
        ...queueItem,
        pagesDone: 4,
        currentPage: 4,
        stage: "render",
        secondsLeft: 38.7,
      },
      page: {
        index: 3,
        state: "done",
        imagePath: `${work}\\pages\\page-0003.png`,
        error: null,
      },
    };
    expect(progress).toEqual(examples["tauri.event.queue-progress"]);

    const setup: SetupStatus = {
      phase: "calibrating",
      modelsDir,
      components: [
        {
          id: "accurate-paddleocr-vl-1.6",
          kind: "model",
          name: "PaddleOCR-VL 1.6",
          state: "done",
          bytesDone: 1988649560,
          bytesTotal: 1988649560,
          bytesPerSecond: null,
          secondsLeft: null,
        },
      ],
      calibration: null,
      error: null,
    };
    expect(setup).toEqual(examples["tauri.event.setup-progress"]);
  });

  it("event names", () => {
    expect(EVENT_QUEUE_PROGRESS).toBe("queue://progress");
    expect(EVENT_SETUP_PROGRESS).toBe("setup://progress");
  });
});
