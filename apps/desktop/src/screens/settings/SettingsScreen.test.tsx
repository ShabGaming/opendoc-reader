import { act, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { ACCURATE_ID, DS2_ID, MAX_ID } from "../../api/mock/catalog";
import { renderApp } from "../../test/renderApp";

async function openSettings(
  section: "general" | "models" | "privacy" | "about" = "general",
  options = {},
) {
  const app = await renderApp({ ...options, dev: { screen: "settings", section } });
  await screen.findByRole("heading", { level: 1 });
  return app;
}

afterEach(() => {
  document.documentElement.removeAttribute("data-theme");
});

describe("Settings navigation", () => {
  it("lists the four sections and marks the current one", async () => {
    const { user } = await openSettings();
    const nav = screen.getByRole("navigation", { name: "Settings sections" });
    expect(within(nav).getByRole("button", { name: "General" })).toHaveAttribute(
      "aria-current",
      "true",
    );
    await user.click(within(nav).getByRole("button", { name: "Privacy and logs" }));
    expect(screen.getByRole("heading", { name: "Privacy and logs" })).toBeInTheDocument();
    expect(within(nav).getByRole("button", { name: "Privacy and logs" })).toHaveAttribute(
      "aria-current",
      "true",
    );
    expect(within(nav).getByRole("button", { name: "General" })).not.toHaveAttribute(
      "aria-current",
    );
  });

  it("goes back to Setup when it was opened from there", async () => {
    const { user } = await renderApp({ scenario: "fresh" });
    await user.click(screen.getByRole("button", { name: "Settings" }));
    expect(screen.getByRole("heading", { name: "General" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Back" }));
    expect(screen.getByRole("heading", { name: "Welcome to OpenDoc Reader" })).toBeInTheDocument();
  });
});

describe("General", () => {
  it("keeps at least one default format and saves the change at once", async () => {
    const { user, backend } = await openSettings();
    const word = screen.getByRole("checkbox", { name: "Word" });
    const markdown = screen.getByRole("checkbox", { name: "Markdown" });
    await user.click(word);
    expect(word).not.toBeChecked();
    await user.click(markdown);
    expect(markdown).toBeChecked();
    const patches = backend.calls.filter((c) => c.command === "settings_set").map((c) => c.args);
    expect(patches).toEqual([{ patch: { defaultFormats: ["markdown"] } }]);
  });

  it("switches the theme and applies it to the page", async () => {
    const { user } = await openSettings();
    await user.selectOptions(screen.getByRole("combobox", { name: "Theme" }), "Dark");
    expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
    await user.selectOptions(screen.getByRole("combobox", { name: "Theme" }), "System");
    expect(document.documentElement.hasAttribute("data-theme")).toBe(false);
  });

  it("has the experimental Vulkan switch, off by default, with its warning", async () => {
    const { user, backend } = await openSettings();
    const vulkan = screen.getByRole("switch", { name: /Use Vulkan for AMD and Intel graphics/ });
    expect(vulkan).toHaveAttribute("aria-checked", "false");
    expect(screen.getAllByText("Experimental").length).toBeGreaterThan(0);
    expect(screen.getByText(/Experimental and untested/)).toBeInTheDocument();
    await user.click(vulkan);
    expect(vulkan).toHaveAttribute("aria-checked", "true");
    expect(backend.calls.find((c) => c.command === "settings_set")?.args).toEqual({
      patch: { experimentalVulkan: true },
    });
  });

  it("toggles the monthly update check", async () => {
    const { user } = await openSettings();
    const updates = screen.getByRole("switch", { name: "Check for new versions once a month" });
    expect(updates).toHaveAttribute("aria-checked", "true");
    await user.click(updates);
    expect(updates).toHaveAttribute("aria-checked", "false");
  });

  it("saves into a chosen folder and back to the original folder", async () => {
    const { user, backend } = await openSettings("general", { platform: { folder: "D:\\Out" } });
    await user.click(screen.getByRole("button", { name: "Change..." }));
    expect(await screen.findByText("D:\\Out")).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: /Always this folder/ })).toBeChecked();
    expect(backend.calls.find((c) => c.command === "settings_set")?.args).toEqual({
      patch: { saveTo: { kind: "folder", path: "D:\\Out" } },
    });
    await user.click(screen.getByRole("radio", { name: "Next to the original" }));
    expect(screen.getByRole("radio", { name: "Next to the original" })).toBeChecked();
  });

  it("offers only installed models as the default", async () => {
    await openSettings();
    const select = screen.getByRole("combobox", { name: "Default model" });
    expect(
      within(select)
        .getAllByRole("option")
        .map((o) => o.textContent),
    ).toEqual(["Accurate"]);
  });
});

describe("Models", () => {
  it("lists installed and available models with size, tier and license", async () => {
    await openSettings("models");
    const tables = screen.getAllByRole("table");
    expect(tables).toHaveLength(2);
    const installed = within(tables[0]!);
    expect(installed.getByRole("cell", { name: "PaddleOCR-VL 1.6" })).toBeInTheDocument();
    expect(installed.getByText("1.9 GB")).toBeInTheDocument();
    expect(installed.getByText("Apache-2.0")).toBeInTheDocument();
    expect(installed.getByRole("button", { name: "Remove Accurate" })).toBeInTheDocument();
    const available = within(tables[1]!);
    expect(available.getByRole("button", { name: "Install Fast" })).toBeInTheDocument();
    expect(available.getByRole("button", { name: "Install Max" })).toBeInTheDocument();
    expect(
      screen.getByText("The folder can only be changed while no model is installed."),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Change..." })).toBeDisabled();
  });

  it("asks before removing, puts focus on Cancel, and Escape cancels", async () => {
    const { user, backend } = await openSettings("models");
    const remove = screen.getByRole("button", { name: "Remove Accurate" });
    await user.click(remove);

    const dialog = screen.getByRole("dialog", { name: "Remove Accurate?" });
    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(
      within(dialog).getByText("This frees 1.9 GB on G:. You can install it again later."),
    ).toBeInTheDocument();
    const cancel = within(dialog).getByRole("button", { name: "Cancel" });
    expect(cancel).toHaveFocus();

    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    // Focus goes back to the button that opened it, and nothing was removed.
    expect(remove).toHaveFocus();
    expect(backend.calls.some((c) => c.command === "models_remove")).toBe(false);
  });

  it("keeps Tab inside the dialog", async () => {
    const { user } = await openSettings("models");
    await user.click(screen.getByRole("button", { name: "Remove Accurate" }));
    const dialog = screen.getByRole("dialog");
    const cancel = within(dialog).getByRole("button", { name: "Cancel" });
    const confirm = within(dialog).getByRole("button", { name: "Remove" });
    expect(cancel).toHaveFocus();
    await user.tab();
    expect(confirm).toHaveFocus();
    await user.tab();
    expect(cancel).toHaveFocus();
    await user.tab({ shift: true });
    expect(confirm).toHaveFocus();
  });

  it("Cancel leaves the model installed", async () => {
    const { user, backend } = await openSettings("models");
    await user.click(screen.getByRole("button", { name: "Remove Accurate" }));
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(backend.calls.some((c) => c.command === "models_remove")).toBe(false);
    expect(screen.getByRole("button", { name: "Remove Accurate" })).toBeInTheDocument();
  });

  it("keeps the model and says why when a conversion is using it", async () => {
    const { user } = await openSettings("models");
    await user.click(screen.getByRole("button", { name: "Remove Accurate" }));
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Remove" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "The model could not be removed. It may be in use.",
    );
    expect(screen.getByRole("button", { name: "Remove Accurate" })).toBeInTheDocument();
  });

  it("removes the model after the confirmation and moves it to Available", async () => {
    const { user, backend } = await openSettings("models", { scenario: "ready-empty" });
    await user.click(screen.getByRole("button", { name: "Remove Accurate" }));
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Remove" }));
    expect(backend.calls.find((c) => c.command === "models_remove")?.args).toEqual({
      modelId: ACCURATE_ID,
    });
    expect(await screen.findByText("No models installed.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Install Accurate" })).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("Accurate was removed.");
    // With nothing installed the folder can be changed again.
    expect(screen.getByRole("button", { name: "Change..." })).toBeEnabled();
  });

  it("installs a model with progress and confirms with a toast", async () => {
    const { user, advance } = await openSettings("models");
    await user.click(screen.getByRole("button", { name: "Install Fast" }));
    expect(await screen.findByText("Installing...")).toBeInTheDocument();
    expect(screen.getByRole("progressbar", { name: "Fast download progress" })).toBeInTheDocument();
    await advance(8000);
    expect(await screen.findByRole("button", { name: "Remove Fast" })).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("Fast is installed.");
  });

  it("explains a download that stopped and retries the same model", async () => {
    const { user, advance } = await openSettings("models", {
      scenario: "ready-empty",
      failDownload: "offline",
    });
    await user.click(screen.getByRole("button", { name: "Install Max" }));
    await advance(9000);
    const alert = await screen.findByRole("alert");
    expect(within(alert).getByText("The download stopped.")).toBeInTheDocument();
    expect(within(alert).getByText(/could not reach the internet/)).toBeInTheDocument();

    await user.click(within(alert).getByRole("button", { name: "Retry" }));
    await advance(12_000);
    expect(await screen.findByRole("button", { name: "Remove Max" })).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("disables Install for a model that will not fit this computer", async () => {
    await openSettings("models", { hardware: "cpu" });
    expect(await screen.findByRole("button", { name: "Install Max" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Install Fast" })).toBeEnabled();
  });

  it("imports a model from a folder and reports files that do not match", async () => {
    const { user, backend } = await openSettings("models", {
      platform: { folder: "D:\\corrupt-copy" },
    });
    await user.click(screen.getByRole("button", { name: "Import from folder..." }));
    expect(backend.calls.find((c) => c.command === "models_import")?.args).toMatchObject({
      folder: "D:\\corrupt-copy",
    });
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "The files in that folder do not match the expected checksums.",
    );
  });

  it("lists the experimental model under Advanced models with its tags", async () => {
    const { user, backend } = await openSettings("models");
    expect(screen.getByRole("heading", { name: "Advanced models" })).toBeInTheDocument();
    expect(screen.getByText("Community conversion")).toBeInTheDocument();
    expect(
      screen.getByText(
        "3.6 GB, Apache-2.0. Needs about 7 GB of graphics memory. No quality advantage over Max was measured.",
      ),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Install DeepSeek-OCR 2" }));
    expect(backend.calls.find((c) => c.command === "models_install")?.args).toEqual({
      modelId: DS2_ID,
    });
    expect(MAX_ID).toBeDefined();
  });
});

describe("Privacy and logs, About", () => {
  it("states that there is no telemetry and opens the logs folder", async () => {
    const { user, backend } = await openSettings("privacy");
    expect(screen.getByText("OpenDoc Reader has no telemetry.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Open logs folder" }));
    expect(backend.calls.some((c) => c.command === "open_logs_folder")).toBe(true);
  });

  it("copies the diagnostics and shows Copied for a moment", async () => {
    const { user, platform } = await openSettings("privacy");
    await user.click(screen.getByRole("button", { name: "Copy diagnostics" }));
    expect(platform.copied[0]).toMatch(/^OpenDoc Reader 0\.1\.0/);
    expect(await screen.findByText("Copied")).toBeInTheDocument();
  });

  it("shows the version and opens the source and licenses links through the host", async () => {
    const { user, platform } = await openSettings("about");
    expect(await screen.findByText("Version 0.1.0 (pre-alpha)")).toBeInTheDocument();
    await user.click(screen.getByRole("link", { name: "View licenses" }));
    await user.click(
      screen.getByRole("link", { name: /github\.com\/ShabGaming\/opendoc-reader$/ }),
    );
    await act(async () => undefined);
    expect(platform.opened).toEqual([
      "https://github.com/ShabGaming/opendoc-reader/blob/main/LICENSE",
      "https://github.com/ShabGaming/opendoc-reader",
    ]);
  });
});
