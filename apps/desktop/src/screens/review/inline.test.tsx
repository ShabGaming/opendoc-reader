import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { createApi } from "../../api/api";
import { MockBackend } from "../../api/mock/backend";
import { createMockPlatform } from "../../api/mock/platform";
import { AppProvider } from "../../state/AppContext";
import { InlineText } from "./inline";

function show(text: string) {
  const platform = createMockPlatform({ openWindow: false });
  const utils = render(
    <AppProvider api={createApi(new MockBackend())} platform={platform}>
      <p data-testid="out">
        <InlineText text={text} />
      </p>
    </AppProvider>,
  );
  return { platform, out: screen.getByTestId("out"), ...utils };
}

describe("InlineText", () => {
  it("renders emphasis, code and plain text", () => {
    const { out } = show("a **bold** and *italic* word with `code`");
    expect(out.querySelector("strong")?.textContent).toBe("bold");
    expect(out.querySelector("em")?.textContent).toBe("italic");
    expect(out.querySelector("code")?.textContent).toBe("code");
    expect(out.textContent).toBe("a bold and italic word with code");
  });

  it("renders inline math with KaTeX", () => {
    const { out } = show("the value $x^2$ grows");
    expect(out.querySelector(".katex")).not.toBeNull();
    expect(out.querySelector("annotation")?.textContent).toBe("x^2");
  });

  it("never turns text into markup", () => {
    const { out } = show('<img src=x onerror="alert(1)"> and <b>no</b>');
    expect(out.querySelector("img")).toBeNull();
    expect(out.querySelector("b")).toBeNull();
    expect(out.textContent).toContain("<b>no</b>");
  });

  it("opens web links through the host and drops other schemes", async () => {
    const user = userEvent.setup();
    const { out, platform } = show("[site](https://example.org/a) and [bad](javascript:alert(1))");
    const link = out.querySelector("a")!;
    expect(link.textContent).toBe("site");
    await user.click(link);
    expect(platform.opened).toEqual(["https://example.org/a"]);
    expect(out.querySelectorAll("a")).toHaveLength(1);
    expect(out.textContent).toContain("bad");
  });
});
