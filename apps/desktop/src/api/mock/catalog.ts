import type {
  FitBadge,
  HardwareInfo,
  ModelFit,
  ModelInfo,
  Recommendation,
  SpeedEstimate,
} from "../../types/commands";

export type HardwareProfile = "gpu" | "cpu" | "mac";

export const MODELS_DIR = "G:\\OpenDoc Reader\\models";
export const RUNTIME_BYTES = 410_000_000;
export const ENGINE_BYTES = 380_000_000;

export const FAST_ID = "fast-ppocrv6-tiny";
export const ACCURATE_ID = "accurate-paddleocr-vl-1.6";
export const MAX_ID = "max-deepseek-ocr-1-q8";
export const DS2_ID = "experimental-deepseek-ocr-2-q8";

/** The manifest's models, without install state. */
export const MODEL_CATALOG: Omit<ModelInfo, "state" | "installedSizeBytes">[] = [
  {
    id: FAST_ID,
    tier: "fast",
    displayName: "PP-OCRv6 tiny + PP-StructureV3",
    engineId: "paddleocr-structure",
    experimental: false,
    license: "Apache-2.0",
    downloadSizeBytes: 176_035_573,
    sourceUrl: "https://github.com/PaddlePaddle/PaddleOCR",
  },
  {
    id: ACCURATE_ID,
    tier: "accurate",
    displayName: "PaddleOCR-VL 1.6",
    engineId: "paddleocr-vl",
    experimental: false,
    license: "Apache-2.0",
    downloadSizeBytes: 1_988_649_560,
    sourceUrl: "https://github.com/PaddlePaddle/PaddleOCR",
  },
  {
    id: MAX_ID,
    tier: "max",
    displayName: "DeepSeek-OCR v1 (Q8_0)",
    engineId: "deepseek-ocr",
    experimental: false,
    license: "MIT",
    downloadSizeBytes: 3_613_101_480,
    sourceUrl: "https://github.com/deepseek-ai/DeepSeek-OCR",
  },
  {
    id: DS2_ID,
    tier: "max",
    displayName: "DeepSeek-OCR 2 (Q8_0, experimental)",
    engineId: "deepseek-ocr",
    experimental: true,
    license: "Apache-2.0",
    downloadSizeBytes: 3_677_782_600,
    sourceUrl: "https://github.com/deepseek-ai/DeepSeek-OCR-2",
  },
];

export function hardwareFor(profile: HardwareProfile): HardwareInfo {
  switch (profile) {
    case "cpu":
      return {
        os: "windows",
        gpus: [],
        backend: "cpu",
        ramMB: 6144,
        unifiedMemoryMB: null,
        cpuName: "Intel(R) Core(TM) i5-8250U CPU @ 1.60GHz",
        cpuCores: 8,
        modelsDir: MODELS_DIR,
        freeDiskMB: 96_000,
        cudaDriverVersion: null,
      };
    case "mac":
      return {
        os: "macos",
        gpus: [{ vendor: "apple", name: "Apple M2", vramMB: null }],
        backend: "metal",
        ramMB: 16384,
        unifiedMemoryMB: 16384,
        cpuName: "Apple M2",
        cpuCores: 8,
        modelsDir: "/Users/demo/Library/Application Support/OpenDoc Reader/models",
        freeDiskMB: 180_000,
        cudaDriverVersion: null,
      };
    default:
      return {
        os: "windows",
        gpus: [{ vendor: "nvidia", name: "NVIDIA GeForce RTX 4090", vramMB: 24576 }],
        backend: "cuda",
        ramMB: 65536,
        unifiedMemoryMB: null,
        cpuName: "AMD Ryzen 7 7800X3D 8-Core Processor",
        cpuCores: 16,
        modelsDir: MODELS_DIR,
        freeDiskMB: 396_000,
        cudaDriverVersion: "576.80",
      };
  }
}

function vramMB(hw: HardwareInfo): number {
  return hw.unifiedMemoryMB ?? hw.gpus[0]?.vramMB ?? 0;
}

function fitOf(modelId: string, hw: HardwareInfo): ModelFit {
  const gpu = hw.backend === "cuda" || hw.backend === "vulkan";
  const mac = hw.backend === "metal";
  const mem = vramMB(hw);
  let badge: FitBadge;
  let speed: SpeedEstimate | null;
  switch (modelId) {
    case FAST_ID:
      badge = hw.ramMB >= 2048 ? "runsWell" : "wontFit";
      speed = { secondsPerPage: gpu ? 7 : 8.4, basis: "estimated" };
      break;
    case ACCURATE_ID:
      badge =
        (gpu && mem >= 3072) || (mac && mem >= 16384)
          ? "runsWell"
          : hw.ramMB >= 4096
            ? "slow"
            : "wontFit";
      speed = { secondsPerPage: gpu ? 4.8 : mac ? 9 : 42, basis: gpu ? "measured" : "estimated" };
      break;
    case MAX_ID:
      badge =
        (gpu && mem >= 6144) || (mac && mem >= 16384)
          ? "runsWell"
          : hw.ramMB >= 10240
            ? "slow"
            : "wontFit";
      speed = { secondsPerPage: gpu ? 3 : mac ? 12 : 55, basis: gpu ? "measured" : "estimated" };
      break;
    default:
      badge = (gpu && mem >= 8192) || (mac && mem >= 16384) ? "runsWell" : "hidden";
      speed = badge === "hidden" ? null : { secondsPerPage: 4, basis: "estimated" };
  }
  return { modelId, badge, speed };
}

export function recommendationFor(hw: HardwareInfo): Recommendation {
  const fits = MODEL_CATALOG.map((m) => fitOf(m.id, hw));
  const gpu = hw.backend === "cuda" || hw.backend === "vulkan";
  const gpuName = hw.gpus[0]?.name ?? "graphics card";
  let modelId = ACCURATE_ID;
  let reason = "Best balance of accuracy and speed on this computer.";
  if (gpu && vramMB(hw) >= 8192) {
    modelId = MAX_ID;
    reason = `Your ${gpuName} (${Math.round(vramMB(hw) / 1024)} GB) runs Max on the GPU.`;
  } else if (hw.backend === "metal" && vramMB(hw) >= 16384) {
    modelId = MAX_ID;
    reason = `Your Mac has ${Math.round(vramMB(hw) / 1024)} GB of memory, enough to run Max.`;
  } else if (hw.backend === "cpu" && hw.ramMB < 8192) {
    modelId = FAST_ID;
    reason = `No supported GPU was found and this PC has ${Math.round(hw.ramMB / 1024)} GB of RAM, so the lightweight Fast tier is the practical choice. Tables and formulas stay as images.`;
  } else if (hw.backend === "cpu" && hw.ramMB >= 16384) {
    modelId = MAX_ID;
    reason = `No supported GPU was found, but with ${Math.round(hw.ramMB / 1024)} GB of RAM Max measured faster on a CPU than Accurate.`;
  }
  return { modelId, reason, fits, runtimeDownloadBytes: RUNTIME_BYTES + ENGINE_BYTES };
}
