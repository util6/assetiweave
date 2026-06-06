import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  fallbackAppShortcuts,
  fallbackAssets,
  fallbackNavigationModel,
  fallbackProfiles,
  fallbackSources,
  fallbackSkillGroups,
} from "../mock/catalog";
import type { NavigationModel } from "../router/types";
import {
  applyAssetGroupMountResultSchema,
  applySkillGroupExclusiveMountResultSchema,
  assetGroupDetailListSchema,
  assetGroupDetailSchema,
  assetGroupInputSchema,
  skillGroupExclusiveMountInputSchema,
  skillGroupExclusiveMountPreviewSchema,
} from "../schemas/group";
import { appShortcutListSchema, navigationModelSchema } from "../schemas/navigation";
import { targetProfileInputSchema, targetProfileListSchema, targetProfileSchema } from "../schemas/profile";
import { sourceInputSchema } from "../schemas/source";
import { parseSchemaOrFallback, parseSchemaOrThrow } from "../schemas/validation";
import type {
  ApplyAssetGroupMountResult,
  ApplySkillGroupExclusiveMountResult,
  AppOverview,
  AppShortcut,
  Asset,
  AssetGroup,
  AssetGroupDetail,
  AssetGroupInput,
  AssetKind,
  AssetMount,
  AssetMountUpdateResult,
  AssetMountStatus,
  DeploymentPlan,
  DeploymentStrategy,
  ExecutionResult,
  Source,
  SourceInput,
  SkillBackupSettings,
  SkillGroupExclusiveMountInput,
  SkillGroupExclusiveMountPreview,
  TargetProfile,
  TargetProfileInput,
} from "../types";
import { defaultAppShortcut, deriveProfileId, targetProfileFromInput } from "../utils/profile";

