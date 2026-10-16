import type { ReactNode } from "react";
import { FloppyDisk, FolderOpen } from "@phosphor-icons/react";
import { t } from "../i18n";
import type { OutputsState } from "../state/useOutputs";
import { Button } from "./Button";
import { MenuButton } from "./MenuButton";

interface OutputActionsProps {
  outputs: OutputsState;
  onReveal(path: string): void;
  /** More buttons, placed after "Show in folder". */
  children?: ReactNode;
}

/**
 * What to do with a finished file: open the saved Word or Markdown file, show it in its folder,
 * or save a copy somewhere else. Falls back to Markdown, then to "Save a copy as", when the
 * Word file is gone.
 */
export function OutputActions({ outputs, onReveal, children }: OutputActionsProps) {
  const { word, markdown, saveMenu } = outputs;
  const first = word ?? markdown;
  return (
    <>
      {first ? (
        <>
          <Button variant="primary" onClick={() => void outputs.open(first)}>
            {word ? t("outputs.openWord") : t("outputs.openMarkdown")}
          </Button>
          {word && markdown && (
            <Button onClick={() => void outputs.open(markdown)}>{t("outputs.openMarkdown")}</Button>
          )}
        </>
      ) : (
        <MenuButton variant="primary" label={t("outputs.saveCopy")} items={saveMenu} />
      )}
      <Button
        variant="tertiary"
        icon={<FolderOpen size={16} aria-hidden="true" />}
        disabled={!outputs.revealPath}
        onClick={() => outputs.revealPath && onReveal(outputs.revealPath)}
      >
        {t("outputs.showInFolder")}
      </Button>
      {children}
      {first && (
        <MenuButton
          variant="ghost"
          label={t("outputs.saveCopy")}
          icon={<FloppyDisk size={16} aria-hidden="true" />}
          items={saveMenu}
        />
      )}
    </>
  );
}
