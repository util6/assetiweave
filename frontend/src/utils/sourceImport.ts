import type { SourceInput, SourceOrigin, SourceProbeCandidate } from "../types";

export const DEFAULT_SKILL_INCLUDE_GLOBS = ["**/SKILL.md"];
export const DEFAULT_SKILL_EXCLUDE_GLOBS = [
  "**/.git/**",
  "**/node_modules/**",
  "**/target/**",
  "**/dist/**",
];

export interface SourceImportFormValues {
  enabled: boolean;
  excludeGlobsText: string;
  includeGlobsText: string;
  name: string;
  priority: string;
  rootPath: string;
  sourceOrigin?: SourceOrigin;
}

export function buildImportSourceInput(
  values: SourceImportFormValues,
  overrideOrigin?: SourceOrigin,
): SourceInput {
  const rootPath = values.rootPath.trim();
  const includeGlobs = splitRuleLines(values.includeGlobsText);
  const excludeGlobs = splitRuleLines(values.excludeGlobsText);

  return {
    default_kind: "skill",
    enabled: values.enabled,
    exclude_globs:
      excludeGlobs.length > 0 ? excludeGlobs : DEFAULT_SKILL_EXCLUDE_GLOBS,
    include_globs:
      includeGlobs.length > 0 ? includeGlobs : DEFAULT_SKILL_INCLUDE_GLOBS,
    kind: "import",
    name: values.name.trim() || deriveSourceName(rootPath),
    origin_app_kind: null,
    priority: parsePriority(values.priority),
    repo_root: null,
    root_path: rootPath,
    scan_root: "",
    scanner_kind: "skill",
    source_origin: overrideOrigin ?? values.sourceOrigin ?? "local_folder",
  };
}

export function buildCandidateSourceInputs(
  candidate: SourceProbeCandidate,
  baseValues: SourceImportFormValues,
  index = 0,
): SourceInput {
  const includeGlobs = splitRuleLines(baseValues.includeGlobsText);
  const excludeGlobs = splitRuleLines(baseValues.excludeGlobsText);

  return {
    default_kind: "skill",
    enabled: baseValues.enabled,
    exclude_globs:
      excludeGlobs.length > 0 ? excludeGlobs : DEFAULT_SKILL_EXCLUDE_GLOBS,
    include_globs:
      includeGlobs.length > 0 ? includeGlobs : DEFAULT_SKILL_INCLUDE_GLOBS,
    kind: "import",
    name: candidate.name,
    origin_app_kind: null,
    priority: parsePriority(baseValues.priority) + index,
    repo_root: null,
    root_path: candidate.path,
    scan_root: "",
    scanner_kind: "skill",
    source_origin: candidate.is_git_repo ? "git_repo" : "local_folder",
  };
}

export function deriveSourceName(rootPath: string) {
  const normalizedPath = rootPath.trim().replace(/[\\/]+$/, "");
  const segments = normalizedPath.split(/[\\/]/).filter(Boolean);
  return segments[segments.length - 1] ?? "";
}

function splitRuleLines(value: string) {
  return value
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
}

function parsePriority(value: string) {
  const priority = Number(value);
  return Number.isInteger(priority) ? priority : 0;
}
