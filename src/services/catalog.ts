import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  fallbackAppShortcuts,
  fallbackAssets,
  fallbackNavigationModel,
  fallbackProfiles,
  fallbackSources,
} from "../mock/catalog";
import type { NavigationModel } from "../router/types";
import { appShortcutListSchema, navigationModelSchema } from "../schemas/navigation";
import { sourceInputSchema } from "../schemas/source";
import { parseSchemaOrFallback, parseSchemaOrThrow } from "../schemas/validation";
import type {
  AppOverview,
  AppShortcut,
  Asset,
  AssetKind,
  AssetMount,
  AssetMountUpdateResult,
  AssetMountStatus,
  DeploymentPlan,
  DeploymentStrategy,
  ExecutionResult,
  Source,
  SourceInput,
  TargetProfile,
} from "../types";

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

export async function listAssets(kind?: AssetKind): Promise<Asset[]> {
  try {
    return await invoke<Asset[]>("list_assets", { kind: kind ?? null });
  } catch {
    return kind ? fallbackAssets.filter((asset) => asset.kind === kind) : fallbackAssets;
  }
}

export async function listSources(): Promise<Source[]> {
  try {
    return await invoke<Source[]>("list_sources");
  } catch {
    return fallbackSources;
  }
}

export async function listSkillSources(): Promise<Source[]> {
  try {
    return await invoke<Source[]>("list_skill_sources");
  } catch {
    return fallbackSources.filter((source) => source.scanner_kind === "skill");
  }
}

export async function createSource(source: SourceInput): Promise<Source> {
  const parsedSource = parseSchemaOrThrow(sourceInputSchema, source, "Invalid source input");

  try {
    return await invoke<Source>("create_source", { source: parsedSource });
  } catch {
    return {
      ...parsedSource,
      id: parsedSource.id ?? crypto.randomUUID(),
      last_scanned_at: null,
      last_scan_status: "preview",
    };
  }
}

export async function updateSource(source: Source): Promise<Source> {
  try {
    return await invoke<Source>("update_source", { source });
  } catch {
    return source;
  }
}

export async function deleteSource(id: string): Promise<void> {
  try {
    await invoke<void>("delete_source", { id });
  } catch {
    return;
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
    return getStoredFallbackNavigationModel();
  }
}

export async function updateNavigationModel(model: NavigationModel): Promise<NavigationModel> {
  try {
    return await invoke<NavigationModel>("update_navigation_model", { model });
  } catch {
    localStorage.setItem(FALLBACK_NAVIGATION_STORAGE_KEY, JSON.stringify(model));
    return model;
  }
}

export async function listAppShortcuts(): Promise<AppShortcut[]> {
  try {
    return await invoke<AppShortcut[]>("list_app_shortcuts");
  } catch {
    return getStoredFallbackAppShortcuts().filter((shortcut) => shortcut.enabled);
  }
}

export async function listAppShortcutSettings(): Promise<AppShortcut[]> {
  try {
    return await invoke<AppShortcut[]>("list_app_shortcut_settings");
  } catch {
    return getStoredFallbackAppShortcuts();
  }
}

export async function updateAppShortcuts(shortcuts: AppShortcut[]): Promise<AppShortcut[]> {
  try {
    return await invoke<AppShortcut[]>("update_app_shortcuts", { shortcuts });
  } catch {
    localStorage.setItem(FALLBACK_APP_SHORTCUTS_STORAGE_KEY, JSON.stringify(shortcuts));
    return shortcuts;
  }
}

export async function listAssetMounts(assetId?: string): Promise<AssetMount[]> {
  try {
    return await invoke<AssetMount[]>("list_asset_mounts", { assetId });
  } catch {
    return [];
  }
}

export async function listAssetMountStatuses(assetId?: string): Promise<AssetMountStatus[]> {
  try {
    return await invoke<AssetMountStatus[]>("list_asset_mount_statuses", { assetId });
  } catch {
    return fallbackAssets.flatMap((asset) =>
      fallbackProfiles.map((profile) => ({
        asset_id: asset.id,
        profile_id: profile.id,
        target_dir: profile.target_paths[0] ?? "",
        target_path: [profile.target_paths[0] ?? "", asset.name].filter(Boolean).join("/"),
        state: "not_mounted" as const,
        linked_source: null,
      })),
    );
  }
}

export async function toggleAssetMount(assetId: string, profileId: string): Promise<AssetMount> {
  return await invoke<AssetMount>("toggle_asset_mount", { assetId, profileId });
}

export async function unmountAssetMount(assetId: string, profileId: string): Promise<AssetMountUpdateResult> {
  return await invoke<AssetMountUpdateResult>("unmount_asset_mount", { assetId, profileId });
}

export async function setAssetMount(
  assetId: string,
  profileId: string,
  enabled: boolean,
  strategy?: DeploymentStrategy,
): Promise<AssetMount> {
  return await invoke<AssetMount>("set_asset_mount", {
    assetId,
    profileId,
    enabled,
    strategy,
  });
}

export async function scanSources(kind?: AssetKind): Promise<Asset[]> {
  try {
    return await invoke<Asset[]>("scan_sources", { kind: kind ?? null });
  } catch {
    return kind ? fallbackAssets.filter((asset) => asset.kind === kind) : fallbackAssets;
  }
}

export async function scanSkillSources(): Promise<Asset[]> {
  try {
    return await invoke<Asset[]>("scan_skill_sources");
  } catch {
    return fallbackAssets.filter((asset) => asset.kind === "skill");
  }
}

export async function adoptAppLocalSkill(assetId: string): Promise<Asset> {
  return await invoke<Asset>("adopt_app_local_skill", { assetId });
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

export async function selectSourceDirectory(title: string): Promise<string | null> {
  try {
    const selected = await open({
      directory: true,
      multiple: false,
      title,
    });
    return Array.isArray(selected) ? (selected[0] ?? null) : selected;
  } catch {
    return null;
  }
}

const FALLBACK_NAVIGATION_STORAGE_KEY = "assetiweave.preview.navigation";
const FALLBACK_APP_SHORTCUTS_STORAGE_KEY = "assetiweave.preview.appShortcuts";

function getStoredFallbackNavigationModel(): NavigationModel {
  try {
    const stored = localStorage.getItem(FALLBACK_NAVIGATION_STORAGE_KEY);
    return stored
      ? parseSchemaOrFallback(navigationModelSchema, JSON.parse(stored), fallbackNavigationModel)
      : fallbackNavigationModel;
  } catch {
    return fallbackNavigationModel;
  }
}

function getStoredFallbackAppShortcuts(): AppShortcut[] {
  try {
    const stored = localStorage.getItem(FALLBACK_APP_SHORTCUTS_STORAGE_KEY);
    return stored
      ? parseSchemaOrFallback(appShortcutListSchema, JSON.parse(stored), fallbackAppShortcuts)
      : fallbackAppShortcuts;
  } catch {
    return fallbackAppShortcuts;
  }
}
