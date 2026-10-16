import { useCallback, useEffect, useMemo, useState } from "react";
import type { MenuItem } from "../components/MenuButton";
import { t } from "../i18n";
import type { OutputFile, OutputStatus, QueueItem } from "../types/commands";
import { fileName } from "../util/format";
import { useApp } from "./AppContext";

export interface OutputsState {
  /** The saved Word file, only while it is still on disk. */
  word: OutputFile | null;
  /** The saved Markdown file, only while it is still on disk. */
  markdown: OutputFile | null;
  /** One sentence for the screen when a saved file has been moved or deleted, else null. */
  notice: string | null;
  /** What "Show in folder" selects: a saved file that is still there, else the original. Null before anything was saved. */
  revealPath: string | null;
  saveMenu: MenuItem[];
  open(output: OutputFile): Promise<void>;
}

/**
 * The saved outputs of a finished item, checked against the disk, and the actions on them.
 * The check runs when the item's outputs change and again after an action that could change
 * the answer.
 */
export function useOutputs(item: QueueItem | null): OutputsState {
  const { api, actions } = useApp();
  const fileId = item?.fileId ?? null;
  const outputs = item?.outputs;
  const [status, setStatus] = useState<OutputStatus[] | null>(null);

  const refresh = useCallback(async () => {
    if (!fileId) return;
    try {
      setStatus(await api.checkOutputs(fileId));
    } catch {
      setStatus(null);
    }
  }, [api, fileId]);

  useEffect(() => {
    void refresh();
  }, [refresh, outputs]);

  return useMemo(() => {
    const list = outputs ?? [];
    const exists = (o: OutputFile) => status?.find((s) => s.path === o.path)?.exists ?? true;
    const present = (format: OutputFile["format"]) =>
      list.find((o) => o.format === format && exists(o)) ?? null;
    const missing = list.filter((o) => !exists(o)).map((o) => fileName(o.path));
    const save = async (format: OutputFile["format"]) => {
      if (fileId && (await actions.saveCopy(fileId, format))) await refresh();
    };
    return {
      word: present("docx"),
      markdown: present("markdown"),
      revealPath:
        (present("docx") ?? present("markdown"))?.path ?? (list.length > 0 ? item!.path : null),
      notice: missing.length > 0 ? t("outputs.missing", { names: missing.join(", ") }) : null,
      saveMenu: [
        { id: "docx", label: t("outputs.saveWord"), onSelect: () => void save("docx") },
        {
          id: "markdown",
          label: t("outputs.saveMarkdown"),
          onSelect: () => void save("markdown"),
        },
      ],
      async open(output) {
        const result = await actions.openOutput(output.path);
        if (result === "missing") await refresh();
      },
    };
  }, [actions, fileId, item, outputs, refresh, status]);
}
