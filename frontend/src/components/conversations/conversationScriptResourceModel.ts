// 对话适配器市场面板的纯展示/规则模型。
//
// 这里只做“条目 -> 按钮/徽章/文案键”的判定，不 import React、不触碰组件 state，
// 便于在 ConversationScriptResourcePanel 与 ConversationScriptResourceRow 之间复用，
// 也便于用最小单测锁定逐状态 / 逐 origin 的规则。

import type {
  ConversationAdapterPackageCatalogEntry,
  ConversationAdapterPackageCatalogStatus,
  ConversationScriptInstallTaskSnapshot,
} from "../../services/conversations";

export type PackageAction = "install" | "update" | "repair";
export type PackageChangeAction = PackageAction | "unregister" | "uninstall";

export type StatusBadgeTone =
  "neutral" | "primary" | "create" | "update" | "remove" | "conflict";

/** 需要“修复”的状态：运行时缺失、校验失败、hash 漂移、manifest 无效、core 不兼容。 */
export const REPAIR_STATUSES = [
  "runtime_missing",
  "verification_failed",
  "hash_mismatch",
  "manifest_invalid",
  "core_incompatible",
] as const;

export function isRepairStatus(
  status: ConversationAdapterPackageCatalogStatus,
): boolean {
  return (REPAIR_STATUSES as readonly string[]).includes(status);
}

/**
 * 主操作按钮：安装 / 更新 / 修复。
 *
 * 关键点是**不再按 origin 提前返回**：受管目录（local_directory）与内置（built_in）
 * 条目只要后端给出了 update_available 或修复态，就同样可更新/可修复，
 * 由后端 200 号票的本地晋升闭环承接真正执行。
 */
export function packageActionForEntry(
  entry: ConversationAdapterPackageCatalogEntry,
): PackageAction | null {
  if (entry.status === "uninstalled") {
    return "install";
  }
  if (isRepairStatus(entry.status)) {
    return "repair";
  }
  if (entry.update_available || entry.status === "update_available") {
    return "update";
  }
  if (!entry.installed || entry.status === "not_installed") {
    return "install";
  }
  return null;
}

/** “可更新”标签页口径：可更新 ∪ 全部修复态，与 packageActionForEntry 的判断保持一致。 */
export function isUpdateOrRepairEntry(
  entry: ConversationAdapterPackageCatalogEntry,
): boolean {
  const action = packageActionForEntry(entry);
  return action === "update" || action === "repair";
}

export function countUpdateOrRepairEntries(
  entries: readonly ConversationAdapterPackageCatalogEntry[],
): number {
  return entries.filter(isUpdateOrRepairEntry).length;
}

export function isBuiltInEntry(entry: ConversationAdapterPackageCatalogEntry) {
  return (
    entry.status === "built_in" ||
    entry.installed_package?.origin === "built_in" ||
    entry.installed_adapter?.trust_state === "built_in" ||
    entry.item.provider === "built_in"
  );
}

/** 生命周期按钮：受管包走 uninstall（停用 runtime），其余已接入来源走 unregister。 */
export function packageLifecycleAction(
  entry: ConversationAdapterPackageCatalogEntry,
): "unregister" | "uninstall" | null {
  if (!entry.installed) {
    return null;
  }
  if (entry.status === "uninstalled") {
    return null;
  }
  const origin = entry.installed_package?.origin;
  if (!entry.runtime_ready) {
    // 199 号票：a broken local package keeps its row instead of being silently removed.
    // The repair path is unregistering it and re-registering a valid source directory.
    // Managed releases are repaired through the install path instead.
    return origin === "managed_release" || !origin ? null : "unregister";
  }
  return origin === "managed_release" ? "uninstall" : "unregister";
}

export function statusLabelKey(
  status: ConversationAdapterPackageCatalogStatus,
) {
  switch (status) {
    case "installed":
    case "legacy_installed":
      return "conversation.scriptMarket.installed";
    case "uninstalled":
      return "conversation.scriptMarket.uninstalled";
    case "built_in":
      return "conversation.scriptMarket.builtIn";
    case "local_registered":
      return "conversation.scriptMarket.localRegistered";
    case "git_registered":
      return "conversation.scriptMarket.gitRegistered";
    case "dev_override":
      return "conversation.scriptMarket.devOverride";
    case "update_available":
      return "conversation.scriptMarket.updateAvailable";
    case "runtime_missing":
      return "conversation.scriptMarket.runtimeMissing";
    case "verification_failed":
      return "conversation.scriptMarket.verificationFailed";
    case "hash_mismatch":
      return "conversation.scriptMarket.hashMismatch";
    case "manifest_invalid":
      return "conversation.scriptMarket.manifestInvalid";
    case "core_incompatible":
      return "conversation.scriptMarket.coreIncompatible";
    case "ahead_of_release":
      return "conversation.scriptMarket.aheadOfRelease";
    case "not_installed":
    default:
      return "conversation.scriptMarket.notInstalled";
  }
}

