import { afterEach, describe, expect, it } from "vitest";
import { createRuntime, parseDevParams } from "./runtime";

afterEach(() => {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});

describe("parseDevParams", () => {
  it("reads the mock switches and ignores values it does not know", () => {
    const dev = parseDevParams(
      "?mock=1&screen=review&theme=dark&step=3&section=models&page=2&view=result&t=2500&freeze=1&hw=mac&fail=offline&file=Library",
    );
    expect(dev).toEqual({
      mock: true,
      screen: "review",
      theme: "dark",
      step: 3,
      section: "models",
      page: 2,
      view: "result",
      advanceMs: 2500,
      freeze: true,
      hardware: "mac",
      fail: "offline",
      file: "Library",
    });
    const odd = parseDevParams("?screen=nope&theme=pink&step=x&hw=toaster&view=3d");
    expect(odd).toMatchObject({
      mock: false,
      screen: null,
      theme: null,
      step: null,
      hardware: null,
      view: null,
      freeze: false,
      advanceMs: 0,
    });
  });
});

describe("createRuntime", () => {
  it("uses the mock outside Tauri", async () => {
    const runtime = createRuntime("?freeze=1");
    expect(runtime.mock).not.toBeNull();
    expect(runtime.platform.kind).toBe("mock");
    expect((await runtime.api.setupStatus()).phase).toBe("ready");
  });

  it("uses the mock inside Tauri when the URL says mock=1", () => {
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    expect(createRuntime("?mock=1&freeze=1").mock).not.toBeNull();
  });

  it("uses the real core inside Tauri", () => {
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    const runtime = createRuntime("");
    expect(runtime.mock).toBeNull();
    expect(runtime.platform.kind).toBe("tauri");
  });

  it("opens the scenario the screen parameter asks for", async () => {
    const setup = createRuntime("?screen=setup&freeze=1");
    expect((await setup.api.setupStatus()).phase).toBe("notStarted");
    const download = createRuntime("?screen=setup&step=4&t=2000&freeze=1");
    expect((await download.api.setupStatus()).phase).toBe("downloading");
    const empty = createRuntime("?screen=empty&freeze=1");
    expect(await empty.api.queueList()).toEqual([]);
    const demo = createRuntime("?screen=main&freeze=1");
    expect((await demo.api.queueList()).length).toBeGreaterThan(3);
    const themed = createRuntime("?theme=dark&freeze=1");
    expect((await themed.api.settingsGet()).theme).toBe("dark");
  });
});
