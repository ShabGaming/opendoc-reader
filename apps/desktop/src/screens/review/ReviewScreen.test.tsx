import { act, fireEvent, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MockBackend, appError } from "../../api/mock/backend";
import { renderApp } from "../../test/renderApp";

async function openReview(page?: number, view?: "original" | "split" | "result") {
  const app = await renderApp({
    dev: { screen: "review", page: page ?? null, view: view ?? null },
  });
  await screen.findByRole("group", { name: "View" });
  // The page itself loads after the toolbar is on screen.
  await screen.findByRole("region", { name: "Result text" });
  await app.advance(0);
  return app;
}

const resultPane = () => screen.getByRole("region", { name: "Result text" });
const originalPane = () => screen.getByRole("region", { name: "Original page" });
const block = (container: HTMLElement, id: string) =>
  container.querySelector<HTMLElement>(`[data-block-id="${id}"]`)!;
const overlay = (id: string) =>
  originalPane().querySelector<HTMLElement>(`[data-overlay-id="${id}"]`)!;

describe("Review layout", () => {
  it("opens side by side with the pager, zoom, Blocks and export controls", async () => {
    await openReview();
    expect(screen.getByRole("button", { name: "Queue" })).toBeInTheDocument();
    expect(screen.getByText("Library scans.pdf", { selector: "span[title]" })).toBeInTheDocument();
    const view = screen.getByRole("group", { name: "View" });
    expect(within(view).getByRole("button", { name: "Side by side" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByText("Page 1 of 3")).toBeInTheDocument();
    expect(screen.getByText("100%")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Blocks" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "Export Word" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Export Markdown" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Show in folder" })).toBeEnabled();
    // The pane headers name what is shown and which model produced it.
    expect(screen.getByText("Accurate (PaddleOCR-VL 1.6)")).toBeInTheDocument();
  });

  it("switches between Original, Side by side and Result", async () => {
    const { user } = await openReview();
    const view = screen.getByRole("group", { name: "View" });
    await user.click(within(view).getByRole("button", { name: "Result" }));
    expect(screen.queryByRole("region", { name: "Original page" })).not.toBeInTheDocument();
    expect(resultPane()).toBeInTheDocument();
    // Zoom belongs to the original, so it is off in Result.
    expect(screen.getByRole("button", { name: "Zoom in" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Zoom out" })).toBeDisabled();

    await user.click(within(view).getByRole("button", { name: "Original" }));
    expect(originalPane()).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Result text" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Zoom in" })).toBeEnabled();

    await user.click(within(view).getByRole("button", { name: "Side by side" }));
    expect(originalPane()).toBeInTheDocument();
    expect(resultPane()).toBeInTheDocument();
  });

  it("zooms the original from 50 to 200 percent in steps of 25", async () => {
    const { user } = await openReview();
    const scan = () => originalPane().querySelector<HTMLElement>("img")!.parentElement!;
    await user.click(screen.getByRole("button", { name: "Zoom in" }));
    expect(screen.getByText("125%")).toBeInTheDocument();
    expect(scan().style.width).toBe("125%");
    for (let i = 0; i < 3; i++) await user.click(screen.getByRole("button", { name: "Zoom in" }));
    expect(screen.getByText("200%")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Zoom in" })).toBeDisabled();
    for (let i = 0; i < 6; i++) await user.click(screen.getByRole("button", { name: "Zoom out" }));
    expect(screen.getByText("50%")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Zoom out" })).toBeDisabled();
  });

  it("pages with the buttons, the thumbnails and the arrow keys", async () => {
    const { user } = await openReview();
    expect(screen.getByRole("button", { name: "Previous page" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Next page" }));
    expect(await screen.findByText("Page 2 of 3")).toBeInTheDocument();
    expect(await screen.findByText("Lending by branch")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Page 3" }));
    expect(await screen.findByText("Rates of use")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Page 3" })).toHaveAttribute("aria-current", "true");
    expect(screen.getByRole("button", { name: "Next page" })).toBeDisabled();

    await user.keyboard("{ArrowLeft}");
    expect(await screen.findByText("Page 2 of 3")).toBeInTheDocument();
    await user.keyboard("{PageUp}");
    expect(await screen.findByText("Page 1 of 3")).toBeInTheDocument();
    await user.keyboard("{ArrowLeft}");
    expect(screen.getByText("Page 1 of 3")).toBeInTheDocument();
  });

  it("returns to the queue with the Queue button and with Escape", async () => {
    const { user } = await openReview();
    await user.click(screen.getByRole("button", { name: "Queue" }));
    expect(screen.queryByRole("group", { name: "View" })).not.toBeInTheDocument();
    expect(screen.getByText("Finished in 14 s")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Review" }));
    await screen.findByRole("region", { name: "Result text" });
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("group", { name: "View" })).not.toBeInTheDocument();
  });

  it("keeps the queue beside Review and leaves it for a file that is not done", async () => {
    const { user } = await openReview();
    const queue = screen.getByRole("complementary", { name: "File queue" });
    expect(within(queue).getByRole("button", { name: /Library scans/ })).toHaveAttribute(
      "aria-current",
      "true",
    );
    await user.click(within(queue).getByRole("button", { name: /calculus-notes/ }));
    expect(screen.queryByRole("group", { name: "View" })).not.toBeInTheDocument();
  });
});

describe("Result rendering", () => {
  it("builds the page from the IR: heading, emphasis, ordered and nested lists, figure, footnote", async () => {
    await openReview();
    const result = resultPane();
    expect(within(result).getByRole("heading", { name: "The reading room" })).toBeInTheDocument();
    expect(within(result).getByText("two hundred").tagName).toBe("STRONG");
    expect(within(result).getByText("nine thousand").tagName).toBe("EM");

    const lists = result.querySelectorAll("ol, ul");
    expect(lists).toHaveLength(2);
    expect(lists[0]!.tagName).toBe("OL");
    expect(within(lists[0] as HTMLElement).getAllByRole("listitem")).toHaveLength(5);
    expect(lists[0]!.querySelector("ul")).not.toBeNull();

    const figure = within(result).getByRole("img", { name: "Figure cut out of the page" });
    expect(figure.getAttribute("src")).toMatch(/^data:image\/svg\+xml/);
    expect(within(result).getByText("Figure 1. The reading room in 1919.")).toBeInTheDocument();
    expect(
      within(result).getByText("1. Figures are taken from the minutes of the board."),
    ).toBeInTheDocument();
  });

  it("renders a table with merged cells inside a horizontally scrolling wrapper", async () => {
    await openReview(2);
    await screen.findByText("Lending by branch");
    const table = within(resultPane()).getByRole("table");
    expect(table.parentElement!.className).toMatch(/tableScroll/);

    const branch = within(table).getByRole("columnheader", { name: "Branch" });
    expect(branch).toHaveAttribute("rowspan", "2");
    const lent = within(table).getByRole("columnheader", { name: "Books lent" });
    expect(lent).toHaveAttribute("colspan", "2");
    expect(within(table).getByRole("columnheader", { name: "Change" })).toHaveAttribute(
      "rowspan",
      "2",
    );

    // The two header rows sit in the head, the data in the body, and each row is complete.
    const head = table.querySelector("thead")!;
    expect(head.querySelectorAll("tr")).toHaveLength(2);
    expect(
      within(head as HTMLElement).getByRole("columnheader", { name: "1918" }),
    ).toBeInTheDocument();
    const rows = table.querySelectorAll("tbody tr");
    expect(rows).toHaveLength(3);
    expect(
      within(rows[0] as HTMLElement)
        .getAllByRole("cell")
        .map((c) => c.textContent),
    ).toEqual(["Central", "12,480", "13,102", "+5%"]);
    // Numbers are right aligned, words are not.
    const cells = within(rows[2] as HTMLElement).getAllByRole("cell");
    expect(cells[1]!.className).toMatch(/numeric/);
    expect(cells[0]!.className).not.toMatch(/numeric/);
    expect(within(resultPane()).getByText("Table 1. Books lent by branch.")).toBeInTheDocument();
  });

  it("renders formulas with KaTeX and shows the printed equation number beside them", async () => {
    await openReview(3);
    await screen.findByText("Rates of use");
    const result = resultPane();
    const formulas = result.querySelectorAll('[data-block-type="formula"]');
    expect(formulas).toHaveLength(2);
    for (const formula of formulas) {
      expect(formula.querySelector(".katex-display")).not.toBeNull();
      expect(formula.querySelector(".katex-mathml annotation")).not.toBeNull();
    }
    expect(formulas[0]!.querySelector("annotation")!.textContent).toBe(
      "\\int_0^1 x^2\\,dx = \\frac{1}{3}",
    );
    expect(within(formulas[0] as HTMLElement).getByText("(1)")).toBeInTheDocument();
    expect(within(formulas[1] as HTMLElement).getByText("(2)")).toBeInTheDocument();
    // Inline math in a paragraph is typeset too.
    const paragraph = result.querySelector('[data-block-id="p2-b3"]')!;
    expect(paragraph.querySelectorAll(".katex")).toHaveLength(2);
    expect(paragraph.textContent).toContain("readers each borrow");
  });

  it("draws a formula it cannot parse as source text instead of failing", async () => {
    const { renderMath } = await import("./math");
    expect(() => renderMath("\\badcommand{", false)).not.toThrow();
    expect(renderMath("\\badcommand{", false)).toContain("katex-error");
  });

  it("labels page furniture only while Blocks is on", async () => {
    const { user } = await openReview();
    const result = resultPane();
    expect(within(result).getByText("Running head")).toBeInTheDocument();
    expect(within(result).getByText("Page number")).toBeInTheDocument();
    expect(block(result, "p0-b0")).toHaveAttribute(
      "title",
      "Running head. Left out of the export.",
    );

    await user.click(screen.getByRole("button", { name: "Blocks" }));
    expect(screen.getByRole("button", { name: "Blocks" })).toHaveAttribute("aria-pressed", "false");
    expect(within(result).queryByText("Running head")).not.toBeInTheDocument();
    expect(within(result).queryByText("Page number")).not.toBeInTheDocument();
    // The text itself stays, and so do the overlays' absence checks below.
    expect(within(result).getByText("Town library annual report")).toBeInTheDocument();
    expect(originalPane().querySelectorAll("[data-overlay-id]")).toHaveLength(0);

    await user.click(screen.getByRole("button", { name: "Blocks" }));
    expect(originalPane().querySelectorAll("[data-overlay-id]")).toHaveLength(10);
  });

  it("gives every result block a name and a tooltip with its type", async () => {
    await openReview(2);
    await screen.findByText("Lending by branch");
    const table = block(resultPane(), "p1-b4");
    expect(table).toHaveAttribute("aria-label", "Table, 5 rows");
    expect(table).toHaveAttribute("title", "Table, 5 rows");
    expect(block(resultPane(), "p1-b3")).toHaveAttribute("aria-label", "Paragraph");
  });

  it("places an overlay for every block as a percentage of the page image", async () => {
    await openReview();
    const heading = overlay("p0-b2");
    // bbox [110, 190, 900, 250] on a 1240 x 1754 page.
    expect(parseFloat(heading.style.left)).toBeCloseTo((110 / 1240) * 100, 3);
    expect(parseFloat(heading.style.top)).toBeCloseTo((190 / 1754) * 100, 3);
    expect(parseFloat(heading.style.width)).toBeCloseTo((790 / 1240) * 100, 3);
    expect(parseFloat(heading.style.height)).toBeCloseTo((60 / 1754) * 100, 3);
  });
});

describe("Linking the scan and the result", () => {
  it("hovering a result block highlights its overlay, and the other way round", async () => {
    const { user } = await openReview();
    const result = resultPane();
    expect(overlay("p0-b3")).not.toHaveAttribute("data-active");

    await user.hover(block(result, "p0-b3"));
    expect(overlay("p0-b3")).toHaveAttribute("data-active", "true");
    expect(block(result, "p0-b3")).toHaveAttribute("data-active", "true");
    expect(overlay("p0-b4")).not.toHaveAttribute("data-active");

    await user.unhover(block(result, "p0-b3"));
    expect(overlay("p0-b3")).not.toHaveAttribute("data-active");

    await user.hover(overlay("p0-b4"));
    expect(block(result, "p0-b4")).toHaveAttribute("data-active", "true");
    expect(overlay("p0-b4")).toHaveAttribute("data-active", "true");
    await user.unhover(overlay("p0-b4"));
    expect(block(result, "p0-b4")).not.toHaveAttribute("data-active");
  });

  it("makes result blocks reachable by keyboard, and focus highlights the overlay", async () => {
    const { user } = await openReview();
    const result = resultPane();
    const first = block(result, "p0-b0");
    expect(first).toHaveAttribute("tabindex", "0");

    await user.click(screen.getByRole("button", { name: "Show in folder" }));
    await user.tab();
    // Tab order continues from the toolbar through the thumbnails to the first block.
    act(() => block(result, "p0-b3").focus());
    expect(overlay("p0-b3")).toHaveAttribute("data-active", "true");
    act(() => block(result, "p0-b4").focus());
    expect(overlay("p0-b3")).not.toHaveAttribute("data-active");
    expect(overlay("p0-b4")).toHaveAttribute("data-active", "true");
    act(() => block(result, "p0-b4").blur());
    expect(overlay("p0-b4")).not.toHaveAttribute("data-active");
  });

  it("clicking an overlay scrolls the result to its block, centred", async () => {
    const { user } = await openReview();
    const pane = resultPane().querySelector<HTMLElement>("div[class*=pane]")!;
    let top = 0;
    Object.defineProperty(pane, "scrollTop", {
      get: () => top,
      set: (v: number) => (top = v),
      configurable: true,
    });
    Object.defineProperty(pane, "clientHeight", { value: 400, configurable: true });
    pane.getBoundingClientRect = () => ({ top: 100, height: 400 }) as DOMRect;
    block(pane, "p0-b5").getBoundingClientRect = () => ({ top: 1100, height: 40 }) as DOMRect;

    await user.click(overlay("p0-b5"));
    // 1100 - 100 (pane top) - 200 (half the pane) + 20 (half the block)
    expect(top).toBe(820);
  });

  it("clicking a result block, or pressing Enter on it, scrolls the original to its box", async () => {
    const { user } = await openReview();
    const pane = originalPane().querySelector<HTMLElement>("div[class*=pane]")!;
    let top = 0;
    Object.defineProperty(pane, "scrollTop", {
      get: () => top,
      set: (v: number) => (top = v),
      configurable: true,
    });
    Object.defineProperty(pane, "clientHeight", { value: 600, configurable: true });
    pane.getBoundingClientRect = () => ({ top: 50, height: 600 }) as DOMRect;
    overlay("p0-b6").getBoundingClientRect = () => ({ top: 950, height: 100 }) as DOMRect;

    await user.click(block(resultPane(), "p0-b6"));
    // 950 - 50 - 300 + 50
    expect(top).toBe(650);

    top = 0;
    act(() => block(resultPane(), "p0-b6").focus());
    await user.keyboard("{Enter}");
    expect(top).toBe(650);
    top = 0;
    await user.keyboard(" ");
    expect(top).toBe(650);
  });

  it("keeps the two panes at the same fraction of their scroll height", async () => {
    await openReview();
    const orig = originalPane().querySelector<HTMLElement>("div[class*=pane]")!;
    const res = resultPane().querySelector<HTMLElement>("div[class*=pane]")!;
    const fake = (el: HTMLElement, scrollHeight: number, clientHeight: number) => {
      let top = 0;
      Object.defineProperty(el, "scrollHeight", { value: scrollHeight, configurable: true });
      Object.defineProperty(el, "clientHeight", { value: clientHeight, configurable: true });
      Object.defineProperty(el, "scrollTop", {
        get: () => top,
        set: (v: number) => (top = v),
        configurable: true,
      });
    };
    fake(orig, 2000, 500);
    fake(res, 1000, 400);

    orig.scrollTop = 750; // halfway through the original
    fireEvent.scroll(orig);
    expect(res.scrollTop).toBe(300); // halfway through the result
  });

  it("does not pull the other pane back right after a jump", async () => {
    const { user } = await openReview();
    const orig = originalPane().querySelector<HTMLElement>("div[class*=pane]")!;
    const res = resultPane().querySelector<HTMLElement>("div[class*=pane]")!;
    let resTop = 0;
    Object.defineProperty(res, "scrollTop", {
      get: () => resTop,
      set: (v: number) => (resTop = v),
      configurable: true,
    });
    Object.defineProperty(res, "scrollHeight", { value: 3000, configurable: true });
    Object.defineProperty(res, "clientHeight", { value: 400, configurable: true });
    Object.defineProperty(orig, "scrollHeight", { value: 2000, configurable: true });
    Object.defineProperty(orig, "clientHeight", { value: 500, configurable: true });

    await user.click(overlay("p0-b3"));
    const afterJump = resTop;
    fireEvent.scroll(orig); // the jump itself makes the original scroll in a real browser
    expect(resTop).toBe(afterJump);
  });
});

describe("Export and Show in folder", () => {
  it("exports Word and Markdown and says where the file went, never overwriting", async () => {
    const { user, backend } = await openReview();
    await user.click(screen.getByRole("button", { name: "Export Word" }));
    expect(
      await screen.findByText("Saved Library scans (2).docx next to the original."),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Export Markdown" }));
    expect(
      await screen.findByText("Saved Library scans (2).md next to the original."),
    ).toBeInTheDocument();
    const exports = backend.calls.filter((c) => c.command === "export").map((c) => c.args);
    expect(exports).toEqual([
      { fileId: expect.any(String), format: "docx" },
      { fileId: expect.any(String), format: "markdown" },
    ]);
  });

  it("shows the output folder in the file manager", async () => {
    const { user, backend } = await openReview();
    await user.click(screen.getByRole("button", { name: "Show in folder" }));
    expect(backend.revealed).toEqual(["C:\\Users\\Demo\\Documents\\Library scans.docx"]);
  });
});

describe("Review errors", () => {
  function failReview(code: "notFound" | "internal") {
    const original = MockBackend.prototype.invoke;
    vi.spyOn(MockBackend.prototype, "invoke").mockImplementation(function (
      this: MockBackend,
      command: never,
      args: never,
    ) {
      if (command === "get_review") return Promise.reject(appError(code));
      return original.call(this, command, args);
    } as never);
  }

  afterEach(() => vi.restoreAllMocks());

  it("says so when a page has no result yet", async () => {
    failReview("notFound");
    await renderApp({ dev: { screen: "review" } });
    const result = await screen.findByRole("region", { name: "Result text" });
    expect(await within(result).findByText("This page has no result yet.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Next page" })).toBeEnabled();
  });

  it("says so when a page cannot be loaded", async () => {
    failReview("internal");
    await renderApp({ dev: { screen: "review" } });
    const result = await screen.findByRole("region", { name: "Result text" });
    expect(await within(result).findByText("This page could not be loaded.")).toBeInTheDocument();
  });
});
