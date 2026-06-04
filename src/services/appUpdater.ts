import type { DownloadEvent, Update } from "@tauri-apps/plugin-updater";

export const ASSETIWEAVE_RELEASES_URL = "https://github.com/util6/assetiweave/releases/latest";

export interface AppUpdateInfo {
  currentVersion: string;
  date?: string;
  notes?: string;
  version: string;
}

export type UpdateProgressHandler = (event: DownloadEvent) => void;

export function isTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export async function getCurrentAppVersion() {
  const { getVersion } = await import("@tauri-apps/api/app");
  return getVersion();
}

export async function checkForAppUpdate() {
  const { check } = await import("@tauri-apps/plugin-updater");
  return check({ timeout: 30000 });
}

export async function relaunchApp() {
  const { relaunch } = await import("@tauri-apps/plugin-process");
  await relaunch();
}

export async function openReleasePage() {
  const { openUrl } = await import("@tauri-apps/plugin-opener");
  await openUrl(ASSETIWEAVE_RELEASES_URL);
}

export function toAppUpdateInfo(update: Update): AppUpdateInfo {
  return {
    currentVersion: update.currentVersion,
    date: update.date,
    notes: update.body,
    version: update.version,
  };
}

export async function closeAppUpdate(update: Update | null | undefined) {
  if (!update) {
    return;
  }
  await update.close().catch(() => {});
}
