import list from "../../assets/contributors/contributors.json";

/** One entry of contributors.json, written at build time by scripts/fetch-contributors.mjs. */
export interface Contributor {
  login: string;
  name: string | null;
  profileUrl: string;
  contributions: number;
  /** File name in assets/contributors, null when there is none. */
  avatar: string | null;
}

// The avatars are bundled with the app, so no remote image is ever loaded.
const avatarFiles = import.meta.glob<string>("../../assets/contributors/*.png", {
  eager: true,
  query: "?url",
  import: "default",
});

export function avatarUrl(avatar: string | null): string | null {
  if (!avatar) return null;
  return avatarFiles[`../../assets/contributors/${avatar}`] ?? null;
}

/** Most contributions first, then by login. Entries without an https profile are dropped. */
export function sortContributors(entries: Contributor[]): Contributor[] {
  return entries
    .filter((c) => c.profileUrl.startsWith("https://"))
    .sort((a, b) => b.contributions - a.contributions || a.login.localeCompare(b.login));
}

export const bundledContributors: Contributor[] = sortContributors(list as Contributor[]);
