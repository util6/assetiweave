import { invoke } from "@tauri-apps/api/core";
import type {
  ConversationAdapter,
  ConversationAdapterPackage,
  ConversationAdapterRuntimeGateStatus,
  ConversationPackageUpdatePolicy,
  ConversationSourceKind,
  ConversationMutationResult,
  ConversationQuestionDetail,
  ConversationRecordKind,
  ConversationSearchCardType,
  ConversationSearchHit,
  ConversationSearchResult,
  ConversationSearchScope,
  ConversationSessionDetail,
  ConversationSessionListItem,
  ConversationSource,
} from "../types";

export interface ConversationSessionListParams {
  adapter_id?: string | null;
  source_id?: string | null;
  query?: string | null;
  limit?: number;
  offset?: number;
}

export interface ConversationQuestionListParams {
  session_id: string;
  query?: string | null;
  limit?: number;
  offset?: number;
}

export interface ConversationSearchParams {
  record_kind?: ConversationRecordKind;
  adapter_id?: string | null;
  source_id?: string | null;
  project_path?: string | null;
  query: string;
  content_types?: ConversationSearchCardType[];
  since?: string | null;
  until?: string | null;
  timeline?: boolean;
  limit?: number;
  offset?: number;
}

export interface ConversationExportContentFilter {
  answer: boolean;
  tool: boolean;
  command: boolean;
  code: boolean;
  result: boolean;
}

export interface ConversationAdapterManifest {
  schema_version: number;
  id: string;
  name: string;
  version: string;
  protocol_version: number;
  command: string[];
  runtime?: ConversationAdapterRuntime | null;
  capabilities: string[];
  input_kinds: ConversationSourceKind[];
}

export type ConversationAdapterRuntimeKind = "node" | "python" | "bash" | "executable";

export interface ConversationAdapterRuntime {
  type: ConversationAdapterRuntimeKind;
  entry: string;
  args?: string[];
  version?: string | null;
}

export interface ConversationAdapterRuntimeStatus {
  kind: ConversationAdapterRuntimeKind;
  program: string;
  available: boolean;
  version: string | null;
  required_version: string | null;
  error: string | null;
  hint: string | null;
}

export interface ConversationAdapterValidationResult {
  valid: boolean;
  manifest_path: string;
  content_hash: string;
  manifest_hash: string;
  executable_path: string;
  executable_hash: string | null;
  manifest: ConversationAdapterManifest;
  warnings: string[];
}

export interface ConversationAdapterRegisterResult {
  dry_run: boolean;
  adapter: ConversationAdapter;
  validation: ConversationAdapterValidationResult;
}

export interface ConversationSourceUpsertResult {
  dry_run: boolean;
  source: ConversationSource;
}

export interface ImportConversationSourceParams {
  confirmed?: boolean;
  config_json?: string | null;
  manifest_path: string;
  record_kind?: ConversationRecordKind;
  source_id?: string | null;
  source_kind: ConversationSourceKind;
  source_location: string;
  source_name: string;
}

export interface ImportConversationSourceResult {
  adapter: ConversationAdapter;
  source: ConversationSource;
  task: ConversationSyncTaskSnapshot;
  validation: ConversationAdapterValidationResult;
}

export type ImportConversationSourceProgress = "validating" | "source" | "sync";
export type StartConversationSync = typeof syncConversations;

export type ConversationSyncTaskStatus = "running" | "completed" | "failed";

export type ConversationScriptCatalogSourceKind = "github" | "artifact_zip" | "local_directory";

export interface ConversationScriptCatalogSource {
  type: ConversationScriptCatalogSourceKind;
  url: string;
  branch?: string | null;
  path?: string | null;
}

export interface ConversationScriptCatalogItem {
  id: string;
  name: string;
  version: string;
  record_kind: ConversationRecordKind;
  provider?: string | null;
  adapter_id?: string | null;
  description?: string | null;
  homepage_url?: string | null;
  repository_url?: string | null;
  tags: string[];
  manifest_file?: string | null;
  package_manifest_file?: string | null;
  expected_content_hash?: string | null;
  expected_package_hash?: string | null;
  source: ConversationScriptCatalogSource;
}

export type ConversationAdapterPackageCatalogStatus =
  | "not_installed"
  | "uninstalled"
  | "legacy_installed"
  | "installed"
  | "update_available"
  | "runtime_missing"
  | "verification_failed"
  | "hash_mismatch"
  | "manifest_invalid"
  | "core_incompatible"
  | "built_in"
  | "local_registered"
  | "git_registered"
  | "dev_override"
  | "ahead_of_release";

export interface ConversationAdapterPackageCatalogEntry {
  item: ConversationScriptCatalogItem;
  installed: boolean;
  update_available: boolean;
  ahead_of_release: boolean;
  runtime_ready: boolean;
  status: ConversationAdapterPackageCatalogStatus;
  installed_package?: ConversationAdapterPackage | null;
  installed_adapter?: ConversationAdapter | null;
  install_path?: string | null;
  display_install_path?: string | null;
  display_manifest_path?: string | null;
  error_message?: string | null;
}

export type ConversationAdapterPackageChangeAction =
  | "register"
  | "unregister"
  | "install"
  | "update"
  | "uninstall"
  | "revalidate";

export interface ConversationAdapterPackageInspection {
  origin: ConversationAdapterPackage["origin"];
  package?: ConversationAdapterPackage | null;
  adapter?: ConversationAdapter | null;
  affected_sources: ConversationSource[];
}

export interface ConversationAdapterPackageChangePreflight {
  action: ConversationAdapterPackageChangeAction;
  origin: ConversationAdapterPackage["origin"];
  package_id?: string | null;
  adapter_id?: string | null;
  managed_paths: string[];
  affected_sources: ConversationSource[];
  task_conflicts: string[];
  preserves_conversation_records: boolean;
  risk: "read_only" | "write" | "high_risk_write";
  confirmation_required: boolean;
}

export interface ConversationAdapterCatalogRelease {
  catalog_url: string;
  package_id: string;
  adapter_id: string;
  name: string;
  publisher: string;
  version: string;
  channel: "stable" | "beta";
  released_at?: string | null;
  core_compatibility: string;
  artifact_url: string;
  artifact_size?: number | null;
  artifact_sha256: string;
  changelog_markdown: string;
  breaking_change: boolean;
  runtime_protocol: string;
  record_kind: ConversationRecordKind;
  package_manifest_file: string;
  adapter_manifest_file: string;
  adapter_manifest_json?: string | null;
  source_json?: string | null;
  etag?: string | null;
  fetched_at: string;
}

export interface ConversationAdapterPackageUpdateStatus {
  package_id: string;
  current_version: string;
  latest_compatible_release?: ConversationAdapterCatalogRelease | null;
  update_available: boolean;
}

export interface ConversationAdapterPackageVersion {
  package_id: string;
  version: string;
  install_dir: string;
  artifact_hash?: string | null;
  content_hash: string;
  runtime_gate_status: ConversationAdapterRuntimeGateStatus;
  installed_at: string;
}

export interface ConversationScriptCatalogEntry {
  item: ConversationScriptCatalogItem;
  installed: boolean;
  update_available: boolean;
  installed_adapter?: ConversationAdapter | null;
  install_path?: string | null;
}

export type ConversationScriptInstallTaskStatus = "running" | "completed" | "failed";

