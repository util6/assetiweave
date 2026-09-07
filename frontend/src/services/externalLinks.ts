import { isTauriRuntime } from "./appUpdater";

export async function openExternalUrl(url: string): Promise<void> {
  if (typeof window === "undefined") {
    return;
  }

  if (!isTauriRuntime()) {
    window.open(url, "_blank", "noopener,noreferrer");
    return;
  }

  try {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(url);
  } catch {
    window.open(url, "_blank", "noopener,noreferrer");
  }
}
