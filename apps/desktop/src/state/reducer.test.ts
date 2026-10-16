import { describe, expect, it } from "vitest";
import type { PageStatus, QueueItem, Settings, SetupStatus } from "../types/commands";
import { initialState, reducer, setupStepFor, type AppState } from "./reducer";
import {
  convertibleItems,
  effectiveModelId,
  percentDone,
  preferredModelId,
  smallerInstalledModel,
} from "./selectors";

function item(over: Partial<QueueItem> = {}): QueueItem {
  return {
    fileId: "f-1",
    path: "C:\\Docs\\a.pdf",
    name: "a.pdf",
    kind: "pdf",
    sizeBytes: 1000,
    pageCount: 4,
    hasTextLayer: false,
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
    ...over,
  };
}

const settings: Settings = {
  defaultFormats: ["markdown"],
  defaultModelId: null,
  saveTo: { kind: "folder", path: "D:\\Out" },
  theme: "system",
  checkForUpdates: true,
  modelsDir: "G:\\models",
  experimentalVulkan: false,
};

function setup(phase: SetupStatus["phase"]): SetupStatus {
  return { phase, modelsDir: "G:\\models", components: [], calibration: null, error: null };
}

const booted = (phase: SetupStatus["phase"], queue: QueueItem[] = []): AppState =>
  reducer(initialState, {
    type: "booted",
    setup: setup(phase),
    settings,
    models: [],
    queue,
  });

describe("boot", () => {
  it("opens Main when a model is ready and Setup otherwise", () => {
    expect(booted("ready").screen).toBe("main");
    expect(booted("notStarted").screen).toBe("setup");
    expect(booted("downloading").screen).toBe("setup");
    expect(booted("failed").screen).toBe("setup");
  });

  it("starts Setup at the step that matches the download", () => {
    expect(setupStepFor(setup("notStarted"))).toBe(1);
    expect(booted("notStarted").setupUi.step).toBe(1);
    expect(booted("paused").setupUi.step).toBe(4);
    expect(booted("calibrating").setupUi.step).toBe(4);
  });

  it("takes the batch defaults from the settings and selects the first file", () => {
    const state = booted("ready", [item(), item({ fileId: "f-2" })]);
    expect(state.choice.formats).toEqual(["markdown"]);
    expect(state.choice.saveTo).toEqual({ kind: "folder", path: "D:\\Out" });
    expect(state.selectedId).toBe("f-1");
  });

  it("records a failed start", () => {
    expect(reducer(initialState, { type: "bootFailed" }).boot).toBe("failed");
  });
});

describe("queue events", () => {
  it("replaces the item with the same id and keeps its place", () => {
    const state = booted("ready", [item(), item({ fileId: "f-2", name: "b.pdf" })]);
    const next = reducer(state, {
      type: "queueProgress",
      progress: { item: item({ status: "converting", pagesDone: 2 }), page: null },
    });
    expect(next.queue.map((q) => q.fileId)).toEqual(["f-1", "f-2"]);
    expect(next.queue[0]).toMatchObject({ status: "converting", pagesDone: 2 });
  });

  it("appends an item it has not seen", () => {
    const next = reducer(booted("ready"), {
      type: "queueProgress",
      progress: { item: item({ fileId: "f-9" }), page: null },
    });
    expect(next.queue.map((q) => q.fileId)).toEqual(["f-9"]);
    expect(next.selectedId).toBe("f-9");
  });

  it("merges a page status into a page list that was loaded", () => {
    const pages: PageStatus[] = [0, 1, 2].map((index) => ({
      index,
      state: "pending",
      imagePath: null,
      error: null,
    }));
    let state = booted("ready", [item()]);
    state = reducer(state, { type: "pagesLoaded", fileId: "f-1", pages });
    const done: PageStatus = { index: 1, state: "done", imagePath: "p.png", error: null };
    state = reducer(state, { type: "queueProgress", progress: { item: item(), page: done } });
    expect(state.pages["f-1"]?.[1]).toEqual(done);
    expect(state.pages["f-1"]?.[0]?.state).toBe("pending");
  });

  it("does not invent a page list for a file that has none", () => {
    const done: PageStatus = { index: 0, state: "done", imagePath: null, error: null };
    const state = reducer(booted("ready", [item()]), {
      type: "queueProgress",
      progress: { item: item(), page: done },
    });
    expect(state.pages["f-1"]).toBeUndefined();
  });

  it("drops removed items and moves the selection and Review off them", () => {
    let state = booted("ready", [item({ status: "done" }), item({ fileId: "f-2" })]);
    state = reducer(state, { type: "openReview", fileId: "f-1" });
    state = reducer(state, { type: "queueRemoved", fileIds: ["f-1"] });
    expect(state.queue.map((q) => q.fileId)).toEqual(["f-2"]);
    expect(state.selectedId).toBe("f-2");
    expect(state.reviewId).toBeNull();
  });

  it("selects the first added item and leaves Review", () => {
    let state = booted("ready", [item({ status: "done" })]);
    state = reducer(state, { type: "openReview", fileId: "f-1" });
    state = reducer(state, { type: "queueAdded", items: [item({ fileId: "f-5" })] });
    expect(state.selectedId).toBe("f-5");
    expect(state.reviewId).toBeNull();
  });
});

