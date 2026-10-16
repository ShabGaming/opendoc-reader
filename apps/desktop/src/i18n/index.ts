import en from "./en.json";

export type MessageKey = keyof typeof en;

/** Keys that have `.one` and `.other` forms, for example `queue.count`. */
export type PluralKey = {
  [K in MessageKey]: K extends `${infer Base}.one` ? Base : never;
}[MessageKey];

const messages: Record<string, string> = en;

/**
 * Looks up a UI string and fills `{name}` placeholders from `vars`.
 * An unknown key returns the key itself, so a missing string is visible
 * on screen instead of rendering as blank.
 */
export function t(key: MessageKey, vars?: Record<string, string | number>): string {
  const template = messages[key];
  if (template === undefined) return key;
  if (!vars) return template;
  return template.replace(/\{(\w+)\}/g, (match, name: string) =>
    name in vars ? String(vars[name]) : match,
  );
}

/**
 * Picks `<key>.one` for a count of 1 and `<key>.other` otherwise, and fills `{count}`. Plural
 * forms live in the strings, not in the layout.
 */
export function tp(key: PluralKey, count: number, vars?: Record<string, string | number>): string {
  const form = count === 1 ? "one" : "other";
  return t(`${key}.${form}` as MessageKey, { count, ...vars });
}
