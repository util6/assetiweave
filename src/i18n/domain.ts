import type { AssetKind, DeploymentActionType, DeploymentStrategy } from "../types";
import type { Translator } from "./I18nProvider";
import type { TranslationKey } from "./messages";

export function assetKindLabel(kind: AssetKind, t: Translator) {
  return t(`assetKind.${kind}` as TranslationKey);
}

export function deploymentActionLabel(actionType: DeploymentActionType, t: Translator) {
  return t(`deploymentAction.${actionType}` as TranslationKey);
}

export function deploymentStrategyLabel(strategy: DeploymentStrategy, t: Translator) {
  return t(`deploymentStrategy.${strategy}` as TranslationKey);
}

export function translateScanStatus(status: string | null | undefined, t: Translator) {
  if (!status) {
    return t("status.loading");
  }

  const normalized = status.trim();
  if (normalized === "pending") {
    return t("status.pending");
  }
  if (normalized === "等待首次扫描") {
    return t("status.waitingFirstScan");
  }
  if (normalized === "preview" || normalized === "浏览器预览模式：使用内置示例数据") {
    return t("status.previewData");
  }

  const okMatch = normalized.match(/^ok: (\d+) assets$/);
  if (okMatch) {
    return t("status.scanOk", { count: okMatch[1] });
  }

  const errorMatch = normalized.match(/^error: (.+)$/);
  if (errorMatch) {
    return t("status.scanError", { message: errorMatch[1] });
  }

  return normalized;
}

export function translatePlanReason(reason: string, t: Translator) {
  if (reason === "目标路径已存在，MVP 默认不覆盖非本应用管理的文件") {
    return t("plan.reason.conflictExisting");
  }

  const unsupportedMatch = reason.match(/^(.+) 不支持 ([A-Za-z]+) 或未命中 include 规则$/);
  if (unsupportedMatch) {
    return t("plan.reason.unsupported", {
      profile: unsupportedMatch[1],
      kind: formatDebugAssetKind(unsupportedMatch[2], t),
    });
  }

  const projectMatch = reason.match(/^(.+) 支持 ([A-Za-z]+)，将以 ([A-Za-z]+) 投影到目标目录$/);
  if (projectMatch) {
    return t("plan.reason.project", {
      profile: projectMatch[1],
      kind: formatDebugAssetKind(projectMatch[2], t),
      strategy: formatDebugDeploymentStrategy(projectMatch[3], t),
    });
  }

  const missingAssetMatch = reason.match(/^asset not found: (.+)$/);
  if (missingAssetMatch) {
    return t("error.assetNotFound", { assetId: missingAssetMatch[1] });
  }

  return reason;
}

function formatDebugAssetKind(value: string, t: Translator) {
  const normalized = normalizeRustDebugEnum(value);
  if (isAssetKind(normalized)) {
    return assetKindLabel(normalized, t);
  }
  return value;
}

function formatDebugDeploymentStrategy(value: string, t: Translator) {
  const normalized = normalizeRustDebugEnum(value);
  if (isDeploymentStrategy(normalized)) {
    return deploymentStrategyLabel(normalized, t);
  }
  return value;
}

function normalizeRustDebugEnum(value: string) {
  return value.replace(/([a-z0-9])([A-Z])/g, "$1_$2").toLowerCase();
}

function isAssetKind(value: string): value is AssetKind {
  return [
    "prompt",
    "rule",
    "memory",
    "skill",
    "mcp",
    "agent",
    "command",
    "workflow",
    "profile",
    "custom",
    "unclassified",
  ].includes(value);
}

function isDeploymentStrategy(value: string): value is DeploymentStrategy {
  return ["symlink", "copy", "render", "append", "config_merge"].includes(value);
}
