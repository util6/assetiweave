import { invoke } from "@tauri-apps/api/core";
import type { AppSettings } from "../store/settings/settingsSchema";

export interface AppSettingsFile {
  config_dir: string;
  config_path: string;
  conversation_adapter_dir: string;
  display_config_dir?: string;
  display_config_path?: string;
  display_conversation_adapter_dir?: string;
  settings: unknown;
}

export async function getAppSettings(): Promise<AppSettingsFile> {
  return invoke<AppSettingsFile>("get_app_settings");
}

export async function saveAppSettings(settings: AppSettings): Promise<AppSettingsFile> {
  return invoke<AppSettingsFile>("save_app_settings", { settings });
}
