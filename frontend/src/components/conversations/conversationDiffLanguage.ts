export function isDiffLanguage(language?: string | null) {
  return ["diff", "patch", "udiff", "unified-diff"].includes(language?.trim().toLowerCase() ?? "");
}

export function isUnifiedDiffText(value?: string | null) {
  if (!value) return false;
  const lines = value.replace(/\r\n?/g, "\n").split("\n");
  const nonEmpty = lines.filter((line) => line.trim().length > 0);
  if (nonEmpty.length < 4) return false;
  const trimmed = lines.map((line) => line.trimStart());
  const hasGitHeader = trimmed.some((line) => line.startsWith("diff --git "));
  const hasFileMarkers =
    trimmed.some((line) => line.startsWith("--- ")) &&
    trimmed.some((line) => line.startsWith("+++ "));
  const hasHunk = trimmed.some((line) => /^@@ -\d+(?:,\d+)? \+\d+(?:,\d+)? @@/.test(line));
  if (!((hasGitHeader || hasFileMarkers) && hasHunk)) return false;
  const markedLines = trimmed.filter((line) => /^[+\- ]/.test(line)).length;
  return markedLines / lines.length >= 0.5;
}