export interface ConversationScriptInstallTaskSnapshot {
  id: string;
  status: ConversationScriptInstallTaskStatus;
  item_id: string;
  package_id?: string;
  action?: "install" | "update" | "uninstall";
  version?: string | null;
  catalog_url?: string | null;
  dry_run: boolean;
  phase?: string | null;
  started_at: string;
  finished_at: string | null;
  result: unknown | null;
  error: string | null;
}

export interface ConversationSyncTaskSnapshot {
  id: string;
  status: ConversationSyncTaskStatus;
  source_id: string | null;
  adapter_id: string | null;
  record_kind?: ConversationRecordKind | null;
  dry_run: boolean;
  started_at: string;
  finished_at: string | null;
  result: unknown | null;
  error: string | null;
}

export interface ConversationSyncSummaryCounts {
  sourceCount: number;
  incrementalStatsAvailable: boolean;
  discoveredSessionCount: number;
  changedSessionCount: number;
  skippedSessionCount: number;
  retainedSessionCount: number;
  turnCount: number;
  warningCount: number;
  errorCount: number;
}

interface ConversationSyncResultItem {
  incremental?: unknown;
  session_count?: unknown;
  active_session_count?: unknown;
  skipped_session_count?: unknown;
  retained_session_count?: unknown;
  turn_count?: unknown;
  warning_count?: unknown;
}

export function summarizeConversationSyncTask(
  task: ConversationSyncTaskSnapshot,
): ConversationSyncSummaryCounts | null {
  if (!isRecord(task.result)) {
    return null;
  }
  const results = Array.isArray(task.result.results) ? task.result.results : [];
  const errors = Array.isArray(task.result.errors) ? task.result.errors : [];
  if (results.length === 0 && errors.length === 0) {
    return null;
  }

  return results.reduce<ConversationSyncSummaryCounts>(
    (summary, rawResult) => {
      const result = isRecord(rawResult) ? (rawResult as ConversationSyncResultItem) : {};
      const sessionCount = numberValue(result.session_count);
      const activeSessionCount = numberValue(result.active_session_count);
      const skippedSessionCount = numberValue(result.skipped_session_count);
      const hasActiveSessionCount = result.active_session_count !== undefined;
      return {
        sourceCount: summary.sourceCount + 1,
        incrementalStatsAvailable:
          summary.incrementalStatsAvailable || result.incremental === true,
        discoveredSessionCount:
          summary.discoveredSessionCount + (result.incremental === true ? sessionCount : 0),
        changedSessionCount:
          summary.changedSessionCount +
          (hasActiveSessionCount ? activeSessionCount : Math.max(0, sessionCount - skippedSessionCount)),
        skippedSessionCount: summary.skippedSessionCount + skippedSessionCount,
        retainedSessionCount:
          summary.retainedSessionCount + numberValue(result.retained_session_count),
        turnCount: summary.turnCount + numberValue(result.turn_count),
        warningCount: summary.warningCount + numberValue(result.warning_count),
        errorCount: summary.errorCount,
      };
    },
    {
      sourceCount: 0,
      incrementalStatsAvailable: false,
      discoveredSessionCount: 0,
      changedSessionCount: 0,
      skippedSessionCount: 0,
      retainedSessionCount: 0,
      turnCount: 0,
      warningCount: 0,
      errorCount: errors.length,
    },
  );
}

export async function listConversationAdapters(): Promise<ConversationAdapter[]> {
  try {
    return await invoke<ConversationAdapter[]>("list_conversation_adapters");
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return fallbackAdapters;
  }
}

export async function validateConversationAdapter(
  manifestPath: string,
): Promise<ConversationAdapterValidationResult> {
  try {
    return await invoke<ConversationAdapterValidationResult>("validate_conversation_adapter", {
      params: { manifest_path: manifestPath },
    });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return fallbackConversationAdapterValidation(manifestPath);
  }
}

export async function listConversationAdapterRuntimeStatuses(): Promise<
  ConversationAdapterRuntimeStatus[]
> {
  if (!isTauriRuntime()) {
    return [
      {
        kind: "node",
        program: "node",
        available: true,
        version: "preview",
        required_version: ">=20",
        error: null,
        hint: null,
      },
      {
        kind: "python",
        program: "python3",
        available: false,
        version: null,
        required_version: null,
        error: "Not available in browser preview.",
        hint: "Install Python 3.10 or newer, or set an absolute runtime path in Settings.",
      },
      {
        kind: "bash",
        program: "bash",
        available: true,
        version: "preview",
        required_version: null,
        error: null,
        hint: null,
      },
    ];
  }
  return invoke<ConversationAdapterRuntimeStatus[]>(
    "list_conversation_adapter_runtime_statuses",
  );
}

export async function registerConversationAdapter(
  manifestPath: string,
  dryRun = false,
  confirmed = false,
): Promise<ConversationAdapterRegisterResult> {
  try {
    return await invoke<ConversationAdapterRegisterResult>("register_conversation_adapter", {
      params: { dry_run: dryRun, manifest_path: manifestPath, yes: confirmed },
    });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    const validation = fallbackConversationAdapterValidation(manifestPath);
    return {
      dry_run: dryRun,
      adapter: conversationAdapterFromValidation(validation),
      validation,
    };
  }
}

export async function upsertConversationSource(
  source: ConversationSource,
  dryRun = false,
): Promise<ConversationSourceUpsertResult> {
  try {
    return await invoke<ConversationSourceUpsertResult>("upsert_conversation_source", {
      params: { dry_run: dryRun, source },
    });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return { dry_run: dryRun, source };
  }
}