describe("navigation", () => {
  it("keeps Review open on another finished file and leaves it for one that is not done", () => {
    let state = booted("ready", [
      item({ fileId: "a", status: "done" }),
      item({ fileId: "b", status: "done" }),
      item({ fileId: "c", status: "waiting" }),
    ]);
    state = reducer(state, { type: "openReview", fileId: "a" });
    state = reducer(state, { type: "select", fileId: "b" });
    expect(state.reviewId).toBe("b");
    state = reducer(state, { type: "select", fileId: "c" });
    expect(state.reviewId).toBeNull();
    expect(state.selectedId).toBe("c");
  });

  it("returns from Settings to where it was opened", () => {
    const fromMain = reducer(booted("ready"), { type: "openSettings", section: "models" });
    expect(fromMain.screen).toBe("settings");
    expect(fromMain.settingsSection).toBe("models");
    expect(reducer(fromMain, { type: "closeSettings" }).screen).toBe("main");

    const fromSetup = reducer(booted("notStarted"), { type: "openSettings" });
    expect(reducer(fromSetup, { type: "closeSettings" }).screen).toBe("setup");
  });

  it("finishes Setup into Main", () => {
    expect(reducer(booted("ready"), { type: "finishSetup" }).screen).toBe("main");
  });
});

describe("toasts and choices", () => {
  it("adds and dismisses a toast by id", () => {
    let state = reducer(initialState, { type: "toast", id: 1, text: "Saved." });
    state = reducer(state, { type: "toast", id: 2, text: "Again." });
    state = reducer(state, { type: "dismissToast", id: 1 });
    expect(state.toasts).toEqual([{ id: 2, text: "Again." }]);
  });

  it("patches the batch choice", () => {
    const state = reducer(initialState, { type: "choice", patch: { modelId: "m" } });
    expect(state.choice.modelId).toBe("m");
    expect(state.choice.formats).toEqual(["docx", "markdown"]);
  });
});

describe("selectors", () => {
  const models = [
    ["fast", "installed"],
    ["accurate", "installed"],
    ["max", "notInstalled"],
  ] as const;
  const state: AppState = {
    ...booted("ready"),
    models: models.map(([tier, installed]) => ({
      id: `${tier}-model`,
      tier,
      displayName: tier,
      engineId: "e",
      experimental: false,
      license: "MIT",
      downloadSizeBytes: 1,
      installedSizeBytes: installed === "installed" ? 1 : null,
      state: installed,
      sourceUrl: null,
    })),
  };

  it("counts only waiting files that no batch has claimed", () => {
    const queue = [
      item({ fileId: "1" }),
      item({ fileId: "2", modelId: "accurate-model" }),
      item({ fileId: "3", status: "needsAttention" }),
      item({ fileId: "4", pageCount: null }),
    ];
    expect(convertibleItems(queue).map((q) => q.fileId)).toEqual(["1"]);
  });

  it("prefers the saved default, then the biggest installed model", () => {
    expect(preferredModelId(state)).toBe("accurate-model");
    const saved = { ...state, settings: { ...settings, defaultModelId: "fast-model" } };
    expect(preferredModelId(saved)).toBe("fast-model");
  });

  it("falls back from a choice that is not installed", () => {
    const stale = { ...state, choice: { ...state.choice, modelId: "max-model" } };
    expect(effectiveModelId(stale)).toBe("accurate-model");
    const picked = { ...state, choice: { ...state.choice, modelId: "fast-model" } };
    expect(effectiveModelId(picked)).toBe("fast-model");
  });

  it("finds the biggest installed model below a tier", () => {
    expect(smallerInstalledModel(state.models, "max-model")?.id).toBe("accurate-model");
    expect(smallerInstalledModel(state.models, "fast-model")).toBeNull();
    expect(smallerInstalledModel(state.models, null)).toBeNull();
  });

  it("rounds progress to whole percent", () => {
    expect(percentDone(item({ pagesDone: 30, pageCount: 48 }))).toBe(63);
    expect(percentDone(item({ pageCount: null }))).toBe(0);
  });
});
