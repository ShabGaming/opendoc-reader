/** What the UI needs from the host that is not a command: pickers, drag and drop, links. */
export interface DropHandlers {
  onOver(over: boolean): void;
  onDrop(paths: string[]): void;
}

export interface Platform {
  readonly kind: "tauri" | "mock";
  /** A URL an <img> can load for a path from the core. Data URLs pass through. */
  assetUrl(path: string): string;
  /** The paths of the chosen files, empty when the picker was cancelled. */
  pickFiles(): Promise<string[]>;
  /** The chosen folder, null when the picker was cancelled. */
  pickFolder(defaultPath?: string): Promise<string | null>;
  /** Subscribes to files dragged onto the window. Resolves with an unsubscribe function. */
  onFileDrop(handlers: DropHandlers): Promise<() => void>;
  openUrl(url: string): Promise<void>;
  copyText(text: string): Promise<void>;
}

export const FILE_EXTENSIONS = [
  "pdf",
  "png",
  "jpg",
  "jpeg",
  "tif",
  "tiff",
  "bmp",
  "webp",
  "heic",
  "heif",
];
