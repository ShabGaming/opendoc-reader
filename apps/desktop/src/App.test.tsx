import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { renderApp } from "./test/renderApp";

describe("App shell", () => {
  it("shows Setup when no model is installed yet", async () => {
    await renderApp({ scenario: "fresh" });
    expect(screen.getByRole("heading", { name: "Welcome to OpenDoc Reader" })).toBeInTheDocument();
    expect(screen.queryByRole("complementary", { name: "File queue" })).not.toBeInTheDocument();
  });

  it("shows Main with the queue once setup is done", async () => {
    await renderApp({ scenario: "ready-empty" });
    expect(screen.getByRole("complementary", { name: "File queue" })).toBeInTheDocument();
    expect(screen.getByText("No files in the queue.")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Drop PDFs or images here" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "or choose files..." })).toBeInTheDocument();
  });

  it("keeps the Convert button disabled with an empty queue", async () => {
    await renderApp({ scenario: "ready-empty" });
    expect(screen.getByRole("button", { name: "Convert" })).toBeDisabled();
  });

  it("keeps at least one output format checked", async () => {
    const { user } = await renderApp({ scenario: "ready-empty" });
    const word = screen.getByRole("checkbox", { name: "Word" });
    const markdown = screen.getByRole("checkbox", { name: "Markdown" });

    await user.click(word);
    expect(word).not.toBeChecked();

    await user.click(markdown);
    expect(markdown).toBeChecked();
  });

  it("opens Settings from the sidebar and returns with Back", async () => {
    const { user } = await renderApp({ scenario: "ready-empty" });
    await user.click(screen.getByRole("button", { name: "Settings" }));
    expect(screen.getByRole("heading", { name: "General" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Back" }));
    expect(screen.getByRole("complementary", { name: "File queue" })).toBeInTheDocument();
  });

  it("opens Settings with Ctrl+comma and adds files with Ctrl+O", async () => {
    const { user, backend } = await renderApp({ scenario: "ready-empty" });
    await user.keyboard("{Control>}o{/Control}");
    expect(backend.calls.some((c) => c.command === "queue_add")).toBe(true);
    await user.keyboard("{Control>},{/Control}");
    expect(screen.getByRole("heading", { name: "General" })).toBeInTheDocument();
  });

  it("reports a file picker that cannot open instead of failing silently", async () => {
    const { user, platform } = await renderApp({ scenario: "ready-empty" });
    platform.pickFiles = () => Promise.reject(new Error("no dialog plugin"));
    await user.click(screen.getByRole("button", { name: "or choose files..." }));
    expect(await screen.findByRole("alert")).toHaveTextContent("The file picker could not open.");
  });

  it("shows a start-up failure when the core cannot be reached", async () => {
    const { MockBackend } = await import("./api/mock/backend");
    const original = MockBackend.prototype.invoke;
    MockBackend.prototype.invoke = () => Promise.reject(new Error("no core"));
    try {
      await renderApp({ scenario: "ready-empty" });
      expect(screen.getByRole("alert")).toHaveTextContent("OpenDoc Reader could not start.");
    } finally {
      MockBackend.prototype.invoke = original;
    }
  });
});
