import { describe, expect, it } from "vitest";
import { avatarUrl, bundledContributors, sortContributors, type Contributor } from "./contributors";

const person = (login: string, contributions: number, profileUrl?: string): Contributor => ({
  login,
  name: null,
  profileUrl: profileUrl ?? `https://github.com/${login}`,
  contributions,
  avatar: null,
});

describe("contributors", () => {
  it("sorts by contributions, then login, and drops links that are not https", () => {
    const sorted = sortContributors([
      person("b", 3),
      person("a", 3),
      person("top", 9),
      person("bad", 50, "http://example.com/bad"),
      person("worse", 50, "javascript:alert(1)"),
    ]);
    expect(sorted.map((c) => c.login)).toEqual(["top", "a", "b"]);
  });

  it("resolves every bundled avatar to a local file", () => {
    expect(bundledContributors.length).toBeGreaterThan(0);
    for (const c of bundledContributors) {
      const url = avatarUrl(c.avatar);
      expect(url, c.login).toBeTruthy();
      expect(url).not.toMatch(/^https?:/);
    }
    expect(avatarUrl(null)).toBeNull();
    expect(avatarUrl("missing.png")).toBeNull();
  });
});
