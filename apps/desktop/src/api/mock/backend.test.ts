import { describe, expect, it } from "vitest";
import type { AppError, QueueProgress, SetupStatus } from "../../types/commands";
import { createApi } from "../api";
import { MockBackend, type MockOptions } from "./backend";
import { ACCURATE_ID, DS2_ID, FAST_ID, MAX_ID } from "./catalog";

function make(options: MockOptions = {}) {
  const backend = new MockBackend(options);
  const api = createApi(backend);
  const setupEvents: SetupStatus[] = [];
  const queueEvents: QueueProgress[] = [];
  void api.onSetupProgress((s) => setupEvents.push(s));
  void api.onQueueProgress((p) => queueEvents.push(p));
  return { backend, api, setupEvents, queueEvents };
}

async function rejection(promise: Promise<unknown>): Promise<AppError> {
  try {
    await promise;
  } catch (error) {
    return error as AppError;
  }
  throw new Error("expected the call to reject");
}

describe("setup download", () => {
  it("starts empty on a fresh install", async () => {
    const { api } = make({ scenario: "fresh" });
    const status = await api.setupStatus();
    expect(status.phase).toBe("notStarted");
    expect(status.components).toEqual([]);
    expect((await api.modelsList()).every((m) => m.state === "notInstalled")).toBe(true);
  });

  it("downloads the runtime, the engine and the model, then calibrates, then is ready", async () => {
    const { backend, api, setupEvents } = make({ scenario: "fresh" });
    await api.startSetupDownload([ACCURATE_ID]);
    let status = await api.setupStatus();
    expect(status.phase).toBe("downloading");
    expect(status.components.map((c) => c.kind)).toEqual(["runtime", "engine", "model"]);
    expect(status.components.every((c) => c.state === "pending")).toBe(true);

    backend.advance(1000);
    status = await api.setupStatus();
    const first = status.components.find((c) => c.state === "downloading");
    expect(first).toBeDefined();
    expect(first!.bytesDone).toBeGreaterThan(0);
    expect(first!.bytesPerSecond).toBeGreaterThan(0);
    expect(first!.secondsLeft).not.toBeNull();

    backend.advance(5000);
    status = await api.setupStatus();
    expect(status.phase).toBe("calibrating");
    expect(status.components.every((c) => c.state === "done")).toBe(true);
    expect(status.calibration).toBeNull();

    backend.advance(3000);
    status = await api.setupStatus();
    expect(status.phase).toBe("ready");
    expect(status.calibration?.modelId).toBe(ACCURATE_ID);
    expect(status.calibration?.secondsPerPage).toBeGreaterThan(0);

    const phases = setupEvents.map((e) => e.phase);
    expect(phases).toContain("downloading");
    expect(phases).toContain("calibrating");
    expect(phases.at(-1)).toBe("ready");
    const models = await api.modelsList();
    expect(models.find((m) => m.id === ACCURATE_ID)).toMatchObject({ state: "installed" });
  });

  it("pauses without losing progress and resumes", async () => {
    const { backend, api } = make({ scenario: "fresh" });
    await api.startSetupDownload([MAX_ID]);
    backend.advance(1500);
    await api.setupPause();
    const paused = await api.setupStatus();
    expect(paused.phase).toBe("paused");
    const bytes = paused.components.map((c) => c.bytesDone);

    backend.advance(5000);
    expect((await api.setupStatus()).components.map((c) => c.bytesDone)).toEqual(bytes);

    await api.setupResume();
    backend.advance(500);
    const resumed = await api.setupStatus();
    expect(resumed.phase).toBe("downloading");
    expect(resumed.components.some((c, i) => c.bytesDone > bytes[i]!)).toBe(true);
  });

  it("fails once with the requested error, keeps what was downloaded and finishes on retry", async () => {
    const { backend, api } = make({ scenario: "fresh", failDownload: "diskFull" });
    await api.startSetupDownload([ACCURATE_ID]);
    backend.advance(10_000);
    const failed = await api.setupStatus();
    expect(failed.phase).toBe("failed");
    expect(failed.error?.code).toBe("diskFull");
    expect(failed.components.find((c) => c.state === "failed")?.kind).toBe("model");

    await api.startSetupDownload([ACCURATE_ID]);
    const retried = await api.setupStatus();
    expect(retried.phase).toBe("downloading");
    expect(retried.error).toBeNull();
    backend.advance(20_000);
    expect((await api.setupStatus()).phase).toBe("ready");
  });

  it("rejects a download with no model", async () => {
    const { api } = make({ scenario: "fresh" });
    expect((await rejection(api.startSetupDownload([]))).code).toBe("invalidArgument");
  });

  it("installs one more model from Settings through the same events", async () => {
    const { backend, api } = make({ scenario: "ready-empty" });
    await api.modelsInstall(FAST_ID);
    expect((await api.modelsList()).find((m) => m.id === FAST_ID)?.state).toBe("downloading");
    backend.advance(10_000);
    expect((await api.modelsList()).find((m) => m.id === FAST_ID)?.state).toBe("installed");
    expect((await api.setupStatus()).phase).toBe("ready");
  });

  it("removes a model, and rejects an import that does not match the checksums", async () => {
    const { api } = make({ scenario: "ready-empty" });
    await api.modelsRemove(ACCURATE_ID);
    expect((await api.modelsList()).find((m) => m.id === ACCURATE_ID)?.state).toBe("notInstalled");
    expect((await rejection(api.modelsImport(DS2_ID, "D:\\corrupt-folder"))).code).toBe(
      "checksumMismatch",
    );
  });
});

