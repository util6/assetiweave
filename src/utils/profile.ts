import type { AppKind, AppShortcut, TargetProfile, TargetProfileInput, TargetProfileRuleSet } from "../types";

export interface AppProfileFormValues {
  accentColor: string;
  appKind: AppKind;
  displayIcon: string;
  enabled: boolean;
  name: string;
  shortcutEnabled: boolean;
  targetPath: string;
}

export function deriveProfileId(name: string) {
  return name
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

export function hasProfileIdConflict(profileId: string, profiles: TargetProfile[], editingProfileId?: string) {
  return profiles.some((profile) => profile.id === profileId && profile.id !== editingProfileId);
}

export function buildTargetProfileInput(values: AppProfileFormValues, editingProfile?: TargetProfile | null): TargetProfileInput {
  const name = values.name.trim();
  const targetPath = values.targetPath.trim();
  const baseProfile = editingProfile ?? null;

  return {
    app_kind: values.appKind,
    deployment_strategy: baseProfile?.deployment_strategy ?? "symlink_to_source",
    enabled: values.enabled,
    exclude: baseProfile?.exclude ?? defaultProfileExclude(),
    id: baseProfile?.id ?? deriveProfileId(name),
    include: baseProfile?.include ?? defaultProfileInclude(),
    name,
    safety: baseProfile?.safety ?? { allow_overwrite: false, allow_remove: false },
    supported_kinds: baseProfile?.supported_kinds ?? ["skill"],
    target_paths: [targetPath],
  };
}

export function targetProfileFromInput(input: TargetProfileInput): TargetProfile {
  const name = input.name.trim();
  return {
    app_kind: input.app_kind ?? "custom",
    deployment_strategy: input.deployment_strategy ?? "symlink_to_source",
    enabled: input.enabled ?? true,
    exclude: input.exclude ?? defaultProfileExclude(),
    id: input.id?.trim() || deriveProfileId(name),
    include: input.include ?? defaultProfileInclude(),
    name,
    safety: input.safety ?? { allow_overwrite: false, allow_remove: false },
    supported_kinds: input.supported_kinds ?? ["skill"],
    target_paths: (input.target_paths ?? []).map((path) => path.trim()).filter(Boolean),
  };
}

export function defaultAppShortcut(profile: TargetProfile, overrides: Partial<AppShortcut> = {}): AppShortcut {
  return {
    accentColor: defaultAccentColor(profile.app_kind),
    appKind: profile.app_kind,
    displayIcon: defaultDisplayIcon(profile),
    enabled: profile.enabled,
    iconSvg: null,
    profileId: profile.id,
    profileName: profile.name,
    ...overrides,
  };
}

export function defaultProfileInclude(): TargetProfileRuleSet {
  return {
    groups: [],
    kinds: ["skill"],
    path_patterns: [],
    sources: [],
    tags: [],
  };
}

export function defaultProfileExclude(): TargetProfileRuleSet {
  return {
    groups: [],
    kinds: ["unclassified"],
    path_patterns: [],
    sources: [],
    tags: [],
  };
}

function defaultDisplayIcon(profile: TargetProfile) {
  return profile.app_kind === "custom" ? profile.name.slice(0, 1).toUpperCase() || "A" : `app:${profile.app_kind}`;
}

function defaultAccentColor(appKind: AppKind) {
  const colors: Record<AppKind, string> = {
    antigravity: "#a78bfa",
    claude: "#d97757",
    codex: "#10b981",
    cursor: "#94a3b8",
    custom: "#8c909f",
    gemini: "#8e75b2",
    openclaw: "#f43f5e",
    opencode: "#6366f1",
  };
  return colors[appKind];
}
