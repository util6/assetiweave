export type AssetKind =
  | "prompt"
  | "rule"
  | "memory"
  | "skill"
  | "mcp"
  | "agent"
  | "command"
  | "workflow"
  | "profile"
  | "custom"
  | "unclassified";

export type AssetFormat =
  | "markdown"
  | "json"
  | "yaml"
  | "toml"
  | "directory"
  | "script"
  | "sqlite"
  | "unknown";

export interface Asset {
  id: string;
  source_id: string;
  name: string;
  kind: AssetKind;
  format: AssetFormat;
  relative_path: string;
  absolute_path: string;
  entry_file?: string | null;
  description?: string | null;
  content_hash?: string | null;
  discovered_at: string;
  updated_at: string;
}

export interface AppOverview {
  source_count: number;
  asset_count: number;
  profile_count: number;
  last_scan_status: string;
}

export type SourceKind = "local" | "git_checkout" | "import" | "custom";
export type SourceScannerKind = "skill" | "mcp" | "prompt" | "rule" | "mixed" | "custom";
export type SourceOrigin = "git_repo" | "local_folder" | "app_target" | "app_local" | "assetiweave_library" | "custom";

export interface Source {
  id: string;
  name: string;
  kind: SourceKind;
  root_path: string;
  scanner_kind: SourceScannerKind;
  source_origin: SourceOrigin;
  repo_root?: string | null;
  scan_root: string;
  origin_app_kind?: AppKind | null;
  include_globs: string[];
  exclude_globs: string[];
  default_kind?: AssetKind | null;
  enabled: boolean;
  priority: number;
  last_scanned_at?: string | null;
  last_scan_status?: string | null;
}

export interface SourceInput {
  id?: string;
  name: string;
  kind: SourceKind;
  root_path: string;
  scanner_kind?: SourceScannerKind;
  source_origin?: SourceOrigin;
  repo_root?: string | null;
  scan_root?: string;
  origin_app_kind?: AppKind | null;
  include_globs: string[];
  exclude_globs: string[];
  default_kind?: AssetKind | null;
  enabled: boolean;
  priority: number;
}

export type AppKind =
  | "codex"
  | "claude"
  | "cursor"
  | "opencode"
  | "gemini"
  | "antigravity"
  | "openclaw"
  | "custom";

export type DeploymentStrategy =
  | "symlink_to_source"
  | "copy_to_target"
  | "render"
  | "append"
  | "config_merge";

export interface TargetProfileRuleSet {
  kinds: AssetKind[];
  tags: string[];
  groups: string[];
  sources: string[];
  path_patterns: string[];
}

export interface ProfileSafety {
  allow_remove: boolean;
  allow_overwrite: boolean;
}

export interface TargetProfile {
  id: string;
  name: string;
  app_kind: AppKind;
  target_paths: string[];
  supported_kinds: AssetKind[];
  deployment_strategy: DeploymentStrategy;
  enabled: boolean;
  include: TargetProfileRuleSet;
  exclude: TargetProfileRuleSet;
  safety: ProfileSafety;
}

export interface TargetProfileInput {
  id?: string;
  name: string;
  app_kind?: AppKind;
  target_paths?: string[];
  supported_kinds?: AssetKind[];
  deployment_strategy?: DeploymentStrategy;
  enabled?: boolean;
  include?: TargetProfileRuleSet;
  exclude?: TargetProfileRuleSet;
  safety?: ProfileSafety;
}

export interface AppShortcutIconPath {
  clipRule?: "evenodd" | "nonzero";
  d: string;
  fillRule?: "evenodd" | "nonzero";
}

export interface AppShortcutIconSvg {
  paths: AppShortcutIconPath[];
  viewBox?: string;
}

export interface AppShortcut {
  profileId: string;
  profileName: string;
  appKind: AppKind;
  displayIcon: string;
  iconSvg?: AppShortcutIconSvg | null;
  accentColor: string;
  enabled: boolean;
}