export async function getOverview(): Promise<AppOverview> {
  try {
    return await invoke<AppOverview>("get_app_overview");
  } catch {
    return {
      source_count: 2,
      asset_count: fallbackAssets.length,
      profile_count: getStoredFallbackProfiles().length,
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

export async function getSkillBackupSettings(): Promise<SkillBackupSettings> {
  try {
    return await invoke<SkillBackupSettings>("get_skill_backup_settings");
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return {
      root_path: "~/.assetiweave/library/skills",
      expanded_root_path: "~/.assetiweave/library/skills",
      default_root_path: "~/.assetiweave/library/skills",
      is_default_root: true,
      exists: true,
    };
  }
}

export async function updateSkillBackupSettings(rootPath: string, migrate = true): Promise<SkillBackupSettings> {
  return await invoke<SkillBackupSettings>("update_skill_backup_settings", { root_path: rootPath, migrate });
}

export async function backupSkill(assetId: string): Promise<Asset> {
  return await invoke<Asset>("backup_skill", { assetId });
}

export async function updateAssetDescription(assetId: string, description: string | null): Promise<Asset> {
  try {
    return await invoke<Asset>("update_asset_description", { assetId, description });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    const asset = fallbackAssets.find((candidate) => candidate.id === assetId);
    if (!asset) {
      throw new Error(`asset not found: ${assetId}`);
    }
    return {
      ...asset,
      description,
      updated_at: new Date().toISOString(),
    };
  }
}

export async function deleteAsset(assetId: string, unmount = false): Promise<Asset> {
  try {
    return await invoke<Asset>("delete_asset", { assetId, unmount });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    const asset = fallbackAssets.find((candidate) => candidate.id === assetId);
    if (!asset) {
      throw new Error(`asset not found: ${assetId}`);
    }
    return asset;
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
    return getStoredFallbackProfiles();
  }
}

export async function createProfile(profile: TargetProfileInput): Promise<TargetProfile> {
  const parsedProfile = parseSchemaOrThrow(
    targetProfileInputSchema,
    { ...profile, id: profile.id ?? deriveProfileId(profile.name) },
    "Invalid target profile input",
  );

  try {
    return parseSchemaOrThrow(
      targetProfileSchema,
      await invoke<TargetProfile>("create_profile", { input: parsedProfile }),
      "Invalid target profile",
    );
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    const profile = targetProfileFromInput(parsedProfile);
    const profiles = getStoredFallbackProfiles();
    if (profiles.some((candidate) => candidate.id === profile.id)) {
      throw new Error(`profile already exists: ${profile.id}`);
    }
    setStoredFallbackProfiles([...profiles, profile]);
    upsertStoredFallbackAppShortcut(defaultAppShortcut(profile));
    return profile;
  }
}

export async function updateProfile(profile: TargetProfile): Promise<TargetProfile> {
  const parsedProfile = parseSchemaOrThrow(targetProfileSchema, profile, "Invalid target profile");

  try {
    return parseSchemaOrThrow(
      targetProfileSchema,
      await invoke<TargetProfile>("update_profile", { profile: parsedProfile }),
      "Invalid target profile",
    );
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    const profiles = getStoredFallbackProfiles();
    if (!profiles.some((candidate) => candidate.id === parsedProfile.id)) {
      throw new Error(`profile not found: ${parsedProfile.id}`);
    }
    setStoredFallbackProfiles(profiles.map((candidate) => (candidate.id === parsedProfile.id ? parsedProfile : candidate)));
    upsertStoredFallbackAppShortcut(defaultAppShortcut(parsedProfile));
    return parsedProfile;
  }
}

export async function deleteProfile(id: string): Promise<void> {
  try {
    await invoke<void>("delete_profile", { id });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    if (getStoredFallbackMountStatuses().some((status) => status.profile_id === id && status.state === "mounted")) {
      throw new Error(`profile has mounted assets: ${id}`);
    }
    setStoredFallbackProfiles(getStoredFallbackProfiles().filter((profile) => profile.id !== id));
    setStoredFallbackAppShortcuts(getStoredFallbackAppShortcuts().filter((shortcut) => shortcut.profileId !== id));
    setStoredFallbackMountStatuses(getStoredFallbackMountStatuses().filter((status) => status.profile_id !== id));
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
    setStoredFallbackAppShortcuts(shortcuts);
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
    const statuses = getStoredFallbackMountStatuses();
    return assetId ? statuses.filter((status) => status.asset_id === assetId) : statuses;
  }
}

export async function refreshAssetMountStatuses(assetId?: string): Promise<AssetMountStatus[]> {
  try {
    return await invoke<AssetMountStatus[]>("refresh_asset_mount_statuses", { assetId: assetId ?? null });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    const statuses = getStoredFallbackMountStatuses();
    return assetId ? statuses.filter((status) => status.asset_id === assetId) : statuses;
  }
}

export async function toggleAssetMount(assetId: string, profileId: string): Promise<AssetMount> {
  return await invoke<AssetMount>("toggle_asset_mount", { assetId, profileId });
}

export async function mountAssetMount(assetId: string, profileId: string): Promise<AssetMountUpdateResult> {
  try {
    return await invoke<AssetMountUpdateResult>("mount_asset_mount", { assetId, profileId });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return setStoredFallbackMountStatus(assetId, profileId, true);
  }
}

export async function unmountAssetMount(assetId: string, profileId: string): Promise<AssetMountUpdateResult> {
  try {
    return await invoke<AssetMountUpdateResult>("unmount_asset_mount", { assetId, profileId });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return setStoredFallbackMountStatus(assetId, profileId, false);
  }
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

export async function listSkillGroups(): Promise<AssetGroupDetail[]> {
  try {
    return parseSchemaOrThrow(
      assetGroupDetailListSchema,
      await invoke<AssetGroupDetail[]>("list_skill_groups"),
      "Invalid skill group list",
    );
  } catch {
    return getStoredFallbackSkillGroups();
  }
}

export async function createSkillGroup(input: AssetGroupInput): Promise<AssetGroupDetail> {
  const parsedInput = parseSchemaOrThrow(assetGroupInputSchema, input, "Invalid skill group input");

  try {
    return parseSchemaOrThrow(
      assetGroupDetailSchema,
      await invoke<AssetGroupDetail>("create_skill_group", { input: parsedInput }),
      "Invalid skill group",
    );
  } catch {
    const now = new Date().toISOString();
    const detail: AssetGroupDetail = {
      group: {
        id: parsedInput.id ?? crypto.randomUUID(),
        name: parsedInput.name,
        description: parsedInput.description ?? null,
        color: parsedInput.color ?? "#10b981",
        asset_kind: "skill",
        enabled: parsedInput.enabled ?? true,
        sort_order: parsedInput.sort_order ?? getStoredFallbackSkillGroups().length * 10,
        rules: parsedInput.rules ?? { source_ids: [], relative_path_globs: [], name_contains: null },
        created_at: now,
        updated_at: now,
      },
      members: [],
      manual_asset_ids: [],
    };
    const groups = [...getStoredFallbackSkillGroups(), detail];
    setStoredFallbackSkillGroups(groups);
    return detail;
  }
}

export async function updateSkillGroup(group: AssetGroup): Promise<AssetGroupDetail> {
  try {
    return parseSchemaOrThrow(
      assetGroupDetailSchema,
      await invoke<AssetGroupDetail>("update_skill_group", { group }),
      "Invalid skill group",
    );
  } catch {
    const groups = getStoredFallbackSkillGroups().map((detail) =>
      detail.group.id === group.id ? resolveFallbackGroupDetail({ ...detail, group }) : detail,
    );
    setStoredFallbackSkillGroups(groups);
    return groups.find((detail) => detail.group.id === group.id) ?? resolveFallbackGroupDetail({ group, members: [], manual_asset_ids: [] });
  }
}

export async function deleteSkillGroup(groupId: string): Promise<void> {
  try {
    await invoke<void>("delete_skill_group", { groupId });
  } catch {
    setStoredFallbackSkillGroups(getStoredFallbackSkillGroups().filter((detail) => detail.group.id !== groupId));
  }
}

export async function setSkillGroupManualMembers(groupId: string, assetIds: string[]): Promise<AssetGroupDetail> {
  try {
    return parseSchemaOrThrow(
      assetGroupDetailSchema,
      await invoke<AssetGroupDetail>("set_skill_group_manual_members", { groupId, assetIds }),
      "Invalid skill group",
    );
  } catch {
    const groups = getStoredFallbackSkillGroups().map((detail) =>
      detail.group.id === groupId ? resolveFallbackGroupDetail({ ...detail, manual_asset_ids: [...new Set(assetIds)] }) : detail,
    );
    setStoredFallbackSkillGroups(groups);
    return groups.find((detail) => detail.group.id === groupId) ?? getStoredFallbackSkillGroups()[0]!;
  }
}

export async function applySkillGroupMount(
  groupId: string,
  profileId: string,
  enabled: boolean,
): Promise<ApplyAssetGroupMountResult> {
  try {
    return parseSchemaOrThrow(
      applyAssetGroupMountResultSchema,
      await invoke<ApplyAssetGroupMountResult>("apply_skill_group_mount", { groupId, profileId, enabled }),
      "Invalid skill group mount result",
    );
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return {
      group_id: groupId,
      profile_id: profileId,
      enabled,
      requested_count: 0,
      updated_count: 0,
      error_count: 0,
      mounts: [],
      statuses: [],
      errors: [],
    };
  }
}

export async function previewSkillGroupExclusiveMount(
  input: SkillGroupExclusiveMountInput,
): Promise<SkillGroupExclusiveMountPreview> {
  const parsedInput = parseSchemaOrThrow(
    skillGroupExclusiveMountInputSchema,
    { ...input, dry_run: true },
    "Invalid exclusive skill group mount input",
  );

  try {
    return parseSchemaOrThrow(
      skillGroupExclusiveMountPreviewSchema,
      await invoke<SkillGroupExclusiveMountPreview>("preview_skill_group_exclusive_mount", { input: parsedInput }),
      "Invalid exclusive skill group mount preview",
    );
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return buildFallbackExclusiveMountPreview(parsedInput);
  }
}

export async function applySkillGroupExclusiveMount(
  input: SkillGroupExclusiveMountInput,
): Promise<ApplySkillGroupExclusiveMountResult> {
  const parsedInput = parseSchemaOrThrow(
    skillGroupExclusiveMountInputSchema,
    { ...input, dry_run: false },
    "Invalid exclusive skill group mount input",
  );

  try {
    return parseSchemaOrThrow(
      applySkillGroupExclusiveMountResultSchema,
      await invoke<ApplySkillGroupExclusiveMountResult>("apply_skill_group_exclusive_mount", { input: parsedInput }),
      "Invalid exclusive skill group mount result",
    );
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return applyFallbackExclusiveMount(parsedInput);
  }
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

export async function selectTargetDirectory(title: string): Promise<string | null> {
  return selectSourceDirectory(title);
}

const FALLBACK_NAVIGATION_STORAGE_KEY = "assetiweave.preview.navigation";
const FALLBACK_PROFILES_STORAGE_KEY = "assetiweave.preview.profiles";
const FALLBACK_APP_SHORTCUTS_STORAGE_KEY = "assetiweave.preview.appShortcuts";
const FALLBACK_MOUNT_STATUSES_STORAGE_KEY = "assetiweave.preview.mountStatuses";
const FALLBACK_SKILL_GROUPS_STORAGE_KEY = "assetiweave.preview.skillGroups";

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

function getStoredFallbackProfiles(): TargetProfile[] {
  try {
    const stored = localStorage.getItem(FALLBACK_PROFILES_STORAGE_KEY);
    return stored
      ? parseSchemaOrFallback(targetProfileListSchema, JSON.parse(stored), fallbackProfiles)
      : fallbackProfiles;
  } catch {
    return fallbackProfiles;
  }
}

function setStoredFallbackProfiles(profiles: TargetProfile[]) {
  localStorage.setItem(FALLBACK_PROFILES_STORAGE_KEY, JSON.stringify(profiles));
}

function getStoredFallbackAppShortcuts(): AppShortcut[] {
  const profiles = getStoredFallbackProfiles();
  try {
    const stored = localStorage.getItem(FALLBACK_APP_SHORTCUTS_STORAGE_KEY);
    const shortcuts = stored
      ? parseSchemaOrFallback(appShortcutListSchema, JSON.parse(stored), fallbackAppShortcuts)
      : fallbackAppShortcuts;
    const shortcutByProfileId = new Map(shortcuts.map((shortcut) => [shortcut.profileId, shortcut]));
    return profiles.map((profile) => {
      const shortcut = shortcutByProfileId.get(profile.id);
      return shortcut
        ? {
            ...shortcut,
            appKind: profile.app_kind,
            profileName: profile.name,
          }
        : defaultAppShortcut(profile);
    });
  } catch {
    return profiles.map((profile) => {
      const shortcut = fallbackAppShortcuts.find((candidate) => candidate.profileId === profile.id);
      return shortcut
        ? { ...shortcut, appKind: profile.app_kind, profileName: profile.name }
        : defaultAppShortcut(profile);
    });
  }
}

function setStoredFallbackAppShortcuts(shortcuts: AppShortcut[]) {
  localStorage.setItem(FALLBACK_APP_SHORTCUTS_STORAGE_KEY, JSON.stringify(shortcuts));
}

function upsertStoredFallbackAppShortcut(shortcut: AppShortcut) {
  setStoredFallbackAppShortcuts([
    ...getStoredFallbackAppShortcuts().filter((candidate) => candidate.profileId !== shortcut.profileId),
    shortcut,
  ]);
}

function getStoredFallbackSkillGroups(): AssetGroupDetail[] {
  try {
    const stored = localStorage.getItem(FALLBACK_SKILL_GROUPS_STORAGE_KEY);
    const groups = stored
      ? parseSchemaOrFallback(assetGroupDetailListSchema, JSON.parse(stored), fallbackSkillGroups)
      : fallbackSkillGroups;
    return groups.map(resolveFallbackGroupDetail);
  } catch {
    return fallbackSkillGroups.map(resolveFallbackGroupDetail);
  }
}

function setStoredFallbackSkillGroups(groups: AssetGroupDetail[]) {
  localStorage.setItem(FALLBACK_SKILL_GROUPS_STORAGE_KEY, JSON.stringify(groups.map(resolveFallbackGroupDetail)));
}

function isTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function getStoredFallbackMountStatuses(): AssetMountStatus[] {
  const baseStatuses = fallbackMountStatuses();
  const statusByKey = new Map(baseStatuses.map((status) => [mountStatusKey(status.asset_id, status.profile_id), status]));

  if (typeof localStorage === "undefined") {
    return baseStatuses;
  }

  try {
    const stored = localStorage.getItem(FALLBACK_MOUNT_STATUSES_STORAGE_KEY);
    const parsed: unknown = stored ? JSON.parse(stored) : [];
    if (Array.isArray(parsed)) {
      for (const candidate of parsed) {
        if (isAssetMountStatus(candidate)) {
          statusByKey.set(mountStatusKey(candidate.asset_id, candidate.profile_id), candidate);
        }
      }
    }
  } catch {
    return baseStatuses;
  }

  return [...statusByKey.values()];
}

function setStoredFallbackMountStatuses(statuses: AssetMountStatus[]) {
  if (typeof localStorage !== "undefined") {
    localStorage.setItem(FALLBACK_MOUNT_STATUSES_STORAGE_KEY, JSON.stringify(statuses));
  }
}

function setStoredFallbackMountStatus(assetId: string, profileId: string, enabled: boolean): AssetMountUpdateResult {
  const status = fallbackMountStatus(assetId, profileId, enabled);
  const statuses = [
    ...getStoredFallbackMountStatuses().filter(
      (candidate) => candidate.asset_id !== assetId || candidate.profile_id !== profileId,
    ),
    status,
  ];
  setStoredFallbackMountStatuses(statuses);

  const now = new Date().toISOString();
  const profile = getStoredFallbackProfiles().find((candidate) => candidate.id === profileId);
  return {
    mount: {
      asset_id: assetId,
      profile_id: profileId,
      enabled,
      strategy: profile?.deployment_strategy ?? "symlink_to_source",
      created_at: now,
      updated_at: now,
    },
    status,
  };
}

function buildFallbackExclusiveMountPreview(input: SkillGroupExclusiveMountInput): SkillGroupExclusiveMountPreview {
  const profile = getStoredFallbackProfiles().find((candidate) => candidate.id === input.profile_id);
  if (!profile || !profile.enabled || !profile.supported_kinds.includes("skill")) {
    throw new Error(`profile does not support skill assets: ${input.profile_id}`);
  }

  const skillAssetById = new Map(fallbackAssets.filter((asset) => asset.kind === "skill").map((asset) => [asset.id, asset]));
  const selectedSkillIds = new Set<string>();
  const groupIds: string[] = [];
  const seenGroupIds = new Set<string>();
  for (const groupId of input.group_ids) {
    if (seenGroupIds.has(groupId)) {
      continue;
    }
    seenGroupIds.add(groupId);
    const detail = getStoredFallbackSkillGroups().find((group) => group.group.id === groupId);
    if (!detail?.group.enabled) {
      continue;
    }

    groupIds.push(detail.group.id);
    for (const member of detail.members) {
      if (skillAssetById.has(member.asset_id)) {
        selectedSkillIds.add(member.asset_id);
      }
    }
  }

  const statuses = getStoredFallbackMountStatuses();
  const keep: SkillGroupExclusiveMountPreview["keep"] = [];
  const mount: SkillGroupExclusiveMountPreview["mount"] = [];
  const unmount: SkillGroupExclusiveMountPreview["unmount"] = [];

  for (const assetId of selectedSkillIds) {
    const asset = skillAssetById.get(assetId);
    if (!asset) {
      continue;
    }
    const status = statuses.find((candidate) => candidate.asset_id === assetId && candidate.profile_id === profile.id);
    if (status?.state === "mounted") {
      keep.push({ asset_id: asset.id, name: asset.name });
    } else {
      mount.push({ asset_id: asset.id, name: asset.name });
    }
  }

  for (const asset of skillAssetById.values()) {
    if (selectedSkillIds.has(asset.id)) {
      continue;
    }
    const status = statuses.find((candidate) => candidate.asset_id === asset.id && candidate.profile_id === profile.id);
    if (status?.state === "mounted") {
      unmount.push({ asset_id: asset.id, name: asset.name });
    }
  }

  const selected_skill_ids = [...selectedSkillIds].sort();
  keep.sort(compareExclusiveMountItems);
  mount.sort(compareExclusiveMountItems);
  unmount.sort(compareExclusiveMountItems);

  return {
    profile_id: profile.id,
    group_ids: groupIds,
    selected_skill_ids,
    keep,
    mount,
    unmount,
    skipped: [],
    keep_count: keep.length,
    mount_count: mount.length,
    unmount_count: unmount.length,
    skipped_count: 0,
  };
}

function applyFallbackExclusiveMount(input: SkillGroupExclusiveMountInput): ApplySkillGroupExclusiveMountResult {
  const preview = buildFallbackExclusiveMountPreview(input);
  const affectedAssetIds = new Set([
    ...preview.keep.map((item) => item.asset_id),
    ...preview.mount.map((item) => item.asset_id),
    ...preview.unmount.map((item) => item.asset_id),
  ]);
  const nextStatuses = getStoredFallbackMountStatuses().filter(
    (status) => status.profile_id !== preview.profile_id || !affectedAssetIds.has(status.asset_id),
  );
  const statuses = [
    ...preview.keep.map((item) => fallbackMountStatus(item.asset_id, preview.profile_id, true)),
    ...preview.mount.map((item) => fallbackMountStatus(item.asset_id, preview.profile_id, true)),
    ...preview.unmount.map((item) => fallbackMountStatus(item.asset_id, preview.profile_id, false)),
  ];

  setStoredFallbackMountStatuses([...nextStatuses, ...statuses]);

  return {
    ...preview,
    statuses,
    errors: [],
  };
}

function compareExclusiveMountItems(
  left: SkillGroupExclusiveMountPreview["keep"][number],
  right: SkillGroupExclusiveMountPreview["keep"][number],
) {
  return left.name.localeCompare(right.name) || left.asset_id.localeCompare(right.asset_id);
}

function fallbackMountStatuses(): AssetMountStatus[] {
  return fallbackAssets.flatMap((asset) =>
    getStoredFallbackProfiles().map((profile) => fallbackMountStatus(asset.id, profile.id, false)),
  );
}

function fallbackMountStatus(assetId: string, profileId: string, enabled: boolean): AssetMountStatus {
  const asset = fallbackAssets.find((candidate) => candidate.id === assetId);
  const profile = getStoredFallbackProfiles().find((candidate) => candidate.id === profileId);
  const targetDir = profile?.target_paths[0] ?? "";
  return {
    asset_id: assetId,
    profile_id: profileId,
    target_dir: targetDir,
    target_path: [targetDir, asset?.name ?? assetId].filter(Boolean).join("/"),
    state: enabled ? "mounted" : "not_mounted",
    linked_source: enabled ? (asset?.absolute_path ?? null) : null,
  };
}

function mountStatusKey(assetId: string, profileId: string) {
  return `${assetId}:${profileId}`;
}

function isAssetMountStatus(candidate: unknown): candidate is AssetMountStatus {
  if (!candidate || typeof candidate !== "object") {
    return false;
  }

  const value = candidate as Partial<AssetMountStatus>;
  return (
    typeof value.asset_id === "string" &&
    typeof value.profile_id === "string" &&
    typeof value.target_dir === "string" &&
    typeof value.target_path === "string" &&
    (value.state === "mounted" ||
      value.state === "not_mounted" ||
      value.state === "conflict" ||
      value.state === "broken")
  );
}

function resolveFallbackGroupDetail(detail: AssetGroupDetail): AssetGroupDetail {
  const manualIds = new Set(detail.manual_asset_ids);
  const members = new Map<AssetGroupDetail["members"][number]["asset_id"], AssetGroupDetail["members"][number]["origin"]>();
  for (const member of detail.members) {
    if (member.origin === "rule" || member.origin === "manual_and_rule") {
      members.set(member.asset_id, manualIds.has(member.asset_id) ? "manual_and_rule" : "rule");
    } else if (manualIds.has(member.asset_id)) {
      members.set(member.asset_id, "manual");
    }
  }
  for (const assetId of manualIds) {
    members.set(assetId, members.get(assetId) === "rule" ? "manual_and_rule" : "manual");
  }

  return {
    ...detail,
    members: [...members].map(([asset_id, origin]) => ({ asset_id, origin })),
    manual_asset_ids: [...manualIds],
  };
}
