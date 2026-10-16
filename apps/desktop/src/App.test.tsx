import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "./App";

describe("App shell", () => {
  it("shows the empty queue and the drop target", () => {
    render(<App />);
    expect(screen.getByText("No files in the queue.")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Drop PDFs or images here" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "or choose files..." })).toBeInTheDocument();
  });

  it("keeps the Convert button disabled with an empty queue", () => {
    render(<App />);
    expect(screen.getByRole("button", { name: "Convert" })).toBeDisabled();
  });

  it("keeps at least one output format checked", async () => {
    const user = userEvent.setup();
    render(<App />);
    const word = screen.getByRole("checkbox", { name: "Word" });
    const markdown = screen.getByRole("checkbox", { name: "Markdown" });

    await user.click(word);
    expect(word).not.toBeChecked();

    await user.click(markdown);
    expect(markdown).toBeChecked();
  });
});