export interface AssetMount {
  asset_id: string;
  profile_id: string;
  enabled: boolean;
  strategy: DeploymentStrategy;
  created_at: string;
  updated_at: string;
}

export type PhysicalMountState = "mounted" | "not_mounted" | "conflict" | "broken";

export interface AssetMountStatus {
  asset_id: string;
  profile_id: string;
  target_dir: string;
  target_path: string;
  state: PhysicalMountState;
  linked_source?: string | null;
}

export interface AssetMountUpdateResult {
  mount: AssetMount;
  status: AssetMountStatus;
}

export interface AssetGroupRules {
  source_ids: string[];
  relative_path_globs: string[];
  name_contains: string | null;
}

export interface AssetGroup {
  id: string;
  name: string;
  description: string | null;
  color: string;
  asset_kind: AssetKind;
  enabled: boolean;
  sort_order: number;
  rules: AssetGroupRules;
  created_at: string;
  updated_at: string;
}

export type AssetGroupMemberOrigin = "manual" | "rule" | "manual_and_rule";

export interface AssetGroupResolvedMember {
  asset_id: string;
  origin: AssetGroupMemberOrigin;
}

export interface AssetGroupDetail {
  group: AssetGroup;
  members: AssetGroupResolvedMember[];
  manual_asset_ids: string[];
}

export interface AssetGroupInput {
  id?: string;
  name: string;
  description?: string | null;
  color?: string | null;
  enabled?: boolean;
  sort_order?: number;
  rules?: AssetGroupRules;
}

export interface AssetGroupMountError {
  asset_id: string;
  message: string;
}

export interface ApplyAssetGroupMountResult {
  group_id: string;
  profile_id: string;
  enabled: boolean;
  requested_count: number;
  updated_count: number;
  error_count: number;
  mounts: AssetMount[];
  statuses: AssetMountStatus[];
  errors: AssetGroupMountError[];
}

export interface SkillGroupExclusiveMountInput {
  group_ids: string[];
  profile_id: string;
  mount_selected: true;
  dry_run: boolean;
}

export interface SkillGroupExclusiveMountItem {
  asset_id: string;
  name: string;
}

export interface SkillGroupExclusiveMountSkippedItem extends SkillGroupExclusiveMountItem {
  reason: string;
}

export interface SkillGroupExclusiveMountPreview {
  profile_id: string;
  group_ids: string[];
  selected_skill_ids: string[];
  keep: SkillGroupExclusiveMountItem[];
  mount: SkillGroupExclusiveMountItem[];
  unmount: SkillGroupExclusiveMountItem[];
  skipped: SkillGroupExclusiveMountSkippedItem[];
  keep_count: number;
  mount_count: number;
  unmount_count: number;
  skipped_count: number;
}

export interface SkillGroupExclusiveMountError extends SkillGroupExclusiveMountItem {
  message: string;
}

export interface ApplySkillGroupExclusiveMountResult extends SkillGroupExclusiveMountPreview {
  statuses: AssetMountStatus[];
  errors: SkillGroupExclusiveMountError[];
}

export type DeploymentActionType = "create" | "update" | "remove" | "skip" | "conflict";
export type RiskLevel = "low" | "medium" | "high";

export interface DeploymentAction {
  id: string;
  action_type: DeploymentActionType;
  asset_id?: string | null;
  profile_id: string;
  source_path?: string | null;
  target_path: string;
  strategy: DeploymentStrategy;
  reason: string;
  risk: RiskLevel;
  selectable: boolean;
}

export interface DeploymentPlanSummary {
  create_count: number;
  update_count: number;
  remove_count: number;
  skip_count: number;
  conflict_count: number;
}

export interface DeploymentPlan {
  id: string;
  created_at: string;
  profile_id?: string | null;
  actions: DeploymentAction[];
  summary: DeploymentPlanSummary;
}

export interface ExecutionResult {
  executed_count: number;
  skipped_count: number;
  conflict_count: number;
  errors: string[];
}
