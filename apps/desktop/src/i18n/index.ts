import en from "./en.json";

export type MessageKey = keyof typeof en;

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
