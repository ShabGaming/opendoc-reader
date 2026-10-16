import { act, fireEvent, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ACCURATE_ID, MAX_ID } from "../../api/mock/catalog";
import { renderApp } from "../../test/renderApp";

function row(name: string | RegExp) {
  return within(screen.getByRole("complementary", { name: "File queue" })).getByRole("button", {
    name,
  });
}

describe("done detail", () => {
  it("falls back when the saved files were moved", async () => {
    await renderApp({ dev: { missing: "all" } });
    expect(await screen.findByText(/Moved or deleted/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Open in Word" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save a copy as..." })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Review" })).toBeInTheDocument();
  });
});

describe("queue rows", () => {
  it("shows every state with an icon and a word, and the percentage while converting", async () => {
    await renderApp();
    const queue = screen.getByRole("complementary", { name: "File queue" });
    expect(within(queue).getByText("6 files")).toBeInTheDocument();

    expect(row(/Library scans/)).toHaveTextContent("Done");
    expect(row(/Statistical abstract/)).toHaveTextContent("63%");
    expect(row(/Statistical abstract/)).toHaveTextContent("Converting");
    expect(row(/calculus-notes/)).toHaveTextContent("Waiting");
    expect(row(/Invoice/)).toHaveTextContent("Needs attention");
    expect(row(/Large atlas/)).toHaveTextContent("Failed");
  });

  it("marks the selected file as current", async () => {
    const { user } = await renderApp();
    expect(row(/Library scans/)).toHaveAttribute("aria-current", "true");
    await user.click(row(/receipt/));
    expect(row(/receipt/)).toHaveAttribute("aria-current", "true");
    expect(row(/Library scans/)).not.toHaveAttribute("aria-current");
  });

  it("updates as the mock converts the file, and says so politely when the status changes", async () => {
    const { advance } = await renderApp({ pageMs: 1000 });
    await advance(1000);
    expect(row(/Statistical abstract/)).toHaveTextContent("65%");
    await advance(18_000);
    expect(row(/Statistical abstract/)).toHaveTextContent("Done");
    const live = document.querySelector('[aria-live="polite"][aria-atomic="true"]');
    expect(live).toHaveTextContent("Statistical abstract 1920.pdf: Done");
  });

  it("moves through the list with the arrow keys and opens a finished file with Enter", async () => {
    const { user } = await renderApp();
    row(/Library scans/).focus();
    await user.keyboard("{ArrowDown}");
    expect(row(/Statistical abstract/)).toHaveFocus();
    await user.keyboard("{End}");
    expect(row(/Large atlas/)).toHaveFocus();
    await user.keyboard("{Home}");
    expect(row(/Library scans/)).toHaveFocus();

    // One row is in the tab order at a time.
    const tabbable = screen
      .getAllByRole("button")
      .filter((b) => b.getAttribute("tabindex") === "0" && b.closest("ul"));
    expect(tabbable).toHaveLength(1);

    await user.keyboard("{Enter}");
    expect(screen.getByRole("group", { name: "View" })).toBeInTheDocument();
  });

  it("selects an unfinished file on Enter instead of opening Review", async () => {
    const { user } = await renderApp();
    row(/calculus-notes/).focus();
    await user.keyboard("{Enter}");
    expect(row(/calculus-notes/)).toHaveAttribute("aria-current", "true");
    expect(screen.queryByRole("group", { name: "View" })).not.toBeInTheDocument();
  });

  it("opens Review on a double click of a finished file", async () => {
    const { user } = await renderApp();
    await user.dblClick(row(/Library scans/));
    expect(await screen.findByRole("group", { name: "View" })).toBeInTheDocument();
  });

  it("clears finished and cancelled files with Clear done", async () => {
    const { user } = await renderApp();
    await user.click(screen.getByRole("button", { name: "Clear done" }));
    expect(screen.queryByRole("button", { name: /Library scans/ })).not.toBeInTheDocument();
    expect(screen.getByText("5 files")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Clear done" })).toBeDisabled();
  });
});

describe("detail view per state", () => {
  it("converting: page, progress, time left and the queue controls", async () => {
    const { user, backend } = await renderApp();
    await user.click(row(/Statistical abstract/));
    expect(
      screen.getByRole("heading", { name: "Statistical abstract 1920.pdf" }),
    ).toBeInTheDocument();
    expect(screen.getByText("48 pages · 41.8 MB")).toBeInTheDocument();
    expect(screen.getByText("Converting page 31 of 48")).toBeInTheDocument();
    const bar = screen.getByRole("progressbar", { name: "Conversion progress" });
    expect(bar).toHaveAttribute("aria-valuenow", "63");
    expect(screen.getByText(/^About 1 min 26 s left$/)).toBeInTheDocument();

    const pause = screen.getAllByRole("button", { name: "Pause all" });
    expect(pause).toHaveLength(2);
    await user.click(pause[0]!);
    expect(backend.calls.some((c) => c.command === "queue_pause")).toBe(true);
    expect((await screen.findAllByRole("button", { name: "Resume all" })).length).toBe(2);
    expect(screen.getByText("Press Resume all to continue.")).toBeInTheDocument();
  });

  it("done: Open in Word, Open Markdown, Show in folder and Review", async () => {
    const { user, backend } = await renderApp();
    expect(screen.getByText("Finished in 14 s")).toBeInTheDocument();
    expect(screen.getByText("Saved next to the original:")).toBeInTheDocument();
    expect(screen.getByText("Library scans.docx, Library scans.md")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Open in Word" }));
    await user.click(screen.getByRole("button", { name: "Open Markdown" }));
    expect(backend.openedOutputs).toEqual([
      "C:\\Users\\Demo\\Documents\\Library scans.docx",
      "C:\\Users\\Demo\\Documents\\Library scans.md",
    ]);
    await user.click(screen.getByRole("button", { name: "Show in folder" }));
    expect(backend.revealed).toEqual(["C:\\Users\\Demo\\Documents\\Library scans.docx"]);

    await user.click(screen.getByRole("button", { name: "Review" }));
    expect(screen.getByRole("group", { name: "View" })).toBeInTheDocument();
  });

  it("waiting: a file nobody has started says what pressing Convert does", async () => {
    const { user } = await renderApp();
    await user.click(row(/calculus-notes/));
    expect(screen.getByText("Ready to convert")).toBeInTheDocument();
    expect(
      screen.getByText("Press Convert to start it with the options below."),
    ).toBeInTheDocument();
    expect(screen.getByText("JPEG image")).toBeInTheDocument();
  });

  it("cancelled: Add again puts the file back in the queue", async () => {
    const { user } = await renderApp();
    await user.click(screen.getAllByRole("button", { name: "Cancel all" })[0]!);
    await user.click(row(/Statistical abstract/));
    expect(await screen.findByText("The pages that were finished are kept.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Add again" }));
    expect(await screen.findByText("Waiting in the queue")).toBeInTheDocument();
  });

  it("removes a file from the queue", async () => {
    const { user } = await renderApp();
    await user.click(row(/receipt/));
    await user.click(screen.getByRole("button", { name: "Remove from queue" }));
    expect(screen.queryByRole("button", { name: /receipt/ })).not.toBeInTheDocument();
    expect(screen.getByText("5 files")).toBeInTheDocument();
  });
});

describe("inline notice for a PDF that already has text", () => {
  async function openInvoice() {
    const app = await renderApp();
    await app.user.click(row(/Invoice/));
    return app;
  }

  it("explains the choice and offers both answers", async () => {
    await openInvoice();
    expect(screen.getByText("This PDF already contains text.")).toBeInTheDocument();
    expect(
      screen.getByText(
        "Using the existing text is instant and exact. Running OCR reads the page images instead.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Use existing text" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Run OCR anyway" })).toBeInTheDocument();
  });

  it("Use existing text sets that mode, confirms with a toast and moves the file on", async () => {
    const { user, backend } = await openInvoice();
    await user.click(screen.getByRole("button", { name: "Use existing text" }));
    const call = backend.calls.find((c) => c.command === "queue_set_text_mode");
    expect(call?.args).toMatchObject({ mode: "existingText" });
    expect(await screen.findByText("Ready to convert")).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("Used the text already in the PDF.");
    expect(screen.queryByText("This PDF already contains text.")).not.toBeInTheDocument();
  });

  it("Run OCR anyway sets the OCR mode without a toast", async () => {
    const { user, backend } = await openInvoice();
    await user.click(screen.getByRole("button", { name: "Run OCR anyway" }));
    const call = backend.calls.find((c) => c.command === "queue_set_text_mode");
    expect(call?.args).toMatchObject({ mode: "ocr" });
    expect(await screen.findByText("Ready to convert")).toBeInTheDocument();
    expect(screen.getByRole("status")).toBeEmptyDOMElement();
  });
});

describe("failed files (error states)", () => {
  it("out of memory: says what happened, suggests a smaller model and retries", async () => {
    const { user, backend } = await renderApp();
    await user.click(row(/Large atlas/));
    const alert = screen.getByRole("alert");
    expect(
      within(alert).getByText("The computer ran out of memory while reading this file."),
    ).toBeInTheDocument();
    expect(within(alert).getByText("A smaller model needs less memory.")).toBeInTheDocument();
    await user.click(within(alert).getByRole("button", { name: "Retry" }));
    expect(backend.calls.some((c) => c.command === "queue_retry")).toBe(true);
    expect(await screen.findByText("Waiting in the queue")).toBeInTheDocument();
  });

  it("offers to add the file again with an installed smaller model", async () => {
    const { user, backend, api, advance } = await renderApp({
      scenario: "ready-empty",
      pageMs: 400,
      platform: { files: ["C:\\x\\huge-atlas.pdf"] },
    });
    await api.modelsInstall(MAX_ID);
    await advance(9000);
    await user.click(screen.getByRole("button", { name: "or choose files..." }));
    await advance(600);
    expect(screen.getByRole("combobox", { name: "Model" })).toHaveValue(MAX_ID);
    await user.click(screen.getByRole("button", { name: "Convert 1 file" }));
    await advance(3000);

    const alert = await screen.findByRole("alert");
    expect(within(alert).getByText("A smaller model needs less memory.")).toBeInTheDocument();
    await user.click(within(alert).getByRole("button", { name: "Add again with Accurate" }));
    await advance(600);
    expect(await screen.findByText("Ready to convert")).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Model" })).toHaveValue(ACCURATE_ID);
    expect(backend.calls.filter((c) => c.command === "queue_add")).toHaveLength(2);
  });

  it("damaged and password protected files cannot be retried", async () => {
    const { user, backend } = await renderApp({
      scenario: "ready-empty",
      platform: { files: ["C:\\Scans\\corrupt.pdf", "C:\\Scans\\password.pdf"] },
    });
    await user.click(screen.getByRole("button", { name: "or choose files..." }));
    await act(async () => backend.advance(600));

    await user.click(row(/corrupt/));
    expect(screen.getByText("The file looks damaged and could not be opened.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Retry" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Remove from queue" })).toBeInTheDocument();

    await user.click(row(/password/));
    expect(
      screen.getByText("This PDF is password protected. Remove the password, then add it again."),
    ).toBeInTheDocument();
  });
});

describe("action bar", () => {
  it("counts the files the next press will start and sends the batch options", async () => {
    const { user, backend } = await renderApp();
    const convert = screen.getByRole("button", { name: "Convert 2 files" });
    await user.click(screen.getByRole("checkbox", { name: "Word" }));
    await user.click(convert);

    const call = backend.calls.find((c) => c.command === "queue_start");
    expect(call?.args).toEqual({
      options: { formats: ["markdown"], modelId: ACCURATE_ID, saveTo: { kind: "original" } },
    });
    expect(screen.getByRole("status")).toHaveTextContent("2 files are queued for conversion.");
    // Nothing is left to start, and the label falls back.
    expect(await screen.findByRole("button", { name: "Convert" })).toBeDisabled();
  });

  it("lists a model that is not installed as unavailable", async () => {
    await renderApp();
    const select = screen.getByRole("combobox", { name: "Model" });
    const max = within(select).getByRole("option", { name: "Max (not installed)" });
    expect(max).toBeDisabled();
    expect(within(select).getByRole("option", { name: "Accurate" })).not.toBeDisabled();
  });

  it("picks a save folder from the picker, and goes back to the original folder", async () => {
    const { user, backend } = await renderApp({ platform: { folder: "D:\\Converted" } });
    const select = screen.getByRole("combobox", { name: "Save to" });
    await user.selectOptions(select, "Choose folder...");
    expect(within(select).getByRole("option", { name: "D:\\Converted" })).toBeInTheDocument();
    expect(select).toHaveValue("folder");

    await user.click(screen.getByRole("button", { name: "Convert 2 files" }));
    const call = backend.calls.find((c) => c.command === "queue_start");
    expect(call?.args).toMatchObject({
      options: { saveTo: { kind: "folder", path: "D:\\Converted" } },
    });

    await user.selectOptions(select, "Next to the original");
    expect(select).toHaveValue("original");
  });

  it("keeps the choice when the picker is cancelled", async () => {
    const { user } = await renderApp({ platform: { folder: null } });
    const select = screen.getByRole("combobox", { name: "Save to" });
    await user.selectOptions(select, "Choose folder...");
    expect(select).toHaveValue("original");
  });

  it("disables Convert when no model is installed", async () => {
    const { user, api } = await renderApp({ scenario: "ready-empty" });
    await api.modelsRemove(ACCURATE_ID);
    await user.click(screen.getByRole("button", { name: "Add files" }));
    expect(await screen.findByRole("button", { name: "Convert" })).toBeDisabled();
    expect(screen.getByRole("combobox", { name: "Model" })).toBeDisabled();
  });
});

describe("adding files", () => {
  it("adds the files from the picker and says how many", async () => {
    const { user, backend } = await renderApp({ scenario: "ready-empty" });
    await user.click(screen.getByRole("button", { name: "or choose files..." }));
    expect(await screen.findByRole("button", { name: /Meeting notes/ })).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("Added 2 files to the queue.");
    expect(backend.calls.find((c) => c.command === "queue_add")?.args).toEqual({
      paths: [
        "C:\\Users\\Demo\\Documents\\Meeting notes.pdf",
        "C:\\Users\\Demo\\Documents\\scan-014.png",
      ],
    });
  });

  it("shows the files while they are inspected, then Convert counts them", async () => {
    const { user, advance } = await renderApp({ scenario: "ready-empty" });
    await user.click(screen.getByRole("button", { name: "Add files" }));
    expect(await screen.findByText("Reading the file...")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Convert" })).toBeDisabled();
    await advance(600);
    expect(screen.getByRole("button", { name: "Convert 2 files" })).toBeEnabled();
  });

  it("tells the user when none of the files can be converted", async () => {
    const { user } = await renderApp({
      scenario: "ready-empty",
      platform: { files: ["C:\\x\\readme.txt"] },
    });
    await user.click(screen.getByRole("button", { name: "or choose files..." }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "None of those files can be converted.",
    );
    await user.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("adds files dropped on the window and shows the drop state while dragging", async () => {
    await renderApp({ scenario: "ready-empty" });
    const files = [{ name: "dropped.png" }];
    const drag = (type: string) => {
      const event = new Event(type, { bubbles: true, cancelable: true });
      Object.defineProperty(event, "dataTransfer", { value: { types: ["Files"], files } });
      return event;
    };

    await act(async () => {
      window.dispatchEvent(drag("dragenter"));
    });
    expect(screen.getByRole("region", { name: "Drop PDFs or images here" })).toHaveAttribute(
      "data-over",
      "true",
    );

    await act(async () => {
      window.dispatchEvent(drag("drop"));
    });
    expect(await screen.findByRole("button", { name: /dropped\.png/ })).toBeInTheDocument();
  });

  it("shows an overlay over the detail view while files are dragged over a filled queue", async () => {
    await renderApp();
    const event = new Event("dragenter", { bubbles: true, cancelable: true });
    Object.defineProperty(event, "dataTransfer", { value: { types: ["Files"], files: [] } });
    await act(async () => {
      fireEvent(window, event);
    });
    expect(screen.getByText("Drop files to add them to the queue")).toBeInTheDocument();
  });
});
