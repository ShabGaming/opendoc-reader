import { describe, expect, it } from "vitest";
import en from "./en.json";
import { t, tp, type MessageKey } from "./index";

describe("t", () => {
  it("returns the string for a known key", () => {
    expect(t("queue.empty")).toBe("No files in the queue.");
  });

  it("fills placeholders from vars", () => {
    expect(t("action.convertCount.other", { count: 3 })).toBe("Convert 3 files");
  });

  it("returns the key itself when the key is missing", () => {
    expect(t("does.not.exist" as MessageKey)).toBe("does.not.exist");
  });

  it("leaves a placeholder in place when its variable is not given", () => {
    expect(t("action.convertCount.other", {})).toBe("Convert {count} files");
  });
});

describe("tp", () => {
  it("uses the singular form for one", () => {
    expect(tp("action.convertCount", 1)).toBe("Convert 1 file");
    expect(tp("queue.count", 1)).toBe("1 file");
  });

  it("uses the plural form for zero and for many", () => {
    expect(tp("action.convertCount", 3)).toBe("Convert 3 files");
    expect(tp("queue.count", 0)).toBe("0 files");
  });
});

describe("en.json", () => {
  it("has an other form for every one form", () => {
    const keys = Object.keys(en);
    for (const key of keys.filter((k) => k.endsWith(".one"))) {
      expect(keys).toContain(key.replace(/\.one$/, ".other"));
    }
  });

  it("uses no emoji and no exclamation marks", () => {
    for (const text of Object.values(en)) {
      expect(text).not.toMatch(/!/);
      expect(text).not.toMatch(/\p{Extended_Pictographic}/u);
    }
  });
});