describe("hardware and recommendation", () => {
  it("recommends Max on a big GPU, Fast on a small CPU-only PC", async () => {
    const gpu = createApi(new MockBackend({ hardware: "gpu" }));
    expect((await gpu.probeHardware()).backend).toBe("cuda");
    expect((await gpu.getRecommendation()).modelId).toBe(MAX_ID);

    const cpu = createApi(new MockBackend({ hardware: "cpu" }));
    const rec = await cpu.getRecommendation();
    expect(rec.modelId).toBe(FAST_ID);
    expect(rec.fits.find((f) => f.modelId === ACCURATE_ID)?.badge).toBe("slow");
    expect(rec.fits.find((f) => f.modelId === MAX_ID)?.badge).toBe("wontFit");
    expect(rec.fits.find((f) => f.modelId === DS2_ID)?.badge).toBe("hidden");
  });
});

describe("queue", () => {
  it("converts the converting file page by page and then writes its outputs", async () => {
    const { backend, api, queueEvents } = make({ scenario: "demo", pageMs: 1000 });
    const before = (await api.queueList()).find((q) => q.name.startsWith("Statistical"))!;
    expect(before).toMatchObject({ status: "converting", pagesDone: 30, pageCount: 48 });

    backend.advance(1000);
    const one = (await api.queueList()).find((q) => q.fileId === before.fileId)!;
    expect(one.pagesDone).toBe(31);
    const pageEvent = queueEvents.find((e) => e.page?.index === 30);
    expect(pageEvent?.page).toMatchObject({ state: "done" });
    expect(pageEvent?.item.fileId).toBe(before.fileId);

    backend.advance(18_000);
    const finished = (await api.queueList()).find((q) => q.fileId === before.fileId)!;
    expect(finished.status).toBe("done");
    expect(finished.durationMs).toBeGreaterThan(0);
    expect(finished.outputs.map((o) => o.format)).toEqual(["docx", "markdown"]);
  });

  it("walks the stages inside a page", async () => {
    const { backend, queueEvents } = make({ scenario: "demo", pageMs: 1200 });
    backend.advance(1100);
    const stages = new Set(queueEvents.map((e) => e.item.stage));
    for (const stage of ["render", "layout", "recognize", "crops"] as const) {
      expect(stages.has(stage)).toBe(true);
    }
  });

  it("adds files, inspects them in the background and flags a PDF with text", async () => {
    const { backend, api } = make({ scenario: "ready-empty" });
    const added = await api.queueAdd([
      "C:\\Scans\\plain.pdf",
      "C:\\Scans\\Quarterly report.pdf",
      "C:\\Scans\\photo.jpg",
      "C:\\Scans\\notes.txt",
    ]);
    expect(added.map((a) => a.name)).toEqual(["plain.pdf", "Quarterly report.pdf", "photo.jpg"]);
    expect(added.every((a) => a.status === "waiting" && a.pageCount === null)).toBe(true);

    backend.advance(600);
    const list = await api.queueList();
    expect(list.find((q) => q.name === "plain.pdf")).toMatchObject({ status: "waiting" });
    expect(list.find((q) => q.name === "plain.pdf")!.pageCount).toBeGreaterThan(0);
    expect(list.find((q) => q.name === "Quarterly report.pdf")).toMatchObject({
      status: "needsAttention",
      attention: "hasTextLayer",
      hasTextLayer: true,
    });
  });

  it("rejects a batch of unsupported files and does not add a path twice", async () => {
    const { api } = make({ scenario: "ready-empty" });
    const error = await rejection(api.queueAdd(["C:\\x\\readme.txt"]));
    expect(error).toMatchObject({ code: "badInput", reason: "unsupported_format" });
    await api.queueAdd(["C:\\x\\one.png"]);
    expect((await rejection(api.queueAdd(["C:\\x\\one.png"]))).code).toBe("badInput");
  });

  it("fails a damaged or password protected file when it is inspected", async () => {
    const { backend, api } = make({ scenario: "ready-empty" });
    await api.queueAdd(["C:\\x\\corrupt.pdf", "C:\\x\\password.pdf"]);
    backend.advance(600);
    const list = await api.queueList();
    expect(list[0]!.error).toMatchObject({ code: "badInput", reason: "corrupt" });
    expect(list[1]!.error).toMatchObject({ code: "badInput", reason: "password_protected" });
    await api.queueRetry(list[0]!.fileId);
    expect((await api.queueList())[0]!.status).toBe("failed");
  });

  it("starts the waiting files in order after queue_start", async () => {
    const { backend, api } = make({ scenario: "ready-empty", pageMs: 500 });
    await api.queueAdd(["C:\\x\\a.png", "C:\\x\\b.png"]);
    backend.advance(600);
    await api.queueStart({
      formats: ["markdown"],
      modelId: ACCURATE_ID,
      saveTo: { kind: "original" },
    });
    let [a, b] = await api.queueList();
    expect(a).toMatchObject({ status: "converting", modelId: ACCURATE_ID, formats: ["markdown"] });
    expect(b).toMatchObject({ status: "waiting", modelId: ACCURATE_ID });
    backend.advance(2000);
    [a, b] = await api.queueList();
    expect(a!.status).toBe("done");
    expect(b!.status).toBe("done");
    expect(a!.outputs.map((o) => o.format)).toEqual(["markdown"]);
  });

  it("refuses to start with a model that is not installed", async () => {
    const { backend, api } = make({ scenario: "ready-empty" });
    await api.queueAdd(["C:\\x\\a.png"]);
    backend.advance(600);
    const error = await rejection(
      api.queueStart({ formats: ["docx"], modelId: MAX_ID, saveTo: { kind: "original" } }),
    );
    expect(error.code).toBe("modelNotReady");
  });

  it("pauses and resumes the converting file", async () => {
    const { backend, api } = make({ scenario: "demo", pageMs: 1000 });
    await api.queuePause();
    const paused = (await api.queueList()).find((q) => q.status === "paused")!;
    expect(paused).toBeDefined();
    backend.advance(5000);
    expect((await api.queueList()).find((q) => q.fileId === paused.fileId)!.pagesDone).toBe(30);
    await api.queueResume();
    backend.advance(1000);
    expect((await api.queueList()).find((q) => q.fileId === paused.fileId)!.pagesDone).toBe(31);
  });

  it("cancels everything that is running or waiting, and retry adds it again", async () => {
    const { api } = make({ scenario: "demo" });
    await api.queueCancel(null);
    const list = await api.queueList();
    expect(list.filter((q) => q.status === "cancelled").map((q) => q.name)).toEqual([
      "Statistical abstract 1920.pdf",
      "calculus-notes-p77.jpg",
      "receipt.heic",
    ]);
    const cancelled = list.find((q) => q.status === "cancelled")!;
    await api.queueRetry(cancelled.fileId);
    expect((await api.queueList()).find((q) => q.fileId === cancelled.fileId)!.status).toBe(
      "waiting",
    );
  });

  it("answers the text-layer notice by moving the file to waiting", async () => {
    const { api } = make({ scenario: "demo" });
    const attention = (await api.queueList()).find((q) => q.status === "needsAttention")!;
    await api.queueSetTextMode(attention.fileId, "existingText");
    const after = (await api.queueList()).find((q) => q.fileId === attention.fileId)!;
    expect(after).toMatchObject({ status: "waiting", textMode: "existingText", attention: null });
  });

  it("removes one file, and clears done and cancelled ones", async () => {
    const { api } = make({ scenario: "demo" });
    const list = await api.queueList();
    const waiting = list.find((q) => q.name === "receipt.heic")!;
    await api.queueRemove(waiting.fileId);
    const converting = list.find((q) => q.status === "converting")!;
    expect((await rejection(api.queueRemove(converting.fileId))).code).toBe("invalidArgument");
    const cleared = await api.queueClearDone();
    expect(cleared).toHaveLength(1);
    expect((await api.queueList()).some((q) => q.status === "done")).toBe(false);
  });

  it("runs out of memory once on a huge file and succeeds on retry", async () => {
    const { backend, api } = make({ scenario: "ready-empty", pageMs: 400 });
    await api.queueAdd(["C:\\x\\huge-atlas.pdf"]);
    backend.advance(600);
    await api.queueStart({ formats: ["docx"], modelId: ACCURATE_ID, saveTo: { kind: "original" } });
    backend.advance(3000);
    const failed = (await api.queueList())[0]!;
    expect(failed.status).toBe("failed");
    expect(failed.error?.code).toBe("outOfMemory");
    await api.queueRetry(failed.fileId);
    backend.advance(10_000);
    expect((await api.queueList())[0]!.status).toBe("done");
  });
});

