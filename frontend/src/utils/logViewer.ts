export type LogLevelFilter = "ALL" | "INFO" | "WARN" | "ERROR";

export const DEFAULT_LOG_LINE_LIMIT = 200;
export const MIN_LOG_LINE_LIMIT = 20;
export const MAX_LOG_LINE_LIMIT = 5000;

const LOG_ENTRY_LEVEL_PATTERN = /^\S+\s+(INFO|WARN|ERROR)\s/;

export function clampLogLineLimit(value: number): number {
  if (!Number.isFinite(value)) {
    return DEFAULT_LOG_LINE_LIMIT;
  }

  return Math.min(MAX_LOG_LINE_LIMIT, Math.max(MIN_LOG_LINE_LIMIT, Math.round(value)));
}

export function filterLogContent(content: string, level: LogLevelFilter): string {
  if (level === "ALL" || !content) {
    return content;
  }

  const lines = content.split("\n");
  const matchedEntries: string[] = [];
  let currentEntry: string[] = [];
  let currentLevel: LogLevelFilter | null = null;

  function flushEntry() {
    if (currentEntry.length > 0 && currentLevel === level) {
      matchedEntries.push(currentEntry.join("\n"));
    }
    currentEntry = [];
    currentLevel = null;
  }

  for (const line of lines) {
    const matchedLevel = line.match(LOG_ENTRY_LEVEL_PATTERN)?.[1] as LogLevelFilter | undefined;
    if (matchedLevel) {
      flushEntry();
      currentEntry = [line];
      currentLevel = matchedLevel;
      continue;
    }

    if (currentEntry.length > 0) {
      currentEntry.push(line);
    }
  }

  flushEntry();
  return matchedEntries.join("\n");
}
