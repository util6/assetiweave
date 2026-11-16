import type { Translator } from "../../i18n/I18nProvider";
import { abbreviateHomePath } from "../../utils/path";

export interface CompactActionBlockInput {
  kind?: string;
  renderer?: string;
  signal?: string | null;
  text: string;
  commandLabel?: string | null;
  filePath?: string | null;
  toolName?: string | null;
}

export interface ParsedCompactAction {
  action: string;
  target: string;
  rawTarget?: string;
  lineRange?: string;
  icon: "search" | "read" | "tool";
  hasDetails: boolean;
  details?: string;
}

interface ToolArgEntry {
  key: string;
  lowerKey: string;
  value: string;
}

const COMPACT_READ_TOOLS = new Set([
  "view_file",
  "read_file",
  "read",
  "view",
  "view file",
  "view_file_outline",
  "view_code_item",
  "read_url_content",
  "read_resource",
  "list_dir",
  "list_directory",
  "read_dir",
  "fetch_web_page",
]);

const COMPACT_SEARCH_TOOLS = new Set([
  "grep_search",
  "find_by_name",
  "find_files",
  "search_web",
  "search_docs",
  "file_search",
  "glob",
  "grep",
  "locate_files",
]);

const READ_ONLY_ARG_KEYS = new Set([
  "path",
  "absolutepath",
  "targetfile",
  "file_path",
  "filepath",
  "startline",
  "endline",
  "start_line",
  "end_line",
  "line_start",
  "line_end",
  "byteoffset",
  "byte_offset",
  "offset",
  "limit",
  "line_limit",
  "linelimit",
  "codeitemname",
  "nodepaths",
]);

const PATH_ARG_KEYS = [
  "absolutepath",
  "path",
  "targetfile",
  "file_path",
  "filepath",
  "url",
  "directorypath",
] as const;

const SEARCH_TARGET_ARG_KEYS = [
  "query",
  "pattern",
  "searchpath",
  "searchdirectory",
  "path",
] as const;

const LINE_ARG_KEYS = new Set([
  "startline",
  "start_line",
  "line_start",
  "offset",
  "endline",
  "end_line",
  "line_end",
  "limit",
  "line_limit",
  "linelimit",
]);

const BLOCKED_FALLBACK_HEADER_VERBS =
  /(?:^|[^a-z0-9])(?:delete|remove|rm|write|edit|create|patch|replace|run|exec|kill|move|mv|copy|cp|save|format|apply|update|touch|mkdir|chmod|chown)(?:[^a-z0-9]|$)/i;

/** Returns true when a block represents a low-noise read/search action suitable for compact rendering. */
export function isAmbientActionBlock(block: CompactActionBlockInput): boolean {
  if (block.renderer === "compact_action") return true;
  if (block.signal === "ambient") return true;

  const kind = (block.kind || "").toLowerCase();
  if (
    kind.includes("read") ||
    kind.includes("search") ||
    kind.includes("ambient")
  ) {
    return true;
  }

  const toolName = (block.toolName || "").trim().toLowerCase();
  if (
    toolName &&
    (COMPACT_READ_TOOLS.has(toolName) || COMPACT_SEARCH_TOOLS.has(toolName))
  ) {
    return true;
  }

  const trimmed = (block.text || "").trim();
  if (
    /^(?:view_file|read_file|grep_search|list_dir|list_directory|find_by_name|read_url_content):/i.test(
      trimmed,
    )
  ) {
    return true;
  }

  const toolHeader = splitToolBlock(trimmed);
  if (!toolHeader) return false;

  const headerLower = toolHeader.header.toLowerCase();
  if (
    COMPACT_READ_TOOLS.has(headerLower) ||
    COMPACT_SEARCH_TOOLS.has(headerLower)
  ) {
    return true;
  }
  if (BLOCKED_FALLBACK_HEADER_VERBS.test(toolHeader.header)) {
    return false;
  }
  if (/^(?:read|view)(?:\s|$)/i.test(toolHeader.header)) {
    return true;
  }

  const parsedBody = parseToolPayload(toolHeader.body);
  if (parsedBody.extraLines.length > 0 || parsedBody.entries.length === 0) {
    return false;
  }
  const hasPathKey = parsedBody.entries.some(
    (entry) =>
      entry.lowerKey === "path" ||
      entry.lowerKey === "absolutepath" ||
      entry.lowerKey === "targetfile" ||
      entry.lowerKey === "file_path" ||
      entry.lowerKey === "filepath",
  );
  return (
    hasPathKey &&
    parsedBody.entries.every((entry) => READ_ONLY_ARG_KEYS.has(entry.lowerKey))
  );
}

const AMBIENT_LINE_PATTERN =
  /^(Read|Outline|Symbol|List|Fetch|Search|Find)\s+(.+)$/i;