describe("review and export", () => {
  it("lists the pages and returns the IR page of a finished file", async () => {
    const { api } = make({ scenario: "demo" });
    const done = (await api.queueList()).find((q) => q.status === "done")!;
    const pages = await api.getPages(done.fileId);
    expect(pages).toHaveLength(3);
    expect(pages.every((p) => p.state === "done" && p.imagePath)).toBe(true);

    const review = await api.getReview(done.fileId, 1);
    expect(review.pageCount).toBe(3);
    expect(review.imagePath).toBe(review.page.sourceImage);
    expect(review.page.blocks.some((b) => b.type === "table")).toBe(true);
    const table = review.page.blocks.find((b) => b.type === "table")!.table!;
    expect(table.cells.some((c) => c.colSpan === 2)).toBe(true);
    expect(table.cells.some((c) => c.rowSpan === 2)).toBe(true);
    const formulas = (await api.getReview(done.fileId, 2)).page.blocks.filter(
      (b) => b.type === "formula",
    );
    expect(formulas.map((f) => f.number)).toEqual(["(1)", "(2)"]);
  });

  it("has no result for a page that is not converted yet", async () => {
    const { api } = make({ scenario: "demo" });
    const converting = (await api.queueList()).find((q) => q.status === "converting")!;
    expect((await rejection(api.getReview(converting.fileId, 40))).code).toBe("notFound");
    expect((await api.getReview(converting.fileId, 3)).pageIndex).toBe(3);
  });

  it("never overwrites an export", async () => {
    const { api } = make({ scenario: "demo" });
    const done = (await api.queueList()).find((q) => q.status === "done")!;
    const first = await api.exportFile(done.fileId, "docx");
    const second = await api.exportFile(done.fileId, "docx");
    expect(first.path).toBe("C:\\Users\\Demo\\Documents\\Library scans (2).docx");
    expect(second.path).toBe("C:\\Users\\Demo\\Documents\\Library scans (3).docx");
    expect((await api.exportFile(done.fileId, "markdown")).assetsDir).toBe(
      "C:\\Users\\Demo\\Documents\\Library scans (2)_assets",
    );
  });

  it("saves a copy to the exact path and refuses to overwrite unless told to", async () => {
    const { backend, api } = make({ scenario: "demo" });
    const done = (await api.queueList()).find((q) => q.status === "done")!;
    const dest = "D:\\Archive\\copy.docx";
    expect((await api.exportTo(done.fileId, "docx", dest, false)).path).toBe(dest);
    expect((await rejection(api.exportTo(done.fileId, "docx", dest, false))).reason).toBe(
      "already_exists",
    );
    await api.exportTo(done.fileId, "docx", dest, true);
    expect(backend.savedCopies).toHaveLength(2);
    expect(
      (await rejection(api.exportTo(done.fileId, "docx", "D:\\Archive\\copy.md", true))).reason,
    ).toBe("bad_extension");
    // A copy is not one of the conversion outputs.
    expect((await api.queueList()).find((q) => q.fileId === done.fileId)!.outputs).toHaveLength(2);
  });

  it("opens only saved outputs, and reports the ones that are gone", async () => {
    const { backend, api } = make({ scenario: "demo" });
    const done = (await api.queueList()).find((q) => q.status === "done")!;
    const docx = done.outputs[0]!.path;
    await api.openOutput(docx);
    expect(backend.openedOutputs).toEqual([docx]);
    expect((await rejection(api.openOutput("C:\\Windows\\notepad.exe"))).reason).toBe(
      "not_an_output",
    );
    backend.markOutputsGone("docx");
    expect((await rejection(api.openOutput(docx))).reason).toBe("file_missing");
    expect((await api.checkOutputs(done.fileId)).map((o) => o.exists)).toEqual([false, true]);
  });

  it("records the folder a reveal was asked for", async () => {
    const { backend, api } = make();
    await api.revealInFolder("C:\\x\\a.docx");
    expect(backend.revealed).toEqual(["C:\\x\\a.docx"]);
  });
});

describe("settings", () => {
  it("merges a patch and leaves the other fields alone", async () => {
    const { api } = make();
    const next = await api.settingsSet({ theme: "dark", experimentalVulkan: true });
    expect(next).toMatchObject({ theme: "dark", experimentalVulkan: true, checkForUpdates: true });
    expect(await api.settingsGet()).toEqual(next);
  });

  it("only changes the models folder while no model is installed", async () => {
    expect(
      (await rejection(make({ scenario: "ready-empty" }).api.settingsSet({ modelsDir: "D:\\m" })))
        .code,
    ).toBe("invalidArgument");
    const fresh = make({ scenario: "fresh" });
    expect((await fresh.api.settingsSet({ modelsDir: "D:\\m" })).modelsDir).toBe("D:\\m");
    expect((await fresh.api.setupStatus()).modelsDir).toBe("D:\\m");
  });
});