export function statusBadgeTone(
  status: ConversationAdapterPackageCatalogStatus,
): StatusBadgeTone {
  switch (status) {
    case "installed":
    case "legacy_installed":
    case "built_in":
    case "local_registered":
    case "git_registered":
    case "dev_override":
    case "ahead_of_release":
      return "create";
    case "update_available":
    case "runtime_missing":
    case "uninstalled":
      return "conflict";
    case "verification_failed":
    case "hash_mismatch":
    case "manifest_invalid":
    case "core_incompatible":
      return "remove";
    case "not_installed":
    default:
      return "primary";
  }
}

export function actionLabelKey(action: PackageChangeAction) {
  switch (action) {
    case "unregister":
    case "uninstall":
      return "conversation.scriptMarket.uninstall";
    case "repair":
      return "conversation.scriptMarket.repair";
    case "update":
      return "conversation.scriptMarket.update";
    case "install":
    default:
      return "conversation.scriptMarket.registerPackage";
  }
}

export function packageTaskRunningLabel(
  action?: ConversationScriptInstallTaskSnapshot["action"],
) {
  switch (action) {
    case "update":
      return "conversation.scriptMarket.updating";
    case "uninstall":
      return "conversation.scriptMarket.uninstalling";
    case "install":
    default:
      return "conversation.scriptMarket.installing";
  }
}

export function packageTaskCompletedLabel(
  action?: ConversationScriptInstallTaskSnapshot["action"],
) {
  switch (action) {
    case "update":
      return "conversation.scriptMarket.updateCompleted";
    case "uninstall":
      return "conversation.scriptMarket.uninstallCompleted";
    case "install":
    default:
      return "conversation.scriptMarket.installCompleted";
  }
}

export function packageTaskFailedLabel(
  action?: ConversationScriptInstallTaskSnapshot["action"],
) {
  switch (action) {
    case "update":
      return "conversation.scriptMarket.updateFailed";
    case "uninstall":
      return "conversation.scriptMarket.uninstallFailed";
    case "install":
    default:
      return "conversation.scriptMarket.installFailed";
  }
}

export function manifestPathForEntry(
  entry: ConversationAdapterPackageCatalogEntry,
) {
  if (entry.installed_package?.adapter_manifest_path) {
    return entry.installed_package.adapter_manifest_path;
  }
  if (entry.installed_adapter?.manifest_path) {
    return entry.installed_adapter.manifest_path;
  }
  if (!entry.install_path) {
    return null;
  }
  const manifestFile =
    entry.item.manifest_file?.trim() || "conversation-adapter.json";
  return `${entry.install_path.replace(/\/$/, "")}/${manifestFile}`;
}

export function manifestDisplayPathForEntry(
  entry: ConversationAdapterPackageCatalogEntry,
) {
  return entry.display_manifest_path ?? manifestPathForEntry(entry);
}

export function manifestPathFromInstallResult(result: unknown) {
  if (!result || typeof result !== "object" || !("manifest_path" in result)) {
    return null;
  }
  const value = (result as { manifest_path?: unknown }).manifest_path;
  return typeof value === "string" && value.trim() ? value : null;
}

/**
 * 详情弹窗展示的四个信任/版本字段。缺值统一交给调用方用占位文案兜底，
 * 避免把 `undefined` 渲染成空白。
 */
export function packageTrustSummary(
  entry: ConversationAdapterPackageCatalogEntry,
) {
  const installedPackage = entry.installed_package ?? null;
  const installedAdapter = entry.installed_adapter ?? null;
  return {
    trustedVersion: firstNonEmptyValue(
      installedPackage?.version,
      installedAdapter?.version,
    ),
    managedDirectoryVersion: firstNonEmptyValue(
      installedPackage?.latest_version,
      entry.item.version,
    ),
    contentHash: firstNonEmptyValue(
      installedPackage?.installed_content_hash,
      installedAdapter?.content_hash,
    ),
    trustedHash: firstNonEmptyValue(
      installedPackage?.trusted_package_hash,
      installedAdapter?.trusted_hash,
    ),
  };
}

function firstNonEmptyValue(
  ...values: Array<string | null | undefined>
): string | null {
  for (const value of values) {
    if (typeof value === "string" && value.trim()) {
      return value;
    }
  }
  return null;
}

/**
 * 更新/修复时是否显式带上版本号。
 *
 * 只有当详情弹窗为同一条目拉到了真实发布历史、且用户选中的版本就在其中时才透传版本，
 * 本地目录包没有发布历史；一旦误带 version，后端会走远端 release 安装分支，
 * 反而绕过 200 号票的本地晋升闭环。
 */
export function resolvePackageChangeVersion(params: {
  entryId: string;
  detailEntryId: string | null;
  releaseVersions: readonly string[];
  selectedVersion: string;
}): string | undefined {
  const { detailEntryId, entryId, releaseVersions, selectedVersion } = params;
  if (detailEntryId !== entryId || !selectedVersion) {
    return undefined;
  }
  return releaseVersions.includes(selectedVersion)
    ? selectedVersion
    : undefined;
}