export async function importConversationSource(
  params: ImportConversationSourceParams,
  onProgress?: (step: ImportConversationSourceProgress) => void,
  startSync: StartConversationSync = syncConversations,
): Promise<ImportConversationSourceResult> {
  onProgress?.("validating");
  const validation = await validateConversationAdapter(params.manifest_path);
  if (!validation.manifest.capabilities.includes("read_session")) {
    throw new Error("conversation adapter must declare read_session");
  }
  if (params.record_kind === "web" && !validation.manifest.capabilities.includes("web_records")) {
    throw new Error("web record imports require an adapter with web_records capability");
  }
  if (params.record_kind !== "web" && validation.manifest.capabilities.includes("web_records")) {
    throw new Error("web record adapters must be imported from the web records page");
  }
  if (!validation.manifest.input_kinds.includes(params.source_kind)) {
    throw new Error(`conversation adapter does not support source kind: ${params.source_kind}`);
  }

  onProgress?.("source");
  const registration = await registerConversationAdapter(
    validation.manifest_path,
    false,
    params.confirmed ?? false,
  );
  const nowIso = new Date().toISOString();
  const source: ConversationSource = {
    id:
      params.source_id?.trim() ||
      conversationSourceId(registration.adapter.id, params.source_location),
    adapter_id: registration.adapter.id,
    name: params.source_name.trim() || registration.adapter.name,
    kind: params.source_kind,
    location: params.source_location.trim(),
    config_json: normalizeOptionalJson(params.config_json),
    enabled: true,
    created_at: nowIso,
    updated_at: nowIso,
  };

  const upsert = await upsertConversationSource(source, false);
  onProgress?.("sync");
  const task = await startSync({
    dry_run: false,
    record_kind: params.record_kind ?? "session",
    source_id: upsert.source.id,
  });

  return {
    adapter: registration.adapter,
    source: upsert.source,
    task,
    validation,
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function numberValue(value: unknown) {
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

function normalizeOptionalJson(value: string | null | undefined) {
  const trimmed = value?.trim();
  if (!trimmed) {
    return null;
  }
  JSON.parse(trimmed);
  return trimmed;
}

function conversationSourceId(adapterId: string, location: string) {
  const locationSlug = location
    .trim()
    .toLowerCase()
    .replace(/^~\//, "home/")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 48);
  return [adapterId, locationSlug || "source"].join("-");
}

function fallbackConversationAdapterValidation(
  manifestPath: string,
): ConversationAdapterValidationResult {
  const webLike = /web|browser|qwen|chatgpt/i.test(manifestPath);
  return {
    valid: true,
    manifest_path: manifestPath,
    content_hash: "preview-content-hash",
    manifest_hash: "preview-manifest-hash",
    executable_path: `${manifestPath.replace(/\/[^/]*$/, "")}/adapter`,
    executable_hash: "preview-executable-hash",
    manifest: {
      schema_version: 1,
      id: webLike ? "preview-web-adapter" : "preview-conversation-adapter",
      name: webLike ? "Preview Web Adapter" : "Preview Conversation Adapter",
      version: "0.1.0",
      protocol_version: 1,
      command: ["adapter"],
      capabilities: webLike
        ? ["probe", "read_session", "web_records"]
        : ["probe", "read_session"],
      input_kinds: ["directory", "file", "sqlite"],
    },
    warnings: [],
  };
}

function conversationAdapterFromValidation(
  validation: ConversationAdapterValidationResult,
): ConversationAdapter {
  const nowIso = new Date().toISOString();
  return {
    id: validation.manifest.id,
    name: validation.manifest.name,
    kind: "external",
    version: validation.manifest.version,
    enabled: true,
    manifest_path: validation.manifest_path,
    executable_path: validation.executable_path,
    content_hash: validation.content_hash,
    trusted_hash: validation.content_hash,
    trust_state: "trusted",
    protocol_version: validation.manifest.protocol_version,
    capabilities: validation.manifest.capabilities,
    input_kinds: validation.manifest.input_kinds,
    created_at: nowIso,
    updated_at: nowIso,
  };
}

export async function listConversationSources(): Promise<ConversationSource[]> {
  try {
    return await invoke<ConversationSource[]>("list_conversation_sources");
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return fallbackSources;
  }
}

export async function listConversationScriptCatalog(
  catalogUrl?: string | null,
): Promise<ConversationScriptCatalogEntry[]> {
  try {
    return await invoke<ConversationScriptCatalogEntry[]>("list_conversation_script_catalog", {
      params: { catalog_url: catalogUrl?.trim() || null },
    });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return fallbackConversationScriptCatalogEntries();
  }
}

export async function listConversationAdapterPackages(
  catalogUrl?: string | null,
): Promise<ConversationAdapterPackageCatalogEntry[]> {
  try {
    return await invoke<ConversationAdapterPackageCatalogEntry[]>("list_conversation_adapter_packages", {
      params: { catalog_url: catalogUrl?.trim() || null },
    });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return fallbackConversationAdapterPackageCatalogEntries();
  }
}

export async function inspectConversationAdapterPackage(params: {
  packageId?: string | null;
  adapterId?: string | null;
}): Promise<ConversationAdapterPackageInspection> {
  return await invoke<ConversationAdapterPackageInspection>("inspect_conversation_adapter_package", {
    params: {
      package_id: params.packageId?.trim() || null,
      adapter_id: params.adapterId?.trim() || null,
    },
  });
}

export async function prepareConversationAdapterPackageChange(params: {
  action: ConversationAdapterPackageChangeAction;
  packageId?: string | null;
  adapterId?: string | null;
}): Promise<ConversationAdapterPackageChangePreflight> {
  return await invoke<ConversationAdapterPackageChangePreflight>(
    "prepare_conversation_adapter_package_change",
    {
      params: {
        action: params.action,
        package_id: params.packageId?.trim() || null,
        adapter_id: params.adapterId?.trim() || null,
      },
    },
  );
}

export async function listConversationAdapterPackageReleases(params: {
  packageId: string;
  catalogUrl?: string | null;
  refresh?: boolean;
}): Promise<ConversationAdapterCatalogRelease[]> {
  return await invoke<ConversationAdapterCatalogRelease[]>(
    "list_conversation_adapter_package_releases",
    {
      params: {
        catalog_url: params.catalogUrl?.trim() || null,
        package_id: params.packageId,
        refresh: params.refresh ?? false,
      },
    },
  );
}

export async function listInstalledConversationAdapterPackageVersions(
  packageId: string,
): Promise<ConversationAdapterPackageVersion[]> {
  return await invoke<ConversationAdapterPackageVersion[]>(
    "list_installed_conversation_adapter_package_versions",
    { params: { package_id: packageId } },
  );
}

export async function switchConversationAdapterPackageVersion(params: {
  packageId: string;
  version: string;
  dryRun?: boolean;
  confirmed?: boolean;
}): Promise<unknown> {
  return await invoke("switch_conversation_adapter_package_version", {
    params: {
      package_id: params.packageId,
      version: params.version,
      dry_run: params.dryRun ?? false,
      yes: params.confirmed ?? false,
    },
  });
}

export async function rollbackConversationAdapterPackageVersion(params: {
  packageId: string;
  dryRun?: boolean;
  confirmed?: boolean;
}): Promise<unknown> {
  return await invoke("rollback_conversation_adapter_package_version", {
    params: {
      package_id: params.packageId,
      version: null,
      dry_run: params.dryRun ?? false,
      yes: params.confirmed ?? false,
    },
  });
}

export async function deleteConversationAdapterPackageVersion(params: {
  packageId: string;
  version: string;
  dryRun?: boolean;
  confirmed?: boolean;
}): Promise<unknown> {
  return await invoke("delete_conversation_adapter_package_version", {
    params: {
      package_id: params.packageId,
      version: params.version,
      dry_run: params.dryRun ?? false,
      yes: params.confirmed ?? false,
    },
  });
}

export async function refreshConversationAdapterCatalogs(params?: {
  catalogUrl?: string | null;
  force?: boolean;
}): Promise<ConversationAdapterCatalogRelease[]> {
  return await invoke<ConversationAdapterCatalogRelease[]>(
    "refresh_conversation_adapter_catalogs",
    {
      params: {
        catalog_url: params?.catalogUrl?.trim() || null,
        force: params?.force ?? true,
      },
    },
  );
}

export async function checkConversationAdapterPackageUpdates(params?: {
  catalogUrl?: string | null;
  force?: boolean;
}): Promise<ConversationAdapterPackageUpdateStatus[]> {
  return await invoke<ConversationAdapterPackageUpdateStatus[]>(
    "check_conversation_adapter_package_updates",
    {
      params: {
        catalog_url: params?.catalogUrl?.trim() || null,
        force: params?.force ?? false,
      },
    },
  );
}

export async function setConversationAdapterPackageUpdatePolicy(params: {
  packageId: string;
  updatePolicy: ConversationPackageUpdatePolicy;
}): Promise<ConversationAdapterPackage> {
  return await invoke<ConversationAdapterPackage>(
    "set_conversation_adapter_package_update_policy",
    {
      params: {
        package_id: params.packageId,
        update_policy: params.updatePolicy,
      },
    },
  );
}

export async function installConversationAdapterPackage(params: {
  packageId: string;
  version?: string | null;
  catalogUrl?: string | null;
  dryRun?: boolean;
  confirmed?: boolean;
}): Promise<ConversationScriptInstallTaskSnapshot> {
  try {
    return await invoke<ConversationScriptInstallTaskSnapshot>("install_conversation_adapter_package", {
      params: {
        catalog_url: params.catalogUrl?.trim() || null,
        dry_run: params.dryRun ?? false,
        package_id: params.packageId,
        version: params.version?.trim() || null,
        yes: params.confirmed ?? false,
      },
    });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return fallbackPackageTask(params.packageId, params.catalogUrl, params.dryRun);
  }
}

export async function updateConversationAdapterPackage(params: {
  packageId: string;
  version?: string | null;
  catalogUrl?: string | null;
  dryRun?: boolean;
  confirmed?: boolean;
}): Promise<ConversationScriptInstallTaskSnapshot> {
  try {
    return await invoke<ConversationScriptInstallTaskSnapshot>("update_conversation_adapter_package", {
      params: {
        catalog_url: params.catalogUrl?.trim() || null,
        dry_run: params.dryRun ?? false,
        package_id: params.packageId,
        version: params.version?.trim() || null,
        yes: params.confirmed ?? false,
      },
    });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return fallbackPackageTask(params.packageId, params.catalogUrl, params.dryRun);
  }
}

export async function uninstallConversationAdapterPackage(params: {
  packageId: string;
  dryRun?: boolean;
  confirmed?: boolean;
}): Promise<ConversationScriptInstallTaskSnapshot> {
  return await invoke<ConversationScriptInstallTaskSnapshot>("uninstall_conversation_adapter_package", {
    params: {
      dry_run: params.dryRun ?? false,
      package_id: params.packageId,
      yes: params.confirmed ?? false,
    },
  });
}

export async function unregisterConversationAdapter(params: {
  adapterId: string;
  dryRun?: boolean;
  confirmed?: boolean;
}): Promise<unknown> {
  return await invoke("unregister_conversation_adapter", {
    params: {
      adapter_id: params.adapterId,
      dry_run: params.dryRun ?? false,
      yes: params.confirmed ?? false,
    },
  });
}

export async function installConversationScript(params: {
  itemId: string;
  catalogUrl?: string | null;
  dryRun?: boolean;
}): Promise<ConversationScriptInstallTaskSnapshot> {
  try {
    return await invoke<ConversationScriptInstallTaskSnapshot>("install_conversation_script", {
      params: {
        catalog_url: params.catalogUrl?.trim() || null,
        dry_run: params.dryRun ?? false,
        item_id: params.itemId,
        yes: params.dryRun ? false : true,
      },
    });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return {
      id: `preview-script-install-${Date.now()}`,
      status: "completed",
      item_id: params.itemId,
      package_id: params.itemId,
      catalog_url: params.catalogUrl ?? null,
      dry_run: Boolean(params.dryRun),
      phase: "completed",
      started_at: new Date().toISOString(),
      finished_at: new Date().toISOString(),
      result: { installed: true },
      error: null,
    };
  }
}

export async function getConversationAdapterPackageTask(): Promise<
  ConversationScriptInstallTaskSnapshot | null
> {
  try {
    return await invoke<ConversationScriptInstallTaskSnapshot | null>(
      "get_conversation_adapter_package_task",
    );
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }
    return null;
  }
}

export async function getConversationScriptInstallTask(): Promise<
  ConversationScriptInstallTaskSnapshot | null
> {
  try {
    return await invoke<ConversationScriptInstallTaskSnapshot | null>(
      "get_conversation_script_install_task",
    );
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }
    return null;
  }
}

export async function syncConversations(
  params: {
    source_id?: string | null;
    adapter_id?: string | null;
    dry_run?: boolean;
    record_kind?: ConversationRecordKind | null;
  },
): Promise<ConversationSyncTaskSnapshot> {
  try {
    return await invoke<ConversationSyncTaskSnapshot>("sync_conversations", { params });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    const recordKind = params.record_kind ?? "session";
    return {
      id: "preview-conversation-sync",
      status: "completed",
      source_id: params.source_id ?? null,
      adapter_id: params.adapter_id ?? null,
      record_kind: recordKind,
      dry_run: Boolean(params.dry_run),
      started_at: new Date().toISOString(),
      finished_at: new Date().toISOString(),
      result: {
        dry_run: Boolean(params.dry_run),
        errors: [],
        results: [
          {
            source_id: "codex-live",
            adapter_id: "codex",
            dry_run: Boolean(params.dry_run),
            record_kind: recordKind,
            session_count: fallbackSessions.length,
            active_session_count: fallbackSessions.length,
            skipped_session_count: 0,
            retained_session_count: 0,
            turn_count: fallbackSessions.reduce((total, session) => total + session.turn_count, 0),
            warning_count: 0,
            warnings: [],
          },
        ],
      },
      error: null,
    };
  }
}

export async function getConversationSyncTask(): Promise<ConversationSyncTaskSnapshot | null> {
  try {
    return await invoke<ConversationSyncTaskSnapshot | null>("get_conversation_sync_task");
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }
    return null;
  }
}

export async function listConversationSyncTasks(): Promise<ConversationSyncTaskSnapshot[]> {
  try {
    return await invoke<ConversationSyncTaskSnapshot[]>("list_conversation_sync_tasks");
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }
    return [];
  }
}

