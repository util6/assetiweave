import { invoke } from "@tauri-apps/api/core";

export interface CliToolsStatus {
  bundled: boolean;
  installed: boolean;
  path_configured: boolean;
  install_dir: string;
  path_entry: string;
  shim_path: string;
  bundled_cli_path: string | null;
  bundled_engine_path: string | null;
  message: string;
}

export async function getCliToolsStatus(): Promise<CliToolsStatus> {
  return invoke<CliToolsStatus>("get_cli_tools_status");
}

export async function installCliTools(): Promise<CliToolsStatus> {
  return invoke<CliToolsStatus>("install_cli_tools");
}
