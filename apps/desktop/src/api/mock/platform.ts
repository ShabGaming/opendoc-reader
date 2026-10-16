import type { Platform } from "../platform";

export interface MockPlatformOptions {
  /** What the file picker returns. */
  files?: string[];
  /** What the folder picker returns, null to act as if cancelled. */
  folder?: string | null;
  /** Open links in a new browser tab. Tests turn it off. */
  openWindow?: boolean;
}

const DEMO_DIR = "C:\\Users\\Demo\\Documents";

/**
 * A browser-only host: pickers return canned paths, and files dropped on the page are turned
 * into made-up paths from their names (a browser never reveals real ones).
 */
export function createMockPlatform(options: MockPlatformOptions = {}): Platform & {
  opened: string[];
  copied: string[];
} {
  const opened: string[] = [];
  const copied: string[] = [];
  return {
    kind: "mock",
    opened,
    copied,
    assetUrl: (path) => path,
    pickFiles: async () =>
      options.files ?? [`${DEMO_DIR}\\Meeting notes.pdf`, `${DEMO_DIR}\\scan-014.png`],
    pickFolder: async () =>
      options.folder === undefined ? `${DEMO_DIR}\\Converted` : options.folder,
    onFileDrop(handlers) {
      if (typeof window === "undefined") return Promise.resolve(() => {});
      const hasFiles = (e: DragEvent) =>
        !!e.dataTransfer && [...e.dataTransfer.types].includes("Files");
      let depth = 0;
      const enter = (e: DragEvent) => {
        if (!hasFiles(e)) return;
        e.preventDefault();
        depth += 1;
        handlers.onOver(true);
      };
      const over = (e: DragEvent) => {
        if (hasFiles(e)) e.preventDefault();
      };
      const leave = (e: DragEvent) => {
        if (!hasFiles(e)) return;
        depth = Math.max(0, depth - 1);
        if (depth === 0) handlers.onOver(false);
      };
      const drop = (e: DragEvent) => {
        if (!hasFiles(e)) return;
        e.preventDefault();
        depth = 0;
        handlers.onOver(false);
        const paths = [...(e.dataTransfer?.files ?? [])].map((f) => `${DEMO_DIR}\\${f.name}`);
        if (paths.length > 0) handlers.onDrop(paths);
      };
      window.addEventListener("dragenter", enter);
      window.addEventListener("dragover", over);
      window.addEventListener("dragleave", leave);
      window.addEventListener("drop", drop);
      return Promise.resolve(() => {
        window.removeEventListener("dragenter", enter);
        window.removeEventListener("dragover", over);
        window.removeEventListener("dragleave", leave);
        window.removeEventListener("drop", drop);
      });
    },
    openUrl: async (url) => {
      opened.push(url);
      if (options.openWindow !== false && typeof window !== "undefined") {
        window.open(url, "_blank", "noopener");
      }
    },
    copyText: async (text) => {
      copied.push(text);
      try {
        await navigator.clipboard?.writeText(text);
      } catch {
        // The clipboard can be blocked in a browser; the mock still reports success.
      }
    },
  };
}
