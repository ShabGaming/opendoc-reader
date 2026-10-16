import { t } from "../i18n";
import type { ModelInfo, QueueItem, Tier } from "../types/commands";
import type { AppState } from "./reducer";

const TIER_ORDER: Tier[] = ["fast", "accurate", "max"];

/** Waiting items the next Convert press would start. */
export function convertibleItems(queue: QueueItem[]): QueueItem[] {
  return queue.filter((q) => q.status === "waiting" && q.modelId === null && q.pageCount !== null);
}

export function hasStatus(queue: QueueItem[], ...statuses: QueueItem["status"][]): boolean {
  return queue.some((q) => statuses.includes(q.status));
}

export function installedModels(models: ModelInfo[]): ModelInfo[] {
  return models.filter((m) => m.state === "installed");
}

export function selectedItem(state: AppState): QueueItem | null {
  return state.queue.find((q) => q.fileId === state.selectedId) ?? null;
}

/** The model the action bar starts with: the saved default, else the biggest installed model. */
export function preferredModelId(state: AppState): string | null {
  const installed = installedModels(state.models);
  const saved = state.settings?.defaultModelId;
  if (saved && installed.some((m) => m.id === saved)) return saved;
  const standard = installed.filter((m) => !m.experimental);
  const pool = standard.length > 0 ? standard : installed;
  const best = [...pool].sort((a, b) => TIER_ORDER.indexOf(b.tier) - TIER_ORDER.indexOf(a.tier))[0];
  return best?.id ?? null;
}

/** The model the action bar currently shows, falling back when the choice is not installed. */
export function effectiveModelId(state: AppState): string | null {
  const installed = installedModels(state.models);
  if (state.choice.modelId && installed.some((m) => m.id === state.choice.modelId)) {
    return state.choice.modelId;
  }
  return preferredModelId(state);
}

/** An installed model of a lower tier than `modelId`, the biggest one, or null. */
export function smallerInstalledModel(
  models: ModelInfo[],
  modelId: string | null,
): ModelInfo | null {
  const current = models.find((m) => m.id === modelId);
  if (!current) return null;
  const rank = TIER_ORDER.indexOf(current.tier);
  const lower = installedModels(models)
    .filter((m) => TIER_ORDER.indexOf(m.tier) < rank)
    .sort((a, b) => TIER_ORDER.indexOf(b.tier) - TIER_ORDER.indexOf(a.tier));
  return lower[0] ?? null;
}

/** "Accurate" for the standard models, the product name for experimental ones. */
export function modelShortName(model: ModelInfo): string {
  if (!model.experimental) return t(`tier.${model.tier}`);
  return model.displayName.replace(/\s*\(.*\)\s*$/, "");
}

export function percentDone(item: QueueItem): number {
  if (!item.pageCount) return 0;
  return Math.min(100, Math.round((item.pagesDone / item.pageCount) * 100));
}