/** Parses a compact action card into normalized action items (1 item for single actions, N items for batched ambient cards). */
export function parseCompactActionItems(
  text: string,
  block: CompactActionBlockInput,
  t: Translator,
): ParsedCompactAction[] {
  const trimmed = text.trim();
  const lines = trimmed
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);

  if (
    lines.length > 1 &&
    !splitToolBlock(trimmed) &&
    lines.every((line) => AMBIENT_LINE_PATTERN.test(line))
  ) {
    return lines
      .map((line) => parseAmbientSummaryLine(line, t))
      .filter((item): item is ParsedCompactAction => item !== null);
  }

  return [parseCompactActionText(text, block, t)];
}

/** Parses a single ambient summary line like `Read /path/File.tsx#L65-275` or `Search "Dialog" @ /src`. */
function parseAmbientSummaryLine(
  line: string,
  t: Translator,
): ParsedCompactAction | null {
  const match = line.trim().match(AMBIENT_LINE_PATTERN);
  if (!match) return null;

  const rawVerb = match[1];
  const verb = rawVerb.charAt(0).toUpperCase() + rawVerb.slice(1).toLowerCase();
  const rest = match[2].trim();
  const isSearch = verb === "Search" || verb === "Find";

  if (verb === "Search" || verb === "Find" || verb === "Symbol") {
    const scopeSepIdx = rest.lastIndexOf(" @ ");
    if (scopeSepIdx > 0) {
      const subject = rest.slice(0, scopeSepIdx).trim();
      const scope = rest.slice(scopeSepIdx + 3).trim();
      const abbrScope = abbreviateDisplayTarget(scope);
      return {
        action: verb,
        target: `${subject} @ ${abbrScope}`,
        rawTarget: rest,
        icon: isSearch ? "search" : "read",
        hasDetails: false,
      };
    }
    return {
      action: verb,
      target: verb === "Find" ? abbreviateDisplayTarget(rest) : rest,
      rawTarget: rest,
      icon: isSearch ? "search" : "read",
      hasDetails: false,
    };
  }

  const hashRangeMatch = rest.match(/^(.*?)#L(\d+)(?:-(\d+))?$/);
  if (hashRangeMatch && hashRangeMatch[1]) {
    const rawPath = hashRangeMatch[1].trim();
    const startLine = Number.parseInt(hashRangeMatch[2], 10);
    const endLine = hashRangeMatch[3]
      ? Number.parseInt(hashRangeMatch[3], 10)
      : undefined;
    return {
      action: verb,
      target: abbreviateDisplayTarget(rawPath),
      rawTarget: rawPath,
      lineRange: formatLineRange(startLine, endLine, t),
      icon: "read",
      hasDetails: false,
    };
  }

  const { cleanTarget, lineRange } = extractTargetAndLineRange(
    rest,
    undefined,
    undefined,
    false,
    t,
  );
  return {
    action: verb,
    target: abbreviateDisplayTarget(cleanTarget),
    rawTarget: cleanTarget,
    lineRange,
    icon: "read",
    hasDetails: false,
  };
}

/** Parses a compact action card into normalized action badge, abbreviated target, and optional line range. */
export function parseCompactActionText(
  text: string,
  block: CompactActionBlockInput,
  t: Translator,
): ParsedCompactAction {
  const trimmed = text.trim();

  if (!trimmed.includes("\n") && !splitToolBlock(trimmed)) {
    const singleAmbient = parseAmbientSummaryLine(trimmed, t);
    if (singleAmbient) {
      return singleAmbient;
    }
  }

  const colonMatch = trimmed.match(/^([a-zA-Z0-9_-]+):\s*(.+)$/s);
  if (colonMatch && !/^tool$/i.test(colonMatch[1])) {
    const act = colonMatch[1];
    const rest = colonMatch[2].trim();
    const newlineIdx = rest.indexOf("\n");
    const firstSegment =
      newlineIdx >= 0 ? rest.slice(0, newlineIdx).trim() : rest;
    const details =
      newlineIdx >= 0 ? rest.slice(newlineIdx + 1).trim() : undefined;
    const isSearch = /grep|search|find|glob/i.test(act);
    const isRead = /view|read|cat|head|tail|list/i.test(act);
    const { cleanTarget, lineRange } = extractTargetAndLineRange(
      firstSegment,
      undefined,
      undefined,
      isSearch,
      t,
    );
    return {
      action: act,
      target: abbreviateDisplayTarget(cleanTarget),
      rawTarget: cleanTarget,
      lineRange,
      icon: isSearch ? "search" : isRead ? "read" : "tool",
      hasDetails: Boolean(details),
      details: details || undefined,
    };
  }

  const toolBlock = splitToolBlock(trimmed);
  if (toolBlock) {
    return parseToolActionBlock(toolBlock.header, toolBlock.body, block, t);
  }

  const isSearch =
    /grep|search|find|rg|glob/i.test(trimmed) ||
    /search/i.test(block.kind || "");
  const newlineIdx = trimmed.indexOf("\n");
  const rawFirst =
    block.filePath ||
    (newlineIdx >= 0 ? trimmed.slice(0, newlineIdx) : trimmed).trim();
  const details =
    newlineIdx >= 0 ? trimmed.slice(newlineIdx + 1).trim() : undefined;
  const { cleanTarget, lineRange } = extractTargetAndLineRange(
    rawFirst,
    undefined,
    undefined,
    isSearch,
    t,
  );
  return {
    action: block.commandLabel || (isSearch ? "Search" : "Read"),
    target: abbreviateDisplayTarget(cleanTarget),
    rawTarget: cleanTarget,
    lineRange,
    icon: isSearch ? "search" : "read",
    hasDetails: Boolean(details),
    details: details || undefined,
  };
}

function parseToolActionBlock(
  header: string,
  body: string,
  block: CompactActionBlockInput,
  t: Translator,
): ParsedCompactAction {
  const { entries, extraLines } = parseToolPayload(body);
  const toolId = (block.toolName || header).trim().toLowerCase();
  const isSearch =
    COMPACT_SEARCH_TOOLS.has(toolId) || /grep|search|find|glob/i.test(header);
  const isRead =
    !isSearch &&
    (COMPACT_READ_TOOLS.has(toolId) ||
      /view|read|cat|head|tail|list/i.test(header) ||
      entries.some((entry) =>
        [
          "path",
          "absolutepath",
          "targetfile",
          "file_path",
          "filepath",
        ].includes(entry.lowerKey),
      ));

  const targetKeys = isSearch ? SEARCH_TARGET_ARG_KEYS : PATH_ARG_KEYS;
  let matchedTargetKey: string | undefined;
  let rawTarget = block.filePath?.trim() || "";
  for (const key of targetKeys) {
    const entry = entries.find((item) => item.lowerKey === key && item.value);
    if (entry) {
      matchedTargetKey = entry.lowerKey;
      rawTarget = entry.value.trim();
      break;
    }
  }
  if (!rawTarget) {
    rawTarget = header;
  }

  const rangeFromEntries = extractLineRangeFromEntries(entries, header);
  const { cleanTarget, lineRange } = extractTargetAndLineRange(
    rawTarget,
    rangeFromEntries.startLine,
    rangeFromEntries.endLine,
    isSearch,
    t,
  );

  const remainingDetails: string[] = [];
  for (const entry of entries) {
    if (entry.lowerKey === matchedTargetKey) continue;
    if (LINE_ARG_KEYS.has(entry.lowerKey)) continue;
    if (
      (entry.lowerKey === "byteoffset" || entry.lowerKey === "byte_offset") &&
      entry.value.trim() === "0"
    ) {
      continue;
    }
    remainingDetails.push(`${entry.key}: ${entry.value}`);
  }
  for (const extra of extraLines) {
    remainingDetails.push(extra);
  }
  const detailsText = remainingDetails.join("\n").trim();

  return {
    action: normalizeToolActionLabel(
      toolId,
      header,
      block.toolName,
      isRead,
      isSearch,
    ),
    target: abbreviateDisplayTarget(cleanTarget),
    rawTarget: cleanTarget,
    lineRange,
    icon: isSearch ? "search" : isRead ? "read" : "tool",
    hasDetails: detailsText.length > 0,
    details: detailsText.length > 0 ? detailsText : undefined,
  };
}

function splitToolBlock(
  trimmed: string,
): { header: string; body: string } | null {
  const match = trimmed.match(/^Tool:\s*([^\n]+)(?:\n([\s\S]*))?$/i);
  if (!match) return null;
  return {
    header: match[1].trim(),
    body: (match[2] ?? "").trim(),
  };
}

function parseToolPayload(body: string): {
  entries: ToolArgEntry[];
  extraLines: string[];
} {
  if (!body) return { entries: [], extraLines: [] };

  if (body.startsWith("{")) {
    try {
      const parsed = JSON.parse(body);
      if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
        const entries: ToolArgEntry[] = [];
        for (const [key, val] of Object.entries(
          parsed as Record<string, unknown>,
        )) {
          if (val === null || val === undefined || val === "") continue;
          entries.push({
            key,
            lowerKey: key.toLowerCase(),
            value: typeof val === "string" ? val : JSON.stringify(val),
          });
        }
        return { entries, extraLines: [] };
      }
    } catch {
      // Fall back to line-based parsing
    }
  }

  const entries: ToolArgEntry[] = [];
  const extraLines: string[] = [];
  for (const line of body.split("\n")) {
    const kvMatch = line.match(/^([A-Za-z0-9_]+):\s*(.*)$/);
    if (kvMatch) {
      entries.push({
        key: kvMatch[1],
        lowerKey: kvMatch[1].toLowerCase(),
        value: kvMatch[2].trim(),
      });
    } else if (line.trim()) {
      if (entries.length > 0) {
        entries[entries.length - 1].value =
          `${entries[entries.length - 1].value}\n${line}`.trim();
      } else {
        extraLines.push(line.trim());
      }
    }
  }
  return { entries, extraLines };
}

