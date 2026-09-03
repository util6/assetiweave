import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isTauriRuntime } from "./appUpdater";

export const APP_CLOSE_REQUESTED_EVENT = "app-close-requested";

export function subscribeAppCloseRequested(listener: () => void): Promise<() => void> {
  if (!isTauriRuntime()) {
    return Promise.resolve(() => undefined);
  }
  return listen(APP_CLOSE_REQUESTED_EVENT, () => {
    listener();
  });
}

export async function completeAppClose(backupDatabase: boolean): Promise<void> {
  await invoke("complete_app_close", { backupDatabase });
}

export async function cancelAppClosePrompt(): Promise<void> {
  await invoke("cancel_app_close_prompt");
}

