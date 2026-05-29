import * as z from "zod";
import type {
  AppKind,
  AssetFormat,
  AssetKind,
  DeploymentActionType,
  DeploymentStrategy,
  PhysicalMountState,
  RiskLevel,
  SourceKind,
  SourceOrigin,
  SourceScannerKind,
} from "../types";

export const assetKindValues = [
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
] as const satisfies readonly AssetKind[];

export const assetFormatValues = [
  "markdown",
  "json",
  "yaml",
  "toml",
  "directory",
  "script",
  "sqlite",
  "unknown",
] as const satisfies readonly AssetFormat[];

export const sourceKindValues = ["local", "git_checkout", "import", "custom"] as const satisfies readonly SourceKind[];

export const sourceScannerKindValues = [
  "skill",
  "mcp",
  "prompt",
  "rule",
  "mixed",
  "custom",
] as const satisfies readonly SourceScannerKind[];

export const sourceOriginValues = [
  "git_repo",
  "local_folder",
  "app_target",
  "app_local",
  "assetiweave_library",
  "custom",
] as const satisfies readonly SourceOrigin[];

export const appKindValues = [
  "codex",
  "claude",
  "cursor",
  "opencode",
  "gemini",
  "antigravity",
  "openclaw",
  "custom",
] as const satisfies readonly AppKind[];

export const deploymentStrategyValues = [
  "symlink_to_source",
  "copy_to_target",
  "render",
  "append",
  "config_merge",
] as const satisfies readonly DeploymentStrategy[];

export const physicalMountStateValues = [
  "mounted",
  "not_mounted",
  "conflict",
  "broken",
] as const satisfies readonly PhysicalMountState[];

export const deploymentActionTypeValues = [
  "create",
  "update",
  "remove",
  "skip",
  "conflict",
] as const satisfies readonly DeploymentActionType[];

export const riskLevelValues = ["low", "medium", "high"] as const satisfies readonly RiskLevel[];

export const assetKindSchema = z.enum(assetKindValues);
export const assetFormatSchema = z.enum(assetFormatValues);
export const sourceKindSchema = z.enum(sourceKindValues);
export const sourceScannerKindSchema = z.enum(sourceScannerKindValues);
export const sourceOriginSchema = z.enum(sourceOriginValues);
export const appKindSchema = z.enum(appKindValues);
export const deploymentStrategySchema = z.enum(deploymentStrategyValues);
export const physicalMountStateSchema = z.enum(physicalMountStateValues);
export const deploymentActionTypeSchema = z.enum(deploymentActionTypeValues);
export const riskLevelSchema = z.enum(riskLevelValues);

type ExactType<Actual, Expected> =
  (<Value>() => Value extends Actual ? 1 : 2) extends <Value>() => Value extends Expected ? 1 : 2
    ? (<Value>() => Value extends Expected ? 1 : 2) extends <Value>() => Value extends Actual ? 1 : 2
      ? true
      : never
    : never;

const _assetKindMatchesType: ExactType<z.infer<typeof assetKindSchema>, AssetKind> = true;
const _assetFormatMatchesType: ExactType<z.infer<typeof assetFormatSchema>, AssetFormat> = true;
const _sourceKindMatchesType: ExactType<z.infer<typeof sourceKindSchema>, SourceKind> = true;
const _sourceScannerKindMatchesType: ExactType<z.infer<typeof sourceScannerKindSchema>, SourceScannerKind> = true;
const _sourceOriginMatchesType: ExactType<z.infer<typeof sourceOriginSchema>, SourceOrigin> = true;
const _appKindMatchesType: ExactType<z.infer<typeof appKindSchema>, AppKind> = true;
const _deploymentStrategyMatchesType: ExactType<z.infer<typeof deploymentStrategySchema>, DeploymentStrategy> = true;
const _physicalMountStateMatchesType: ExactType<z.infer<typeof physicalMountStateSchema>, PhysicalMountState> = true;
const _deploymentActionTypeMatchesType: ExactType<z.infer<typeof deploymentActionTypeSchema>, DeploymentActionType> = true;
const _riskLevelMatchesType: ExactType<z.infer<typeof riskLevelSchema>, RiskLevel> = true;