export async function listConversationSessions(params: ConversationSessionListParams): Promise<ConversationSessionListItem[]> {
  try {
    return await invoke<ConversationSessionListItem[]>("list_conversation_sessions", { params });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return fallbackSessions.filter((session) => {
      if (params.adapter_id && session.adapter_id !== params.adapter_id) return false;
      if (params.source_id && session.source_id !== params.source_id) return false;
      if (params.query && !`${session.title} ${session.project_path ?? ""}`.toLowerCase().includes(params.query.toLowerCase())) return false;
      return true;
    });
  }
}

export async function getConversationSession(sessionId: string): Promise<ConversationSessionDetail> {
  try {
    return await invoke<ConversationSessionDetail>("get_conversation_session", { params: { session_id: sessionId } });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return fallbackSessionDetail;
  }
}

export async function listWebRecordSessions(params: ConversationSessionListParams): Promise<ConversationSessionListItem[]> {
  try {
    return await invoke<ConversationSessionListItem[]>("list_web_record_sessions", { params });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return fallbackWebSessions.filter((session) => {
      if (params.adapter_id && session.adapter_id !== params.adapter_id) return false;
      if (params.source_id && session.source_id !== params.source_id) return false;
      if (params.query && !session.title.toLowerCase().includes(params.query.toLowerCase())) return false;
      return true;
    });
  }
}

export async function getWebRecordSession(sessionId: string): Promise<ConversationSessionDetail> {
  try {
    return await invoke<ConversationSessionDetail>("get_web_record_session", { params: { session_id: sessionId } });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return fallbackWebSessionDetail;
  }
}

