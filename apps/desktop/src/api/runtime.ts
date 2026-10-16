import { createApi, type Api } from "./api";
import { MockBackend, type MockScenario } from "./mock/backend";
import type { HardwareProfile } from "./mock/catalog";
import { createMockPlatform } from "./mock/platform";
import type { Platform } from "./platform";
import { tauriPlatform, tauriTransport } from "./tauri";

export type DevScreen = "setup" | "main" | "empty" | "review" | "settings";
export type DevView = "original" | "split" | "result";

/** URL parameters that steer the mock, so a screen can be opened directly for a screenshot. */
export interface DevParams {
  mock: boolean;
  screen: DevScreen | null;
  theme: "light" | "dark" | null;
  step: number | null;
  section: string | null;
  page: number | null;
  view: DevView | null;
  /** Mock time in milliseconds to run before the first render. */
  advanceMs: number;
  /** Keep the mock clock stopped, so a screenshot shows a steady picture. */
  freeze: boolean;
  hardware: HardwareProfile | null;
  fail: "offline" | "diskFull" | "checksumMismatch" | null;
  /** Show the review mode without a file picked: select the file with this name. */
  file: string | null;
}

export interface Runtime {
  api: Api;
  platform: Platform;
  /** Set when the mock backend is in use. */
  mock: MockBackend | null;
  dev: DevParams;
}

function pick<T extends string>(value: string | null, allowed: readonly T[]): T | null {
  return allowed.includes(value as T) ? (value as T) : null;
}

function int(value: string | null): number | null {
  const n = value === null ? NaN : parseInt(value, 10);
  return Number.isFinite(n) ? n : null;
}

export function parseDevParams(search: string): DevParams {
  const p = new URLSearchParams(search);
  return {
    mock: p.get("mock") === "1",
    screen: pick(p.get("screen"), ["setup", "main", "empty", "review", "settings"] as const),
    theme: pick(p.get("theme"), ["light", "dark"] as const),
    step: int(p.get("step")),
    section: p.get("section"),
    page: int(p.get("page")),
    view: pick(p.get("view"), ["original", "split", "result"] as const),
    advanceMs: int(p.get("t")) ?? 0,
    freeze: p.get("freeze") === "1",
    hardware: pick(p.get("hw"), ["gpu", "cpu", "mac"] as const),
    fail: pick(p.get("fail"), ["offline", "diskFull", "checksumMismatch"] as const),
    file: p.get("file"),
  };
}

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function scenarioFor(dev: DevParams): MockScenario {
  if (dev.screen === "setup") return dev.step === 4 ? "downloading" : "fresh";
  if (dev.screen === "empty") return "ready-empty";
  return "demo";
}

/**
 * The real core inside Tauri, the mock everywhere else (and whenever the URL says `mock=1`).
 */
export function createRuntime(
  search = typeof location === "undefined" ? "" : location.search,
): Runtime {
  const dev = parseDevParams(search);
  if (isTauri() && !dev.mock) {
    return { api: createApi(tauriTransport), platform: tauriPlatform, mock: null, dev };
  }
  const mock = new MockBackend({
    scenario: scenarioFor(dev),
    hardware: dev.hardware ?? "gpu",
    theme: dev.theme ?? "system",
    latencyMs: 60,
    failDownload: dev.fail ?? undefined,
  });
  if (dev.advanceMs > 0) mock.advance(dev.advanceMs);
  if (!dev.freeze) mock.start();
  return { api: createApi(mock), platform: createMockPlatform(), mock, dev };
}
