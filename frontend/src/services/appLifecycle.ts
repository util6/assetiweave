import { invoke } from "@tauri-apps/api/core";

export async function completeAppClose(backupDatabase: boolean): Promise<void> {
  await invoke("complete_app_close", { backupDatabase });
}

export async function cancelAppClosePrompt(): Promise<void> {
  await invoke("cancel_app_close_prompt");
}