export async function searchConversationRecords(params: ConversationSearchParams): Promise<ConversationSearchResult> {
  const trimmedQuery = params.query.trim();
  if (!trimmedQuery) {
    const recordKind = params.record_kind ?? "session";
    const limit = params.limit ?? 50;
    const offset = params.offset ?? 0;
    return {
      query: "",
      record_kind: recordKind,
      scope: conversationSearchScope({
        ...params,
        query: "",
        record_kind: recordKind,
        content_types: params.content_types ?? [],
        limit,
        offset,
        timeline: params.timeline ?? false,
      }),
      total_count: 0,
      hits: [],
    };
  }

  const payload = {
    ...params,
    query: trimmedQuery,
    record_kind: params.record_kind ?? "session",
    content_types: params.content_types ?? [],
    since: params.since ?? null,
    until: params.until ?? null,
    timeline: params.timeline ?? false,
    limit: params.limit ?? 50,
    offset: params.offset ?? 0,
  };

  try {
    return await invoke<ConversationSearchResult>("search_conversation_records", { params: payload });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return fallbackConversationSearch(payload);
  }
}

export async function listConversationQuestions(params: ConversationQuestionListParams): Promise<ConversationQuestionDetail[]> {
  try {
    return await invoke<ConversationQuestionDetail[]>("list_conversation_questions", { params });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return fallbackSessionDetail.questions;
  }
}

export async function getConversationQuestion(questionId: string): Promise<ConversationQuestionDetail> {
  try {
    return await invoke<ConversationQuestionDetail>("get_conversation_question", { params: { question_id: questionId } });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return fallbackSessionDetail.questions.find((question) => question.question.id === questionId) ?? fallbackSessionDetail.questions[0];
  }
}

export async function mergeConversationQuestions(questionIds: string[], dryRun = false): Promise<ConversationMutationResult> {
  try {
    return await invoke<ConversationMutationResult>("merge_conversation_questions", {
      params: { question_ids: questionIds, dry_run: dryRun },
    });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return {
      dry_run: dryRun,
      session_id: fallbackSessionDetail.session.id,
      affected_question_ids: questionIds,
      questions: fallbackSessionDetail.questions.filter((question) => questionIds.includes(question.question.id)),
    };
  }
}

export async function splitConversationQuestion(questionId: string, beforeTurnId: string, dryRun = false): Promise<ConversationMutationResult> {
  try {
    return await invoke<ConversationMutationResult>("split_conversation_question", {
      params: { question_id: questionId, before_turn_id: beforeTurnId, dry_run: dryRun },
    });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return {
      dry_run: dryRun,
      session_id: fallbackSessionDetail.session.id,
      affected_question_ids: [questionId],
      questions: fallbackSessionDetail.questions.filter((question) => question.question.id === questionId || question.turns.some((turn) => turn.id === beforeTurnId)),
    };
  }
}

export async function exportConversationSession(
  sessionId: string,
  outputRoot: string,
  dryRun = false,
  questionIds: string[] = [],
  contentFilter?: ConversationExportContentFilter,
) {
  const resolvedContentFilter = contentFilter ?? {
    answer: true,
    code: true,
    command: true,
    result: true,
    tool: true,
  };
  try {
    return await invoke("export_conversation_session", {
      params: {
        session_id: sessionId,
        output_root: outputRoot,
        question_ids: questionIds,
        content_filter: resolvedContentFilter,
        dry_run: dryRun,
      },
    });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return {
      dry_run: dryRun,
      session_id: sessionId,
      question_ids: questionIds,
      output_path: `${outputRoot}/codex/preview-project/preview-conversation-session-preview.md`,
    };
  }
}

export async function exportWebRecordSession(
  sessionId: string,
  outputRoot: string,
  dryRun = false,
  questionIds: string[] = [],
  contentFilter?: ConversationExportContentFilter,
) {
  const resolvedContentFilter = contentFilter ?? {
    answer: true,
    code: true,
    command: true,
    result: true,
    tool: true,
  };
  try {
    return await invoke("export_web_record_session", {
      params: {
        session_id: sessionId,
        output_root: outputRoot,
        question_ids: questionIds,
        content_filter: resolvedContentFilter,
        dry_run: dryRun,
      },
    });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return {
      dry_run: dryRun,
      session_id: sessionId,
      question_ids: questionIds,
      output_path: `${outputRoot}/qwen-web/web/preview-web-record.md`,
    };
  }
}

function fallbackConversationSearch(params: Required<Pick<ConversationSearchParams, "query" | "record_kind" | "content_types" | "limit" | "offset" | "timeline">> & ConversationSearchParams): ConversationSearchResult {
  const detail = params.record_kind === "web" ? fallbackWebSessionDetail : fallbackSessionDetail;
  const session = params.record_kind === "web" ? fallbackWebSessions[0] : fallbackSessions[0];
  const needle = params.query.trim().toLowerCase();
  if (params.record_kind !== "web" && params.project_path && session.project_path !== params.project_path) {
    return {
      query: params.query,
      record_kind: params.record_kind,
      scope: conversationSearchScope(params),
      total_count: 0,
      hits: [],
    };
  }
  if (!conversationSessionWithinSearchTime(session, params.since, params.until)) {
    return {
      query: params.query,
      record_kind: params.record_kind,
      scope: conversationSearchScope(params),
      total_count: 0,
      hits: [],
    };
  }
  const allowedTypes = new Set(params.content_types);
  const hits: ConversationSearchHit[] = [];

  for (const questionDetail of detail.questions) {
    const questionTitle = questionDetail.question.title || firstLine(questionDetail.question.question_text);
    for (const turn of questionDetail.turns) {
      pushFallbackHit(hits, {
        allowedTypes,
        blockId: `${turn.id}-question`,
        cardType: "question",
        needle,
        partId: null,
        questionDetail,
        questionTitle,
        session,
        text: turn.user_text,
        turnId: turn.id,
      });

      for (const part of questionDetail.parts.filter((candidate) => candidate.turn_id === turn.id)) {
        for (const entry of fallbackEntriesForPart(part)) {
          pushFallbackHit(hits, {
            allowedTypes,
            blockId: entry.blockId,
            cardType: entry.cardType,
            needle,
            partId: part.id,
            questionDetail,
            questionTitle,
            session,
            text: entry.text,
            turnId: turn.id,
          });
        }
      }
    }
  }

  return {
    query: params.query,
    record_kind: params.record_kind,
    scope: conversationSearchScope(params),
    total_count: hits.length,
    hits: hits.slice(params.offset, params.offset + params.limit),
  };
}

function conversationSearchScope(params: Required<Pick<ConversationSearchParams, "query" | "record_kind" | "content_types" | "limit" | "offset" | "timeline">> & ConversationSearchParams): ConversationSearchScope {
  return {
    record_kind: params.record_kind,
    adapter_id: params.adapter_id ?? null,
    source_id: params.source_id ?? null,
    project_path: params.project_path ?? null,
    query: params.query,
    content_types: params.content_types,
    since: params.since ?? null,
    until: params.until ?? null,
    timeline: params.timeline,
    limit: params.limit,
    offset: params.offset,
  };
}

function conversationSessionWithinSearchTime(session: ConversationSessionListItem, since?: string | null, until?: string | null) {
  if (!since && !until) return true;
  const sessionTime = Date.parse(session.started_at ?? session.updated_at ?? session.imported_at);
  if (!Number.isFinite(sessionTime)) return false;
  const sinceTime = since ? Date.parse(searchDateBound(since, "start")) : Number.NEGATIVE_INFINITY;
  const untilTime = until ? Date.parse(searchDateBound(until, "end")) : Number.POSITIVE_INFINITY;
  return sessionTime >= sinceTime && sessionTime <= untilTime;
}

function searchDateBound(value: string, bound: "start" | "end") {
  return /^\d{4}-\d{2}-\d{2}$/.test(value)
    ? `${value}T${bound === "start" ? "00:00:00.000Z" : "23:59:59.999Z"}`
    : value;
}

