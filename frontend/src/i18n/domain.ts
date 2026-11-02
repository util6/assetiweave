import type {
  AssetKind,
  DeploymentStrategy,
  SourceKind,
  SourceOrigin,
} from "../types";
import type { Translator } from "./I18nProvider";
import type { TranslationKey } from "./messages";

export function assetKindLabel(kind: AssetKind, t: Translator) {
  return t(`assetKind.${kind}` as TranslationKey);
}

export function deploymentStrategyLabel(
  strategy: DeploymentStrategy,
  t: Translator,
) {
  return t(`deploymentStrategy.${strategy}` as TranslationKey);
}

export function sourceKindLabel(kind: SourceKind, t: Translator) {
  return t(`source.kind.${kind}` as TranslationKey);
}

export function sourceOriginLabel(origin: SourceOrigin, t: Translator) {
  return t(`source.origin.${origin}` as TranslationKey);
}

export function translateScanStatus(
  status: string | null | undefined,
  t: Translator,
) {
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
  if (
    normalized === "preview" ||
    normalized === "浏览器预览模式：使用内置示例数据"
  ) {
    return t("status.previewData");
  }

  const okMatch = normalized.match(/^ok: (\d+) assets$/);
  if (okMatch) {
    return t("status.scanOk", { count: okMatch[1] });
  }

  const validatedMatch = normalized.match(
    /^validated: (\d+) assets, (\d+) removed, (\d+) updated$/,
  );
  if (validatedMatch) {
    return t("status.validatedAssets", {
      count: validatedMatch[1],
      removed: validatedMatch[2],
      updated: validatedMatch[3],
    });
  }

  const errorMatch = normalized.match(/^error: (.+)$/);
  if (errorMatch) {
    return t("status.scanError", { message: errorMatch[1] });
  }

  return normalized;
}
