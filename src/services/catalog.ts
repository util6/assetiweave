import { invoke } from "@tauri-apps/api/core";
import {
  fallbackAppShortcuts,
  fallbackAssets,
  fallbackNavigationModel,
  fallbackProfiles,
} from "../fixtures/catalog";
import type { NavigationModel } from "../navigation/types";
import type { AppOverview, AppShortcut, Asset, DeploymentPlan, ExecutionResult, Source, TargetProfile } from "../types";

export async function getOverview(): Promise<AppOverview> {
  try {
    return await invoke<AppOverview>("get_app_overview");
  } catch {
    return {
      source_count: 2,
      asset_count: fallbackAssets.length,
      profile_count: fallbackProfiles.length,
      last_scan_status: "preview",
    };
  }
}

export async function listAssets(): Promise<Asset[]> {
  try {
    return await invoke<Asset[]>("list_assets");
  } catch {
    return fallbackAssets;
  }
}

export async function listSources(): Promise<Source[]> {
  try {
    return await invoke<Source[]>("list_sources");
  } catch {
    return [];
  }
}

export async function listProfiles(): Promise<TargetProfile[]> {
  try {
    return await invoke<TargetProfile[]>("list_profiles");
  } catch {
    return fallbackProfiles;
  }
}

export async function getNavigationModel(): Promise<NavigationModel> {
  try {
    return await invoke<NavigationModel>("get_navigation_model");
  } catch {
    return fallbackNavigationModel;
  }
}

export async function listAppShortcuts(): Promise<AppShortcut[]> {
  try {
    return await invoke<AppShortcut[]>("list_app_shortcuts");
  } catch {
    return fallbackAppShortcuts;
  }
}

export async function scanSources(): Promise<Asset[]> {
  try {
    return await invoke<Asset[]>("scan_sources");
  } catch {
    return fallbackAssets;
  }
}

export async function createPlan(profileId?: string): Promise<DeploymentPlan> {
  return await invoke<DeploymentPlan>("create_plan", { profileId });
}

export async function executePlan(plan: DeploymentPlan, actionIds?: string[]): Promise<ExecutionResult> {
  return await invoke<ExecutionResult>("execute_plan", {
    plan,
    actionIds,
  });
}

export async function revealPath(path: string): Promise<void> {
  return await invoke<void>("reveal_path", { path });
}
