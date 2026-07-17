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
  display_path?: string;
  entry_file?: string | null;
  description?: string | null;
  content_hash?: string | null;
  discovered_at: string;
  updated_at: string;
  repository?: GitRepositoryInfo | null;
  backup_status?: SkillBackupAssetStatus | null;
}

export interface GitRepositoryInfo {
  root_path: string;
  display_root_path?: string;
  remote_url?: string | null;
  web_url?: string | null;
}

export type SkillBackupState = "backed_up" | "downloaded";

export interface SkillBackupAssetStatus {
  state: SkillBackupState;
  backup_path?: string | null;
  display_backup_path?: string | null;
  hidden_asset_ids: string[];
}

export interface SkillBackupSettings {
  root_path: string;
  expanded_root_path: string;
  default_root_path: string;
  display_root_path?: string;
  display_default_root_path?: string;
  is_default_root: boolean;
  exists: boolean;
}

export interface SkillSearchCandidate {
  name: string;
  description?: string | null;
  match_reason?: string | null;
  url: string;
  path?: string | null;
  clone_url?: string | null;
  default_branch?: string | null;
  stars?: number | null;
  acquire_command: string;
}

export interface SkillSearchResult {
  query: string;
  provider: string;
  candidates: SkillSearchCandidate[];
  warnings: string[];
}

export type SkillRemoteSourceStatus = "unknown" | "current" | "changed" | "error";

export interface SkillRemoteSource {
  asset_id: string;
  provider: string;
  source_url: string;
  repo_url: string;
  branch: string;
  path?: string | null;
  acquired_at: string;
  acquired_tree_sha?: string | null;
  local_content_hash?: string | null;
  last_checked_at?: string | null;
  latest_tree_sha?: string | null;
  status: SkillRemoteSourceStatus;
  message?: string | null;
}

export interface SkillAcquireResult {
  dry_run: boolean;
  provider: string;
  url: string;
  repo_url: string;
  branch?: string | null;
  path?: string | null;
  name: string;
  staging_path: string;
  skill_path: string;
  security_notice?: string | null;
  import?: {
    dry_run: boolean;
    asset?: Asset;
  };
  remote_source?: SkillRemoteSource;
}

export interface AppOverview {
  source_count: number;
  asset_count: number;
  profile_count: number;
  last_scan_status: string;
}

export type TenantKind = "local_workspace" | "organization";
export type TenantStatus = "active" | "archived";

export interface Tenant {
  id: string;
  slug: string;
  name: string;
  kind: TenantKind;
  status: TenantStatus;
  created_at: string;
  updated_at: string;
}