function pushFallbackHit(
  hits: ConversationSearchHit[],
  params: {
    allowedTypes: Set<ConversationSearchCardType>;
    blockId: string;
    cardType: ConversationSearchCardType;
    needle: string;
    partId: string | null;
    questionDetail: ConversationQuestionDetail;
    questionTitle: string;
    session: ConversationSessionListItem;
    text?: string | null;
    turnId: string;
  },
) {
  const text = params.text?.trim();
  if (!text) return;
  if (params.allowedTypes.size > 0 && !params.allowedTypes.has(params.cardType)) return;
  if (!text.toLowerCase().includes(params.needle)) return;

  hits.push({
    block_id: params.blockId,
    card_type: params.cardType,
    part_id: params.partId,
    question_id: params.questionDetail.question.id,
    question_index: params.questionDetail.question.question_index,
    question_title: params.questionTitle,
    score: Math.max(1, text.toLowerCase().split(params.needle).length - 1) * 100,
    session: params.session,
    snippet: fallbackSnippet(text, params.needle),
    turn_id: params.turnId,
  });
}

function fallbackEntriesForPart(part: ConversationQuestionDetail["parts"][number]) {
  const declaredCard = declaredContentCard(part.metadata_json);
  if (!declaredCard) return [];

  const cardType = declaredCard.type;
  const primaryText = declaredCard.text
    ?? (cardType === "command"
      ? part.command ?? part.text
      : part.text ?? part.command);
  return fallbackEntry(part.id, cardType, primaryText, declaredCard.suffix ?? cardType);
}

function fallbackEntry(
  partId: string,
  cardType: ConversationSearchCardType,
  text?: string | null,
  suffix: string = cardType,
) {
  const trimmedText = text?.trim();
  return trimmedText ? [{ blockId: `${partId}-${suffix}`, cardType, text: trimmedText }] : [];
}

interface DeclaredContentCard {
  suffix?: string;
  text?: string;
  type: ConversationSearchCardType;
}

function declaredContentCard(metadataJson?: string | null): DeclaredContentCard | null {
  const metadata = parseMetadata(metadataJson);
  if (!metadata) return null;
  const card = metadata.content_card ?? metadata.contentCard;
  if (!isRecord(card)) return null;
  const type = card.type;
  if (
    type === "answer"
    || type === "tool"
    || type === "command"
    || type === "code"
    || type === "result"
  ) {
    return {
      suffix: stringValue(card.suffix),
      text: stringValue(card.text),
      type,
    };
  }
  return null;
}

function stringValue(value: unknown) {
  return typeof value === "string" && value.trim() ? value : undefined;
}