function extractLineRangeFromEntries(
  entries: readonly ToolArgEntry[],
  header: string,
): { startLine?: number; endLine?: number } {
  const startLine = findPositiveInt(entries, [
    "startline",
    "start_line",
    "line_start",
    "offset",
  ]);
  const explicitEnd = findPositiveInt(entries, [
    "endline",
    "end_line",
    "line_end",
  ]);
  const limit = findPositiveInt(entries, ["limit", "line_limit", "linelimit"]);
  const endLine =
    explicitEnd ??
    (startLine !== undefined && limit !== undefined
      ? startLine + limit - 1
      : undefined);

  if (startLine !== undefined || endLine !== undefined) {
    return { startLine, endLine };
  }

  const headerRangeMatch = header.match(/\blines?\s+(\d+)\s*[-–]\s*(\d+)\b/i);
  if (headerRangeMatch) {
    return {
      startLine: Number.parseInt(headerRangeMatch[1], 10),
      endLine: Number.parseInt(headerRangeMatch[2], 10),
    };
  }
  return {};
}

function findPositiveInt(
  entries: readonly ToolArgEntry[],
  keys: readonly string[],
): number | undefined {
  for (const key of keys) {
    const entry = entries.find((item) => item.lowerKey === key);
    if (!entry) continue;
    const num = Number.parseInt(entry.value.trim(), 10);
    if (Number.isFinite(num) && num > 0) return num;
  }
  return undefined;
}

