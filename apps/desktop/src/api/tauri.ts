import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { AppError } from "../types/commands";
import type { Transport } from "./api";
import { FILE_EXTENSIONS, type Platform } from "./platform";

/** Tauri rejects with whatever the command returned; the contract says that is an AppError. */
export function toAppError(reason: unknown): AppError {
  if (
    typeof reason === "object" &&
    reason !== null &&
    "code" in reason &&
    typeof (reason as { code: unknown }).code === "string"
  ) {
    return reason as AppError;
  }
  return {
    code: "internal",
    message: typeof reason === "string" ? reason : "The command failed.",
    reason: null,
    detail: reason instanceof Error ? reason.message : null,
  };
}

export const tauriTransport: Transport = {
  async invoke(command, args) {
    try {
      return await invoke(command, args);
    } catch (reason) {
      throw toAppError(reason);
    }
  },
  async listen(event, handler) {
    return listen(event, (e) => handler(e.payload as never));
  },
};

export const tauriPlatform: Platform = {
  kind: "tauri",
  assetUrl: (path) => (path.startsWith("data:") ? path : convertFileSrc(path)),
  async pickFiles() {
    const picked = await openDialog({
      multiple: true,
      directory: false,
      filters: [{ name: "PDF and images", extensions: FILE_EXTENSIONS }],
    });
    if (picked === null) return [];
    return Array.isArray(picked) ? picked : [picked];
  },
  async pickFolder(defaultPath) {
    const picked = await openDialog({
      multiple: false,
      directory: true,
      defaultPath,
    });
    return typeof picked === "string" ? picked : null;
  },
  onFileDrop(handlers) {
    return getCurrentWebview().onDragDropEvent((event) => {
      const payload = event.payload;
      if (payload.type === "enter" || payload.type === "over") handlers.onOver(true);
      else if (payload.type === "leave") handlers.onOver(false);
      else if (payload.type === "drop") {
        handlers.onOver(false);
        if (payload.paths.length > 0) handlers.onDrop(payload.paths);
      }
    });
  },
  async openUrl(url) {
    try {
      await openUrl(url);
    } catch {
      // The opener plugin is not registered or the address is outside its scope.
      window.open(url, "_blank", "noopener");
    }
  },
  copyText: (text) => navigator.clipboard.writeText(text),
};