function parseMetadata(metadataJson?: string | null): Record<string, unknown> | null {
  if (!metadataJson?.trim()) return null;
  try {
    const parsed = JSON.parse(metadataJson);
    return isRecord(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

function fallbackSnippet(text: string, needle: string) {
  const index = text.toLowerCase().indexOf(needle);
  const start = Math.max(0, index - 64);
  const end = Math.min(text.length, index + needle.length + 96);
  return `${start > 0 ? "..." : ""}${text.slice(start, end)}${end < text.length ? "..." : ""}`
    .split(/\s+/)
    .join(" ");
}

function firstLine(value: string) {
  return value.split(/\r?\n/).find((line) => line.trim())?.trim() ?? "Untitled question";
}

function isTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

const now = new Date().toISOString();

const fallbackAdapters: ConversationAdapter[] = [
  {
    id: "codex",
    name: "Codex",
    kind: "external",
    version: "1.0.0",
    enabled: true,
    manifest_path: "~/.assetiweave/conversation-adapters/codex/conversation-adapter.json",
    executable_path: "~/.assetiweave/conversation-adapters/codex/adapter.mjs",
    trust_state: "built_in",
    capabilities: ["probe", "list_sessions", "read_session"],
    input_kinds: ["live", "file"],
    created_at: now,
    updated_at: now,
  },
  {
    id: "claude-code",
    name: "Claude Code",
    kind: "external",
    version: "1.0.0",
    enabled: true,
    manifest_path: "~/.assetiweave/conversation-adapters/claude-code/conversation-adapter.json",
    executable_path: "~/.assetiweave/conversation-adapters/claude-code/adapter.mjs",
    trust_state: "built_in",
    capabilities: ["probe", "list_sessions", "read_session"],
    input_kinds: ["live", "directory", "file"],
    created_at: now,
    updated_at: now,
  },
  {
    id: "opencode",
    name: "OpenCode",
    kind: "external",
    version: "1.0.0",
    enabled: true,
    manifest_path: "~/.assetiweave/conversation-adapters/opencode/conversation-adapter.json",
    executable_path: "~/.assetiweave/conversation-adapters/opencode/adapter.mjs",
    trust_state: "built_in",
    capabilities: ["probe", "list_sessions", "read_session"],
    input_kinds: ["live", "sqlite"],
    created_at: now,
    updated_at: now,
  },
  {
    id: "qwen-web",
    name: "Qwen Web",
    kind: "external",
    version: "0.1.0",
    enabled: true,
    manifest_path: "~/.assetiweave/conversation-adapters/market/qwen-web/conversation-adapter.json",
    executable_path: "~/.assetiweave/conversation-adapters/market/qwen-web/adapter.js",
    trust_state: "trusted",
    capabilities: ["probe", "read_session", "web_records"],
    input_kinds: ["directory"],
    created_at: now,
    updated_at: now,
  },
  {
    id: "zcode",
    name: "ZCode",
    kind: "external",
    version: "0.2.1",
    enabled: true,
    manifest_path: "~/.assetiweave/conversation-adapters/market/zcode-session/conversation-adapter.json",
    executable_path: "~/.assetiweave/conversation-adapters/market/zcode-session/zcode_adapter.py",
    trust_state: "trusted",
    capabilities: ["probe", "list_sessions", "read_session"],
    input_kinds: ["sqlite"],
    created_at: now,
    updated_at: now,
  },
  {
    id: "chatgpt-web",
    name: "ChatGPT Web",
    kind: "external",
    version: "0.1.0",
    enabled: true,
    manifest_path: "~/.assetiweave/conversation-adapters/market/chatgpt-web/conversation-adapter.json",
    executable_path: "~/.assetiweave/conversation-adapters/market/chatgpt-web/adapter.js",
    trust_state: "trusted",
    capabilities: ["probe", "read_session", "web_records"],
    input_kinds: ["directory"],
    created_at: now,
    updated_at: now,
  },
  {
    id: "gemini-web",
    name: "Gemini Web",
    kind: "external",
    version: "0.1.2",
    enabled: true,
    manifest_path: "~/.assetiweave/conversation-adapters/market/gemini-web/conversation-adapter.json",
    executable_path: "~/.assetiweave/conversation-adapters/market/gemini-web/adapter.js",
    trust_state: "trusted",
    capabilities: ["probe", "read_session", "web_records"],
    input_kinds: ["directory"],
    created_at: now,
    updated_at: now,
  },
];

function fallbackConversationScriptCatalogEntries(): ConversationScriptCatalogEntry[] {
  const items: ConversationScriptCatalogItem[] = [
    {
      id: "codex-session",
      name: "Codex Session Parser",
      version: "1.0.0",
      record_kind: "session",
      provider: "codex",
      adapter_id: "codex",
      description: "Reads local Codex session records and exports normalized conversation turns.",
      repository_url: "https://github.com/util6/assetiweave",
      tags: ["session", "codex", "node"],
      expected_package_hash: "9289a6e3da31a0f0b5d1880921e0237efdecf8fcadd6a96f439e60209ce42f78",
      expected_content_hash: "7cc193fcb5db8f7536792fd7480e376a9ca1acbca9c201736744304b599db094",
      source: {
        type: "github",
        url: "https://github.com/util6/assetiweave/tree/main/parser-catalog/adapters/codex",
      },
    },
    {
      id: "opencode-session",
      name: "OpenCode Session Parser",
      version: "1.0.0",
      record_kind: "session",
      provider: "opencode",
      adapter_id: "opencode",
      description: "Reads OpenCode SQLite state and converts sessions into conversation records.",
      repository_url: "https://github.com/util6/assetiweave",
      tags: ["session", "opencode", "sqlite", "node"],
      expected_package_hash: "6ed55931a1e4f43506ac27838543a1fa2d045a63f395f00ea8a0a2d8dd63c344",
      expected_content_hash: "7402082acd6351b771383f98988bf0a88ae1c5093b278ecce5d946df6884bd7e",
      source: {
        type: "github",
        url: "https://github.com/util6/assetiweave/tree/main/parser-catalog/adapters/opencode",
      },
    },
    {
      id: "claude-code-session",
      name: "Claude Code Session Parser",
      version: "1.0.0",
      record_kind: "session",
      provider: "claude-code",
      adapter_id: "claude-code",
      description: "Reads Claude Code project conversations and emits the shared external adapter protocol.",
      repository_url: "https://github.com/util6/assetiweave",
      tags: ["session", "claude-code", "node"],
      expected_package_hash: "f1ca450d6936012f6cbf8cc1e0046b625fcfa7175025a08d3fe5c5bd82be2a97",
      expected_content_hash: "84768c83036672f6a6569f9b22914352f58dc677ca4764a7387788376d64a475",
      source: {
        type: "github",
        url: "https://github.com/util6/assetiweave/tree/main/parser-catalog/adapters/claude-code",
      },
    },
    {
      id: "zcode-session",
      name: "ZCode Session Parser",
      version: "0.1.0",
      record_kind: "session",
      provider: "zcode",
      adapter_id: "zcode",
      description: "Reads ZCode SQLite conversation records using the existing external adapter script.",
      repository_url: "https://github.com/util6/assetiweave",
      tags: ["session", "zcode", "sqlite", "python"],
      expected_package_hash: "e81a32d1266f199faf37b267691de5b6c12e6dd9acc99b44e384b6bc7630abe5",
      expected_content_hash: "5a50814a30a7894ee5243873bce8cb175ffdce9fab8d6a75324b5d446837c044",
      source: {
        type: "github",
        url: "https://github.com/util6/assetiweave/tree/main/parser-catalog/adapters/zcode",
      },
    },
    {
      id: "chatgpt-web",
      name: "ChatGPT Web Harvester",
      version: "0.1.0",
      record_kind: "web",
      provider: "chatgpt",
      adapter_id: "chatgpt-web",
      description: "Collects ChatGPT web conversations and exposes normalized web records through the adapter protocol.",
      repository_url: "https://github.com/util6/assetiweave",
      tags: ["web", "chatgpt", "node", "browser-cookie-auth"],
      expected_package_hash: "9d76886074b7835a3ce60e2a9081e3962d4a1681ad67724ed20f0137b4b6e90b",
      expected_content_hash: "1b00dd931991ecfbe19954b4dd59cb92513fc28f43a0d8d1f129f275e737aa31",
      source: {
        type: "github",
        url: "https://github.com/util6/assetiweave/tree/main/parser-catalog/adapters/chatgpt-web",
      },
    },
    {
      id: "qwen-web",
      name: "Qwen Web Harvester",
      version: "0.1.0",
      record_kind: "web",
      provider: "qwen",
      adapter_id: "qwen-web",
      description: "Collects Qwen web conversations and exposes normalized web records through the adapter protocol.",
      repository_url: "https://github.com/util6/assetiweave",
      tags: ["web", "qwen", "node", "browser-cookie-auth"],
      expected_package_hash: "e4540023a6f615f79d5653bf166d1381fb5b779e083a8d11defdbea02e255770",
      expected_content_hash: "3c485df513a682713de1a946e69c19ddf2d6ed86e68e926e7af4f81338971756",
      source: {
        type: "github",
        url: "https://github.com/util6/assetiweave/tree/main/parser-catalog/adapters/qwen-web",
      },
    },
    {
      id: "gemini-web",
      name: "Gemini Web Harvester",
      version: "0.1.2",
      record_kind: "web",
      provider: "gemini",
      adapter_id: "gemini-web",
      description: "Collects Gemini web conversations and exposes normalized web records through the adapter protocol.",
      repository_url: "https://github.com/util6/assetiweave",
      tags: ["web", "gemini", "node", "browser-cookie-auth"],
      expected_package_hash: "e0fd8d5c0add44370c3e3aef249e6cedd1eda20d87c389ddaefb86f3f83856ea",
      expected_content_hash: "20f277c789a111d06f87b30be7523905826d9cb63b7194f5bd18fcf6bc8bfd76",
      source: {
        type: "github",
        url: "https://github.com/util6/assetiweave/tree/main/parser-catalog/adapters/gemini-web",
      },
    },
  ];

  return items.map((item) => {
    const adapterId = item.adapter_id ?? item.id;
    const installedAdapter = fallbackAdapters.find((adapter) => adapter.id === adapterId) ?? null;
    const versionState = conversationAdapterVersionState(installedAdapter?.version, item.version);
    return {
      item,
      installed: Boolean(installedAdapter),
      update_available: versionState.update_available,
      installed_adapter: installedAdapter,
      install_path: installedAdapter?.manifest_path?.replace(/\/conversation-adapter\.json$/, "") ?? null,
    };
  });
}

function fallbackConversationAdapterPackageCatalogEntries(): ConversationAdapterPackageCatalogEntry[] {
  return fallbackConversationScriptCatalogEntries().map((entry) => {
    const versionState = conversationAdapterVersionState(
      entry.installed_adapter?.version,
      entry.item.version,
    );
    return {
      ...versionState,
      item: entry.item,
      installed: entry.installed,
      runtime_ready: Boolean(entry.installed_adapter?.enabled),
      status: entry.installed
        ? versionState.ahead_of_release
          ? "ahead_of_release"
          : versionState.update_available
            ? "update_available"
            : "legacy_installed"
        : "not_installed",
      installed_package: null,
      installed_adapter: entry.installed_adapter ?? null,
      install_path: entry.install_path ?? null,
      error_message: null,
    };
  });
}

function conversationAdapterVersionState(installedVersion: string | undefined, catalogVersion: string) {
  if (!installedVersion) {
    return { update_available: false, ahead_of_release: false };
  }
  const order = compareSemanticVersions(installedVersion, catalogVersion);
  if (order === null) {
    return {
      update_available: installedVersion !== catalogVersion,
      ahead_of_release: false,
    };
  }
  return {
    update_available: order < 0,
    ahead_of_release: order > 0,
  };
}

interface ParsedSemanticVersion {
  core: [string, string, string];
  prerelease: string[] | null;
}

function compareSemanticVersions(left: string, right: string): number | null {
  const leftVersion = parseSemanticVersion(left);
  const rightVersion = parseSemanticVersion(right);
  if (!leftVersion || !rightVersion) {
    return null;
  }
  for (let index = 0; index < 3; index += 1) {
    const comparison = compareNumericIdentifiers(leftVersion.core[index], rightVersion.core[index]);
    if (comparison !== 0) {
      return comparison;
    }
  }
  if (!leftVersion.prerelease && !rightVersion.prerelease) {
    return 0;
  }
  if (!leftVersion.prerelease) {
    return 1;
  }
  if (!rightVersion.prerelease) {
    return -1;
  }
  const length = Math.max(leftVersion.prerelease.length, rightVersion.prerelease.length);
  for (let index = 0; index < length; index += 1) {
    const leftIdentifier = leftVersion.prerelease[index];
    const rightIdentifier = rightVersion.prerelease[index];
    if (leftIdentifier === undefined) {
      return -1;
    }
    if (rightIdentifier === undefined) {
      return 1;
    }
    const leftNumeric = /^\d+$/.test(leftIdentifier);
    const rightNumeric = /^\d+$/.test(rightIdentifier);
    if (leftNumeric && rightNumeric) {
      const comparison = compareNumericIdentifiers(leftIdentifier, rightIdentifier);
      if (comparison !== 0) {
        return comparison;
      }
    } else if (leftNumeric !== rightNumeric) {
      return leftNumeric ? -1 : 1;
    } else if (leftIdentifier !== rightIdentifier) {
      return leftIdentifier < rightIdentifier ? -1 : 1;
    }
  }
  return 0;
}

function parseSemanticVersion(value: string): ParsedSemanticVersion | null {
  const match = value.trim().match(
    /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/,
  );
  if (!match) {
    return null;
  }
  const prerelease = match[4]?.split(".") ?? null;
  if (
    prerelease?.some(
      (identifier) => /^\d+$/.test(identifier) && identifier.length > 1 && identifier.startsWith("0"),
    )
  ) {
    return null;
  }
  return { core: [match[1], match[2], match[3]], prerelease };
}

function compareNumericIdentifiers(left: string, right: string) {
  if (left.length !== right.length) {
    return left.length < right.length ? -1 : 1;
  }
  if (left === right) {
    return 0;
  }
  return left < right ? -1 : 1;
}

function fallbackPackageTask(
  packageId: string,
  catalogUrl?: string | null,
  dryRun?: boolean,
): ConversationScriptInstallTaskSnapshot {
  return {
    id: `preview-package-install-${Date.now()}`,
    status: "completed",
    item_id: packageId,
    package_id: packageId,
    catalog_url: catalogUrl ?? null,
    dry_run: Boolean(dryRun),
    phase: "completed",
    started_at: new Date().toISOString(),
    finished_at: new Date().toISOString(),
    result: { installed: true },
    error: null,
  };
}

const fallbackSources: ConversationSource[] = [
  {
    id: "codex-live",
    adapter_id: "codex",
    name: "Codex local sessions",
    kind: "live",
    location: "~/.codex",
    enabled: true,
    created_at: now,
    updated_at: now,
  },
];

const fallbackSessions: ConversationSessionListItem[] = [
  {
    id: "preview-session",
    source_id: "codex-live",
    adapter_id: "codex",
    external_id: "preview",
    title: "Preview conversation session",
    project_path: "/preview/project",
    missing: false,
    created_at: now,
    imported_at: now,
    question_count: 2,
    turn_count: 3,
  },
];

const fallbackSessionDetail: ConversationSessionDetail = {
  session: fallbackSessions[0],
  questions: [
    {
      question: {
        id: "preview-question-1",
        session_id: "preview-session",
        question_index: 0,
        title: "How does conversation sync work?",
        question_text: "How does conversation sync work?\n\n继续",
        answer_text: "AssetIWeave imports source sessions into normalized turns, then groups adjacent turns into question records.",
        code_text: "",
        command_text: "assetiweave-cli conversation sync --source codex-live",
        grouping_origin: "auto_merged",
        created_at: now,
        updated_at: now,
      },
      turns: [
        {
          id: "preview-turn-1",
          session_id: "preview-session",
          external_id: "turn-1",
          turn_index: 0,
          user_text: "How does conversation sync work?",
          fingerprint: "preview",
          missing: false,
          imported_at: now,
        },
        {
          id: "preview-turn-2",
          session_id: "preview-session",
          external_id: "turn-2",
          turn_index: 1,
          user_text: "继续",
          fingerprint: "preview",
          missing: false,
          imported_at: now,
        },
      ],
      parts: [
        {
          id: "preview-part-1",
          turn_id: "preview-turn-1",
          part_index: 0,
          role: "assistant",
          kind: "text",
          text: "AssetIWeave imports source sessions into normalized turns, then groups adjacent turns into question records.",
          metadata_json: JSON.stringify({
            content_card: { type: "answer", format: "markdown" },
          }),
        },
        {
          id: "preview-part-2",
          turn_id: "preview-turn-2",
          part_index: 0,
          role: "tool",
          kind: "command",
          command: "assetiweave-cli conversation sync --source codex-live",
          metadata_json: JSON.stringify({
            content_card: { type: "command" },
          }),
        },
      ],
    },
    {
      question: {
        id: "preview-question-2",
        session_id: "preview-session",
        question_index: 1,
        title: "Export this session",
        question_text: "Export this session",
        answer_text: "Use session export to write one Markdown file per session.",
        code_text: "",
        command_text: "",
        grouping_origin: "imported",
        created_at: now,
        updated_at: now,
      },
      turns: [
        {
          id: "preview-turn-3",
          session_id: "preview-session",
          external_id: "turn-3",
          turn_index: 2,
          user_text: "Export this session",
          fingerprint: "preview",
          missing: false,
          imported_at: now,
        },
      ],
      parts: [
        {
          id: "preview-part-3",
          turn_id: "preview-turn-3",
          part_index: 0,
          role: "assistant",
          kind: "text",
          text: "Use session export to write one Markdown file per session.",
          metadata_json: JSON.stringify({
            content_card: { type: "answer", format: "markdown" },
          }),
        },
      ],
    },
  ],
};

const fallbackWebSessions: ConversationSessionListItem[] = [
  {
    ...fallbackSessions[0],
    id: "preview-web-record",
    source_id: "qwen-web-export",
    adapter_id: "qwen-web",
    external_id: "qwen-preview",
    title: "Qwen web conversation",
    project_path: null,
  },
];

const fallbackWebSessionDetail: ConversationSessionDetail = {
  session: fallbackWebSessions[0],
  questions: fallbackSessionDetail.questions.map((detail, questionIndex) => ({
    ...detail,
    question: {
      ...detail.question,
      id: `preview-web-question-${questionIndex + 1}`,
      session_id: fallbackWebSessions[0].id,
    },
    turns: detail.turns.map((turn, turnIndex) => ({
      ...turn,
      id: `preview-web-turn-${questionIndex + 1}-${turnIndex + 1}`,
      session_id: fallbackWebSessions[0].id,
    })),
    parts: detail.parts.map((part, partIndex) => ({
      ...part,
      id: `preview-web-part-${questionIndex + 1}-${partIndex + 1}`,
      turn_id: `preview-web-turn-${questionIndex + 1}-${Math.min(partIndex + 1, detail.turns.length)}`,
    })),
  })),
};