function extractTargetAndLineRange(
  rawTarget: string,
  explicitStart: number | undefined,
  explicitEnd: number | undefined,
  isSearch: boolean,
  t: Translator,
): { cleanTarget: string; lineRange?: string } {
  let cleanTarget = rawTarget;
  let startLine = explicitStart;
  let endLine = explicitEnd;

  // Strip trailing `:start` or `:start-end` produced by payload-policy formatLocation,
  // avoiding URLs, search queries (e.g. `port:8080`), and Windows drive prefixes.
  if (!isSearch && !/^https?:\/\//i.test(rawTarget)) {
    const suffixMatch = rawTarget.match(/^(.*):(\d+)(?:-(\d+))?$/);
    if (suffixMatch && suffixMatch[1]) {
      cleanTarget = suffixMatch[1];
      if (startLine === undefined && endLine === undefined) {
        startLine = Number.parseInt(suffixMatch[2], 10);
        if (suffixMatch[3]) {
          endLine = Number.parseInt(suffixMatch[3], 10);
        }
      }
    }
  }

  return {
    cleanTarget,
    lineRange: formatLineRange(startLine, endLine, t),
  };
}

function formatLineRange(
  startLine: number | undefined,
  endLine: number | undefined,
  t: Translator,
): string | undefined {
  if (startLine !== undefined && endLine !== undefined) {
    return t("conversation.content.lineRange", {
      start: startLine,
      end: endLine,
    });
  }
  if (startLine !== undefined) {
    return t("conversation.content.lineFrom", { start: startLine });
  }
  if (endLine !== undefined) {
    return t("conversation.content.lineUntil", { end: endLine });
  }
  return undefined;
}

function normalizeToolActionLabel(
  toolId: string,
  header: string,
  explicitToolName: string | null | undefined,
  isRead: boolean,
  isSearch: boolean,
): string {
  if (toolId === "view_file_outline") return "Outline";
  if (toolId === "view_code_item") return "Symbol";
  if (
    toolId === "list_dir" ||
    toolId === "list_directory" ||
    toolId === "read_dir"
  ) {
    return "List";
  }
  if (
    toolId === "find_by_name" ||
    toolId === "find_files" ||
    toolId === "file_search" ||
    toolId === "glob" ||
    toolId === "locate_files"
  ) {
    return "Find";
  }
  if (isSearch) return "Search";
  if (isRead) return "Read";
  return explicitToolName || header;
}

function abbreviateDisplayTarget(target: string): string {
  if (!target || /^https?:\/\//i.test(target)) return target;
  return abbreviateHomePath(target);
}
