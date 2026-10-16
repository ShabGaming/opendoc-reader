import { t, type MessageKey } from "../i18n";
import type { AppError } from "../types/commands";
import { driveOf } from "../util/format";

/** What a failed conversion says, picked from the error code and reason (never raw text). */
export function conversionErrorText(error: AppError): string {
  switch (error.code) {
    case "badInput": {
      const key = `error.badInput.${error.reason ?? "default"}` as MessageKey;
      const text = t(key);
      return text === key ? t("error.badInput.default") : text;
    }
    case "outOfMemory":
      return t("error.outOfMemory");
    case "modelNotReady":
      return t("error.modelNotReady");
    case "llamaServerUnreachable":
      return t("error.llamaServerUnreachable");
    case "recognitionFailed":
      return t("error.recognitionFailed");
    case "engineUnavailable":
      return t("error.engineUnavailable");
    case "diskFull":
      return t("error.diskFull");
    case "io":
      return t("error.io");
    default:
      return t("error.internal");
  }
}

export type ErrorAdvice = "smallerModel" | "reinstall" | "freeSpace" | "badInput" | "retry";

/** The advice that follows the message, and which button (if any) fits it. */
export function conversionErrorAdvice(error: AppError): ErrorAdvice {
  switch (error.code) {
    case "outOfMemory":
      return "smallerModel";
    case "modelNotReady":
      return "reinstall";
    case "diskFull":
    case "io":
      return "freeSpace";
    case "badInput":
      return "badInput";
    default:
      return "retry";
  }
}

export function adviceText(advice: ErrorAdvice): string {
  switch (advice) {
    case "smallerModel":
      return t("error.hint.smallerModel");
    case "reinstall":
      return t("error.hint.reinstall");
    case "freeSpace":
      return t("error.hint.freeSpace");
    case "badInput":
      return t("error.hint.badInput");
    default:
      return t("error.hint.retry");
  }
}

/** Bad input never succeeds on a retry. */
export function isRetryable(error: AppError): boolean {
  return error.code !== "badInput";
}

/** What a failed download says. `modelsDir` names the drive for a full disk. */
export function setupErrorText(error: AppError, modelsDir: string): string {
  switch (error.code) {
    case "offline":
      return t("setup.error.offline");
    case "downloadFailed":
      return t("setup.error.downloadFailed");
    case "checksumMismatch":
      return t("setup.error.checksumMismatch");
    case "diskFull": {
      const drive = driveOf(modelsDir);
      return drive ? t("setup.error.diskFull", { drive }) : t("setup.error.diskFullNoDrive");
    }
    case "modelNotReady":
      return t("setup.error.modelNotReady");
    default:
      return t("setup.error.default");
  }
}

export function isAppError(value: unknown): value is AppError {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as { code?: unknown }).code === "string"
  );
}