export interface TenantCreateParams {
  name: string;
  slug?: string | null;
  set_active?: boolean;
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

export type ConversationAdapterKind = "external";
export type ConversationSourceKind = "live" | "file" | "directory" | "sqlite" | "custom";
export type ConversationAdapterTrustState = "built_in" | "trusted" | "changed" | "untrusted";
export type ConversationAdapterPackageRecordKind = "session" | "web";
export type ConversationAdapterPackageOrigin =
  | "built_in"
  | "managed_release"
  | "local_directory"
  | "git_ref"
  | "legacy_external"
  | "dev_override";
export type ConversationAdapterRuntimeGateStatus =
  | "ready"
  | "runtime_missing"
  | "hash_mismatch"
  | "manifest_invalid"
  | "core_incompatible";
export type ConversationPackageUpdatePolicy =
  | "manual"
  | "follow_stable"
  | "follow_beta"
  | "pin_exact";
export type ConversationPartRole = "user" | "assistant" | "tool" | "system";
export type ConversationPartKind = "text" | "code_block" | "command" | "tool" | "file_change" | "subagent" | "metadata";
export type ConversationGroupingOrigin = "imported" | "auto_merged" | "manual";

export interface ConversationAdapter {
  id: string;
  name: string;
  kind: ConversationAdapterKind;
  version: string;
  enabled: boolean;
  manifest_path?: string | null;
  executable_path?: string | null;
  content_hash?: string | null;
  trusted_hash?: string | null;
  trust_state: ConversationAdapterTrustState;
  protocol_version?: number | null;
  capabilities: string[];
  input_kinds: ConversationSourceKind[];
  created_at: string;
  updated_at: string;
}

export interface ConversationAdapterPackage {
  package_id: string;
  adapter_id: string;
  name: string;
  version: string;
  record_kind: ConversationAdapterPackageRecordKind;
  install_dir: string;
  manifest_path: string;
  adapter_manifest_path: string;
  runtime_protocol: string;
  runtime_ready: boolean;
  origin: ConversationAdapterPackageOrigin;
  source_url?: string | null;
  git_ref?: string | null;
  git_commit?: string | null;
  catalog_url?: string | null;
  update_policy: ConversationPackageUpdatePolicy;
  latest_version?: string | null;
  last_checked_at?: string | null;
  runtime_gate_status: ConversationAdapterRuntimeGateStatus;
  runtime_validated_at?: string | null;
  installed_content_hash?: string | null;
  trusted_package_hash?: string | null;
  error_message?: string | null;
  created_at: string;
  updated_at: string;
}

export interface ConversationSource {
  id: string;
  adapter_id: string;
  name: string;
  kind: ConversationSourceKind;
  location: string;
  config_json?: string | null;
  enabled: boolean;
  last_synced_at?: string | null;
  last_sync_status?: string | null;
  created_at: string;
  updated_at: string;
}

export interface ConversationSession {
  id: string;
  source_id: string;
  adapter_id: string;
  external_id: string;
  title: string;
  project_path?: string | null;
  started_at?: string | null;
  updated_at?: string | null;
  source_locator?: string | null;
  source_fingerprint?: string | null;
  missing: boolean;
  created_at: string;
  imported_at: string;
}

export interface ConversationSessionListItem extends ConversationSession {
  question_count: number;
  turn_count: number;
}

export interface ConversationTurn {
  id: string;
  session_id: string;
  external_id: string;
  turn_index: number;
  user_text: string;
  title?: string | null;
  started_at?: string | null;
  ended_at?: string | null;
  fingerprint: string;
  missing: boolean;
  imported_at: string;
}

export interface ConversationPart {
  id: string;
  turn_id: string;
  part_index: number;
  role: ConversationPartRole;
  kind: ConversationPartKind;
  text?: string | null;
  language?: string | null;
  command?: string | null;
  cwd?: string | null;
  status?: string | null;
  exit_code?: number | null;
  metadata_json?: string | null;
  translated_text?: string | null;
}

export interface ConversationQuestion {
  id: string;
  session_id: string;
  question_index: number;
  title?: string | null;
  question_text: string;
  answer_text: string;
  code_text: string;
  command_text: string;
  grouping_origin: ConversationGroupingOrigin;
  created_at: string;
  updated_at: string;
}

export interface ConversationQuestionDetail {
  question: ConversationQuestion;
  turns: ConversationTurn[];
  parts: ConversationPart[];
}

export interface ConversationSessionDetail {
  session: ConversationSession;
  questions: ConversationQuestionDetail[];
}

export type ConversationRecordKind = "session" | "web";

export type ConversationSearchCardType = "question" | "answer" | "tool" | "command" | "code" | "result";

export interface ConversationSearchScope {
  record_kind: ConversationRecordKind;
  adapter_id?: string | null;
  source_id?: string | null;
  project_path?: string | null;
  query: string;
  content_types: ConversationSearchCardType[];
  since?: string | null;
  until?: string | null;
  timeline: boolean;
  limit: number;
  offset: number;
}

export interface ConversationSearchHit {
  session: ConversationSessionListItem;
  question_id: string;
  question_index: number;
  question_title: string;
  turn_id?: string | null;
  part_id?: string | null;
  block_id: string;
  card_type: ConversationSearchCardType;
  snippet: string;
  score: number;
}

export interface ConversationSearchResult {
  query: string;
  record_kind: ConversationRecordKind;
  scope?: ConversationSearchScope;
  total_count: number;
  hits: ConversationSearchHit[];
}

export interface ConversationMutationResult {
  dry_run: boolean;
  session_id: string;
  affected_question_ids: string[];
  questions: ConversationQuestionDetail[];
}

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
  display_target_dir?: string;
  display_target_path?: string;
  display_linked_source?: string | null;
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

export interface AssetGroupIconSvg {
  paths: AssetGroupIconPath[];
  view_box?: string;
}

export interface AssetGroupIconPath {
  clip_rule?: "evenodd" | "nonzero";
  d: string;
  fill_rule?: "evenodd" | "nonzero";
}

export interface AssetGroup {
  id: string;
  name: string;
  description: string | null;
  color: string;
  asset_kind: AssetKind;
  display_icon?: string | null;
  icon_svg?: AssetGroupIconSvg | null;
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
  display_icon?: string | null;
  icon_svg?: AssetGroupIconSvg | null;
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
  display_source_path?: string | null;
  display_target_path?: string;
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
