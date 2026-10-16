import { screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ACCURATE_ID, MAX_ID } from "../../api/mock/catalog";
import { renderApp } from "../../test/renderApp";

describe("Setup", () => {
  it("walks through the four steps and ends in Main", async () => {
    const { user, advance, backend } = await renderApp({ scenario: "fresh" });

    // 1. Welcome
    expect(screen.getByText("Step 1 of 4")).toBeInTheDocument();
    expect(
      screen.getByText("Everything runs on this computer. Nothing is uploaded."),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Get started" }));

    // 2. Your computer: the live probe fills the rows in.
    expect(screen.getByRole("heading", { name: "Your computer" })).toBeInTheDocument();
    expect(await screen.findByText("NVIDIA GeForce RTX 4090 (24 GB)")).toBeInTheDocument();
    expect(screen.getByText("64 GB")).toBeInTheDocument();
    expect(screen.getByText(/AMD Ryzen 7 7800X3D/)).toBeInTheDocument();
    expect(screen.getByText("396 GB free on G:")).toBeInTheDocument();
    expect(screen.getByText("This computer can run every model.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Continue" }));

    // 3. Choose a model: the suggester's pick is ticked and tagged.
    expect(screen.getByRole("heading", { name: "Choose a model" })).toBeInTheDocument();
    const max = screen.getByRole("checkbox", { name: /^Max/ });
    expect(max).toBeChecked();
    expect(screen.getByText("Recommended")).toBeInTheDocument();
    expect(
      screen.getByText("Your NVIDIA GeForce RTX 4090 (24 GB) runs Max on the GPU."),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Download 4.4 GB" })).toBeEnabled();
    expect(screen.getByText("Model 3.6 GB, runtime and engine 790 MB")).toBeInTheDocument();

    await user.click(max);
    expect(screen.getByRole("button", { name: "Choose a model" })).toBeDisabled();
    expect(screen.getByText("No model selected")).toBeInTheDocument();
    await user.click(screen.getByRole("checkbox", { name: /^Accurate/ }));
    await user.click(screen.getByRole("button", { name: "Download 2.7 GB" }));

    // 4. Download
    expect(screen.getByRole("heading", { name: "Downloading" })).toBeInTheDocument();
    await advance(1000);
    const bars = screen.getAllByRole("progressbar");
    expect(bars).toHaveLength(3);
    expect(bars[0]).toHaveAttribute("aria-valuenow");
    expect(screen.getByRole("button", { name: "Start using OpenDoc Reader" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Back" })).toBeDisabled();

    await user.click(screen.getByRole("button", { name: "Pause" }));
    expect(screen.getByRole("button", { name: "Resume" })).toBeInTheDocument();
    expect(screen.getAllByText("Paused").length).toBeGreaterThan(0);
    await user.click(screen.getByRole("button", { name: "Resume" }));

    await advance(5000);
    expect(screen.getByText("Testing speed on a sample page...")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start using OpenDoc Reader" })).toBeDisabled();

    await advance(3000);
    expect(screen.getByText("About 4.8 s per page on this computer.")).toBeInTheDocument();
    expect(backend.calls.some((c) => c.command === "start_setup_download")).toBe(true);
    const start = screen.getByRole("button", { name: "Start using OpenDoc Reader" });
    expect(start).toBeEnabled();
    await user.click(start);
    expect(await screen.findByRole("complementary", { name: "File queue" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Drop PDFs or images here" })).toBeInTheDocument();
  });

  it("sends the chosen model to the download", async () => {
    const { user, backend } = await renderApp({ scenario: "fresh" });
    await user.click(screen.getByRole("button", { name: "Get started" }));
    await screen.findByText("NVIDIA GeForce RTX 4090 (24 GB)");
    await user.click(screen.getByRole("button", { name: "Continue" }));
    await user.click(screen.getByRole("checkbox", { name: /^Accurate/ }));
    await user.click(screen.getByRole("button", { name: /^Download/ }));
    const call = backend.calls.find((c) => c.command === "start_setup_download");
    expect(call?.args).toEqual({ modelIds: [MAX_ID, ACCURATE_ID] });
  });

  it("goes back to completed steps from the rail", async () => {
    const { user } = await renderApp({ scenario: "fresh" });
    await user.click(screen.getByRole("button", { name: "Get started" }));
    await screen.findByText("NVIDIA GeForce RTX 4090 (24 GB)");
    await user.click(screen.getByRole("button", { name: "Continue" }));

    const nav = screen.getByRole("navigation", { name: "Setup steps" });
    expect(within(nav).getByRole("button", { name: /Choose a model/ })).toHaveAttribute(
      "aria-current",
      "step",
    );
    expect(within(nav).getByRole("button", { name: /Download/ })).toBeDisabled();
    await user.click(within(nav).getByRole("button", { name: /Welcome/ }));
    expect(screen.getByRole("heading", { name: "Welcome to OpenDoc Reader" })).toBeInTheDocument();
  });

  it("marks models that will not fit and does not pre-check them", async () => {
    const { user } = await renderApp({ scenario: "fresh", hardware: "cpu" });
    await user.click(screen.getByRole("button", { name: "Get started" }));
    expect(
      await screen.findByText("No supported graphics card found. The processor is used instead."),
    ).toBeInTheDocument();
    expect(screen.getByText("Some models won't fit on this computer.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Continue" }));

    expect(screen.getByRole("checkbox", { name: /^Fast/ })).toBeChecked();
    expect(screen.getByText("Won't fit")).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: /^Max/ })).toBeDisabled();
    expect(screen.getByText("Slow on this PC")).toBeInTheDocument();
    // An estimate that was not measured says "about".
    expect(screen.getAllByText(/^about \d/).length).toBeGreaterThan(0);
  });

  it("opens the model details popover and closes it with Escape", async () => {
    const { user } = await renderApp({ scenario: "fresh" });
    await user.click(screen.getByRole("button", { name: "Get started" }));
    await screen.findByText("NVIDIA GeForce RTX 4090 (24 GB)");
    await user.click(screen.getByRole("button", { name: "Continue" }));

    const buttons = screen.getAllByRole("button", { name: "Model details" });
    await user.click(buttons[1]!);
    expect(buttons[1]).toHaveAttribute("aria-expanded", "true");
    const dialog = screen.getByRole("dialog", { name: "Accurate details" });
    expect(within(dialog).getByText("Apache-2.0")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Accurate details" })).not.toBeInTheDocument();
    expect(buttons[1]).toHaveAttribute("aria-expanded", "false");
  });

  it("shows the advanced model and the fit legend only when asked", async () => {
    const { user } = await renderApp({ scenario: "fresh" });
    await user.click(screen.getByRole("button", { name: "Get started" }));
    await screen.findByText("NVIDIA GeForce RTX 4090 (24 GB)");
    await user.click(screen.getByRole("button", { name: "Continue" }));

    expect(screen.queryByText("How fit is shown")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Advanced models" }));
    expect(screen.getByText("How fit is shown")).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: /DeepSeek-OCR 2/ })).not.toBeChecked();
    expect(screen.getByText("Experimental")).toBeInTheDocument();
    expect(screen.getByText("Community conversion")).toBeInTheDocument();
  });

  it("resumes at the download when the app starts while one is running", async () => {
    await renderApp({ scenario: "downloading" });
    expect(screen.getByRole("heading", { name: "Downloading" })).toBeInTheDocument();
    expect(screen.getAllByRole("progressbar").length).toBeGreaterThan(0);
  });
});

describe("Setup errors", () => {
  async function failWith(code: "offline" | "diskFull" | "checksumMismatch") {
    const app = await renderApp({ scenario: "fresh", failDownload: code });
    await app.user.click(screen.getByRole("button", { name: "Get started" }));
    await screen.findByText("NVIDIA GeForce RTX 4090 (24 GB)");
    await app.user.click(screen.getByRole("button", { name: "Continue" }));
    await app.user.click(screen.getByRole("checkbox", { name: /^Max/ }));
    await app.user.click(screen.getByRole("checkbox", { name: /^Accurate/ }));
    await app.user.click(screen.getByRole("button", { name: /^Download/ }));
    await app.advance(6000);
    return app;
  }

  it.each([
    ["offline", /could not reach the internet/],
    ["diskFull", /not enough free space on G:/],
    ["checksumMismatch", /did not match its checksum/],
  ] as const)("explains a %s failure and retries", async (code, message) => {
    const { user, advance } = await failWith(code);
    const alert = screen.getByRole("alert");
    expect(within(alert).getByText("The download stopped.")).toBeInTheDocument();
    expect(within(alert).getByText(message)).toBeInTheDocument();
    // The failure is the only reason to go back.
    expect(screen.getByRole("button", { name: "Back" })).toBeEnabled();

    await user.click(within(alert).getByRole("button", { name: "Retry" }));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    await advance(8000);
    expect(screen.getByText(/per page on this computer/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start using OpenDoc Reader" })).toBeEnabled();
  });

  it("keeps the models that were ticked when a download is retried", async () => {
    const { user, backend } = await failWith("offline");
    await user.click(screen.getByRole("button", { name: "Retry" }));
    const calls = backend.calls.filter((c) => c.command === "start_setup_download");
    expect(calls).toHaveLength(2);
    expect(calls[1]?.args).toEqual({ modelIds: [ACCURATE_ID] });
  });
});
