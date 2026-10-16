import { describe, expect, it } from "vitest";
import { t, type MessageKey } from "./index";

describe("t", () => {
  it("returns the string for a known key", () => {
    expect(t("queue.empty")).toBe("No files in the queue.");
  });

  it("fills placeholders from vars", () => {
    expect(t("action.convertCount", { count: 3 })).toBe("Convert 3 files");
  });

  it("returns the key itself when the key is missing", () => {
    expect(t("does.not.exist" as MessageKey)).toBe("does.not.exist");
  });

  it("leaves a placeholder in place when its variable is not given", () => {
    expect(t("action.convertCount", {})).toBe("Convert {count} files");
  });
});
