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

export interface Source {
  id: string;
  name: string;
  kind: SourceKind;
  root_path: string;
  include_globs: string[];
  exclude_globs: string[];
  default_kind?: AssetKind | null;
  enabled: boolean;
  priority: number;
  last_scanned_at?: string | null;
  last_scan_status?: string | null;
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

export type DeploymentStrategy = "symlink" | "copy" | "render" | "append" | "config_merge";

export interface TargetProfile {
  id: string;
  name: string;
  app_kind: AppKind;
  target_paths: string[];
  supported_kinds: AssetKind[];
  deployment_strategy: DeploymentStrategy;
  enabled: boolean;
}

export interface AppShortcut {
  profileId: string;
  profileName: string;
  appKind: AppKind;
  displayIcon: string;
  accentColor: string;
  enabled: boolean;
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
