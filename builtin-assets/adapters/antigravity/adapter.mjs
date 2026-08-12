#!/usr/bin/env node
import { normalizeSessionPayload } from "./payload-policy.mjs";
import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { homedir } from "node:os";
import path from "node:path";

const input = JSON.parse(readFileSync(0, "utf8") || "{}");
const CONTENT_CARD_SCHEMA_VERSION = "antigravity-content-cards-v8";
const MAX_PART_TEXT_CHARS = 96 * 1024;
const MAX_SESSION_TEXT_CHARS = 384 * 1024;
const MAX_COMPACTED_TOOL_TEXT_CHARS = 24 * 1024;
const MIN_STANDARD_SESSION_TEXT_CHARS = 96 * 1024;
const BROWSE_OUTPUT_EDGE_LINES = 24;
const BROWSE_OUTPUT_CONTEXT_LINES = 2;
const MAX_BROWSE_OUTPUT_LINE_CHARS = 1200;
const SIGNAL_LINE_PATTERN =
  /\b(error|failed|failure|panic|exception|traceback|warning|warn|denied|not found|cannot|could not|timeout|timed out|exit code|failures?|caused by|compilation|syntaxerror|typeerror|referenceerror|assertionerror)\b|error\[[A-Za-z0-9_-]+\]|\b[A-Za-z0-9_./-]+:\d+:\d+\b/i;

// ---------------------------------------------------------------------------
// I/O helpers
// ---------------------------------------------------------------------------

function emit(type, payload = {}) {
  process.stdout.write(`${JSON.stringify({ type, ...payload })}\n`);
}

function fail(message) {
  emit("error", { message });
  emit("complete", { item: {} });
}

function expandPath(value) {
  if (!value) return value;
  if (value === "~") return homedir();
  if (value.startsWith("~/")) return path.join(homedir(), value.slice(2));
  return value;
}

function sha256(text) {
  return createHash("sha256").update(text).digest("hex");
}

function sourceFingerprint(text) {
  return sha256(`${CONTENT_CARD_SCHEMA_VERSION}\0${text}`);
}

function parseJsonValue(value) {
  if (typeof value !== "string") return null;
  const text = value.trim();
  if (!text.startsWith("{") && !text.startsWith("[")) return null;
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

function metadata(contentCard, extra = {}) {
  return JSON.stringify({
    ...(extra && typeof extra === "object" && !Array.isArray(extra) ? extra : {}),
    content_card: contentCard,
  });
}

function compactObject(value) {
  return Object.fromEntries(
    Object.entries(value).filter(([, entry]) => entry !== null && entry !== undefined && entry !== ""),
  );
}

function smallMetadata(value) {
  if (!value || typeof value !== "object") return {};
  return compactObject({
    source_type: value.type,
    name: value.name,
    tool: value.tool ?? value.tool_name ?? value.toolName,
  });
}

// ---------------------------------------------------------------------------
// Text truncation & budget (ported from codex adapter)
// ---------------------------------------------------------------------------

function truncateText(value, maxChars) {
  const text = String(value ?? "");
  if (text.length <= maxChars) return { text, truncated: false, originalChars: text.length };
  return {
    text: maxChars > 0 ? text.slice(0, maxChars).trimEnd() : "",
    truncated: true,
    originalChars: text.length,
  };
}

function markPartTruncated(part, originalChars, budget) {
  const metadataValue = parseJsonValue(part.metadata_json) ?? {};
  metadataValue.truncated = true;
  metadataValue.original_chars = Math.max(Number(metadataValue.original_chars) || 0, originalChars);
  metadataValue.display_chars = String(part.text ?? "").length;
  metadataValue.display_budget_chars = budget;
  part.metadata_json = JSON.stringify(metadataValue);
}

function markPartCompactedForBrowsing(part, originalChars, budget) {
  const metadataValue = parseJsonValue(part.metadata_json) ?? {};
  metadataValue.truncated = true;
  metadataValue.compacted_for_browsing = true;
  metadataValue.original_chars = Math.max(Number(metadataValue.original_chars) || 0, originalChars);
  metadataValue.display_chars = String(part.text ?? "").length;
  metadataValue.compaction_budget_chars = budget;
  part.metadata_json = JSON.stringify(metadataValue);
}

function contentCardType(part) {
  const metadataValue = parseJsonValue(part.metadata_json);
  const card = metadataValue?.content_card ?? metadataValue?.contentCard;
  const type = card && typeof card === "object" && !Array.isArray(card) ? card.type : null;
  return typeof type === "string" ? type : null;
}

function isHighPriorityBrowsePart(part) {
  const type = contentCardType(part);
  return part.role === "assistant" || type === "answer" || type === "code";
}

function highPriorityTextBudget(session) {
  let highPriorityTotal = 0;
  let standardTotal = 0;
  for (const turn of session.turns) {
    for (const part of turn.parts) {
      if (typeof part.text !== "string" || !part.text) continue;
      if (isHighPriorityBrowsePart(part)) {
        highPriorityTotal += Math.min(MAX_PART_TEXT_CHARS, part.text.length);
      } else {
        standardTotal += Math.min(MAX_PART_TEXT_CHARS, part.text.length);
      }
    }
  }
  const standardFloor = Math.min(standardTotal, MIN_STANDARD_SESSION_TEXT_CHARS, MAX_SESSION_TEXT_CHARS);
  const highPriorityBudget = Math.min(highPriorityTotal, MAX_SESSION_TEXT_CHARS - standardFloor);
  return {
    highPriority: highPriorityBudget,
    standard: Math.min(standardTotal, MAX_SESSION_TEXT_CHARS - highPriorityBudget),
  };
}

function browseLine(value) {
  const text = String(value ?? "");
  if (text.length <= MAX_BROWSE_OUTPUT_LINE_CHARS) return text;
  return `${text.slice(0, MAX_BROWSE_OUTPUT_LINE_CHARS)} [line truncated]`;
}

function mergedRanges(ranges) {
  return ranges
    .filter((range) => range.end > range.start)
    .sort((a, b) => a.start - b.start)
    .reduce((merged, range) => {
      const previous = merged.at(-1);
      if (!previous || range.start > previous.end) {
        merged.push({ ...range });
      } else {
        previous.end = Math.max(previous.end, range.end);
      }
      return merged;
    }, []);
}

function compactToolTextForBrowsing(value, maxChars) {
  const text = String(value ?? "");
  if (text.length <= maxChars) return { text, compacted: false, originalChars: text.length };

  const lines = text.split(/\r?\n/);
  const ranges = [
    { start: 0, end: Math.min(BROWSE_OUTPUT_EDGE_LINES, lines.length) },
    { start: Math.max(0, lines.length - BROWSE_OUTPUT_EDGE_LINES), end: lines.length },
  ];
  lines.forEach((line, index) => {
    if (!SIGNAL_LINE_PATTERN.test(line)) return;
    ranges.push({
      start: Math.max(0, index - BROWSE_OUTPUT_CONTEXT_LINES),
      end: Math.min(lines.length, index + BROWSE_OUTPUT_CONTEXT_LINES + 1),
    });
  });

  const budget = Math.max(0, maxChars);
  const pieces = [];
  let previousEnd = 0;
  for (const range of mergedRanges(ranges)) {
    const prefix = range.start > previousEnd ? `\n... omitted ${range.start - previousEnd} low-signal lines ...\n` : "";
    const block = `${prefix}${lines.slice(range.start, range.end).map(browseLine).join("\n")}`;
    const candidate = `${pieces.join("\n")}${pieces.length ? "\n" : ""}${block}`;
    if (candidate.length > budget) break;
    pieces.push(block);
    previousEnd = range.end;
  }
  const compacted = pieces.join("\n") || text.slice(0, budget).trimEnd();
  return { text: compacted, compacted: true, originalChars: text.length };
}

function compactLowSignalToolOutput(session) {
  for (const turn of session.turns) {
    for (const part of turn.parts) {
      if (typeof part.text !== "string" || !part.text) continue;
      if (part.content_card?.renderer === "diff") continue;
      const type = contentCardType(part);
      if (part.role !== "tool" && type !== "result" && type !== "tool") continue;
      const compacted = compactToolTextForBrowsing(part.text, MAX_COMPACTED_TOOL_TEXT_CHARS);
      if (!compacted.compacted) continue;
      part.text = compacted.text;
      markPartCompactedForBrowsing(part, compacted.originalChars, MAX_COMPACTED_TOOL_TEXT_CHARS);
    }
  }
}

function applyTextBudgets(session) {
  compactLowSignalToolOutput(session);
  const budgets = highPriorityTextBudget(session);
  let highPriorityRemaining = budgets.highPriority;
  let standardRemaining = budgets.standard;
  for (const turn of session.turns) {
    for (const part of turn.parts) {
      if (typeof part.text !== "string" || !part.text) continue;
      if (part.content_card?.renderer === "diff") continue;
      const original = part.text;
      const highPriority = isHighPriorityBrowsePart(part);
      const available = highPriority ? highPriorityRemaining : standardRemaining;
      const maxChars = Math.max(0, Math.min(MAX_PART_TEXT_CHARS, available));
      const truncated = truncateText(original, maxChars);
      part.text = truncated.text;
      if (highPriority) {
        highPriorityRemaining = Math.max(0, highPriorityRemaining - part.text.length);
      } else {
        standardRemaining = Math.max(0, standardRemaining - part.text.length);
      }
      if (truncated.truncated || original.length !== part.text.length) {
        markPartTruncated(part, truncated.originalChars, maxChars);
      }
    }
  }
  return session;
}

// ---------------------------------------------------------------------------
// Part construction helpers
// ---------------------------------------------------------------------------

function textPart(role, text) {
  const trimmed = String(text ?? "").trim();
  if (!trimmed) return null;
  return {
    role,
    kind: "text",
    text: trimmed,
    language: null,
    command: null,
    cwd: null,
    status: null,
    exit_code: null,
    metadata_json: role === "assistant" ? metadata({ type: "answer", format: "markdown" }) : null,
  };
}

function splitMarkdownParts(role, text) {
  const parts = [];
  let remaining = String(text ?? "");
  while (remaining.includes("```")) {
    const start = remaining.indexOf("```");
    const beforePart = textPart(role, remaining.slice(0, start));
    if (beforePart) parts.push(beforePart);

    const fenceBody = remaining.slice(start + 3);
    const end = fenceBody.indexOf("```");
    if (end < 0) {
      const trailing = textPart(role, fenceBody);
      if (trailing) parts.push(trailing);
      return parts;
    }

    const fenced = fenceBody.slice(0, end);
    const firstNewline = fenced.indexOf("\n");
    const language = firstNewline < 0 ? null : fenced.slice(0, firstNewline).trim() || null;
    const code = (firstNewline < 0 ? fenced : fenced.slice(firstNewline + 1)).trimEnd();
    if (code.trim()) {
      parts.push({
        role,
        kind: "code_block",
        text: code,
        language,
        command: null,
        cwd: null,
        status: null,
        exit_code: null,
        metadata_json: metadata({ type: "code", ...(language ? { language } : {}) }),
      });
    }
    remaining = fenceBody.slice(end + 3);
  }
  const tail = textPart(role, remaining);
  if (tail) parts.push(tail);
  return parts;
}

function displayTurns(turns) {
  return turns
    .filter((turn) => Array.isArray(turn.parts) && turn.parts.length > 0)
    .map((turn, index) => ({
      ...turn,
      turn_index: index,
    }));
}

// ---------------------------------------------------------------------------
// Antigravity-specific: extract user text from <USER_REQUEST> tags
// ---------------------------------------------------------------------------

function extractUserRequestText(content) {
  if (!content) return null;
  const openTag = "<USER_REQUEST>";
  const closeTag = "</USER_REQUEST>";
  const startIndex = content.indexOf(openTag);
  if (startIndex < 0) return null;
  const textStart = startIndex + openTag.length;
  const endIndex = content.indexOf(closeTag, textStart);
  const text = endIndex < 0 ? content.slice(textStart) : content.slice(textStart, endIndex);
  return text.trim() || null;
}

// ---------------------------------------------------------------------------
// Antigravity-specific: infer project path from tool call args
// ---------------------------------------------------------------------------

function inferProjectPathFromToolCalls(toolCalls) {
  if (!Array.isArray(toolCalls)) return null;
  for (const call of toolCalls) {
    const args = call?.args;
    if (!args || typeof args !== "object") continue;
    for (const key of ["Cwd", "cwd", "DirectoryPath", "SearchPath", "TargetFile", "AbsolutePath"]) {
      const raw = args[key];
      const value = typeof raw === "string" ? raw.replace(/^"|"$/g, "") : null;
      if (value && value.startsWith("/") && !value.includes("/.gemini/")) {
        // Return the directory portion for file paths
        if (key === "TargetFile" || key === "AbsolutePath") {
          return path.dirname(value);
        }
        return value;
      }
    }
  }
  return null;
}

// ---------------------------------------------------------------------------
// Antigravity-specific: build tool parts from PLANNER_RESPONSE tool_calls
// ---------------------------------------------------------------------------

function toolCallParts(toolCalls) {
  if (!Array.isArray(toolCalls)) return [];
  const parts = [];
  for (const call of toolCalls) {
    const toolName = call?.name;
    if (!toolName) continue;
    const args = call?.args;
    const argsObj = args && typeof args === "object" ? args : null;

    // Reconstruct command for run_command calls
    if (toolName === "run_command") {
      const rawCmd = argsObj?.CommandLine;
      const command = typeof rawCmd === "string" ? rawCmd.replace(/^"|"$/g, "") : null;
      const rawCwd = argsObj?.Cwd;
      const cwd = typeof rawCwd === "string" ? rawCwd.replace(/^"|"$/g, "") : null;
      if (command?.trim()) {
        parts.push({
          role: "tool",
          kind: "command",
          text: null,
          language: null,
          command: command.trim(),
          cwd,
          status: null,
          exit_code: null,
          metadata_json: metadata(compactObject({ type: "command", cwd }), { name: toolName }),
        });
      }
      continue;
    }

    // Generic tool call
    const summary = typeof argsObj?.toolSummary === "string"
      ? argsObj.toolSummary.replace(/^"|"$/g, "")
      : null;
    const displayName = summary || toolName;
    const argEntries = argsObj
      ? Object.entries(argsObj)
          .filter(([k]) => !["toolAction", "toolSummary"].includes(k))
          .map(([k, v]) => `${k}: ${typeof v === "string" ? v.replace(/^"|"$/g, "") : v}`)
      : [];
    const text = argEntries.length
      ? `Tool: ${displayName}\n\n${argEntries.join("\n")}`
      : `Tool: ${displayName}`;
    parts.push({
      role: "tool",
      kind: "tool",
      text,
      language: null,
      command: null,
      cwd: null,
      status: null,
      exit_code: null,
      metadata_json: metadata({ type: "tool", format: "plain" }, { name: toolName }),
    });
  }
  return parts;
}

// ---------------------------------------------------------------------------
// Antigravity-specific: build tool result parts from step output
// ---------------------------------------------------------------------------

function toolResultPart(step, fileOperation = null) {
  const type = step.type ?? "";
  const content = String(step.content ?? "").trim();
  if (!content) return [];
  const status = step.status === "ERROR" ? "error" : step.status === "DONE" ? "success" : null;

  const skillContent = skillContentFromViewFile(step, content);
  if (skillContent) {
    return [{
      role: "tool",
      kind: "metadata",
      text: skillContent.body,
      language: null,
      command: null,
      cwd: null,
      status,
      exit_code: null,
      metadata_json: metadata(
        compactObject({ type: "skill-content", format: "markdown", status }),
        { source_type: type, skill_path: skillContent.path, detected_from_view_file: true },
      ),
    }];
  }

  if (type === "RUN_COMMAND") {
    return [{
      role: "tool",
      kind: "tool",
      text: content,
      language: null,
      command: null,
      cwd: null,
      status,
      exit_code: null,
      metadata_json: metadata(
        compactObject({ type: "result", format: "plain", status }),
        { source_type: type },
      ),
    }];
  }

  if (type === "CODE_ACTION") {
    return [{
      role: "tool",
      kind: "file_change",
      text: materializeFileChangeText(content, fileOperation),
      language: null,
      command: null,
      cwd: null,
      status,
      exit_code: null,
      metadata_json: metadata(
        compactObject({ type: "result", format: "plain", status }),
        { source_type: type },
      ),
    }];
  }

  if (type === "ERROR_MESSAGE") {
    const errorText = step.error ?? content;
    return [{
      role: "tool",
      kind: "tool",
      text: String(errorText).trim(),
      language: null,
      command: null,
      cwd: null,
      status: "error",
      exit_code: null,
      metadata_json: metadata(
        compactObject({ type: "result", format: "plain", status: "error" }),
        { source_type: type },
      ),
    }];
  }

  // VIEW_FILE, GREP_SEARCH, LIST_DIRECTORY, GENERIC
  return [{
    role: "tool",
    kind: "tool",
    text: content,
    language: null,
    command: null,
    cwd: null,
    status,
    exit_code: null,
    metadata_json: metadata(
      compactObject({ type: "result", format: "plain", status }),
      { source_type: type },
    ),
  }];
}

function fileOperationFromToolCall(call, knownFileContents) {
  const name = String(call?.name ?? "").toLowerCase();
  const args = call?.args;
  if (!args || typeof args !== "object") return null;
  if (!["write_to_file", "replace_file_content", "delete_file", "move_file"].includes(name)) {
    return null;
  }

  const targetFile = firstString(args.TargetFile, args.targetFile, args.Path, args.path);
  if (!targetFile) return null;
  const content = firstString(args.CodeContent, args.codeContent, args.ReplacementContent, args.replacementContent);
  const oldContent = firstString(
    args.TargetContent,
    args.targetContent,
    name === "write_to_file" ? knownFileContents.get(targetFile) : null,
  );
  const operation = {
    name,
    targetFile,
    content,
    oldContent,
  };
  if (name === "write_to_file" && content != null) knownFileContents.set(targetFile, content);
  return operation;
}

function firstString(...values) {
  return values.find((value) => typeof value === "string") ?? null;
}

function matchPendingFileOperation(operations, resultText) {
  if (operations.length === 0) return null;
  const normalizedResult = String(resultText ?? "").toLowerCase();
  const matchIndex = operations.findIndex((operation) => {
    const target = operation.targetFile.toLowerCase();
    return normalizedResult.includes(target)
      || normalizedResult.includes(encodeURI(operation.targetFile).toLowerCase());
  });
  if (matchIndex < 0 && operations.length !== 1) return null;
  return operations.splice(matchIndex >= 0 ? matchIndex : 0, 1)[0] ?? null;
}

function materializeFileChangeText(resultText, operation) {
  if (!operation || hasConcreteDiff(resultText)) return resultText;
  if (operation.name === "write_to_file" && operation.content != null) {
    return operation.oldContent != null
      ? replaceFileDiff(operation.targetFile, operation.oldContent, operation.content)
      : createFileDiff(operation.targetFile, operation.content);
  }
  if (operation.name === "replace_file_content" && operation.content != null && operation.oldContent != null) {
    return replaceFileDiff(operation.targetFile, operation.oldContent, operation.content);
  }
  return resultText;
}

function hasConcreteDiff(value) {
  const text = String(value ?? "");
  return text.includes("[diff_block_start]")
    || /^diff --git /m.test(text)
    || (/^--- .+/m.test(text) && /^\+\+\+ .+/m.test(text) && /^@@ /m.test(text));
}

function diffPath(value) {
  let filePath = String(value ?? "").trim().replace(/^file:\/\//i, "");
  try {
    filePath = decodeURIComponent(filePath);
  } catch {
    // Keep the original path when the URI contains malformed escaping.
  }
  const normalized = filePath.replaceAll("\\", "/").replace(/^\/+/, "");
  return { old: `a/${normalized}`, next: `b/${normalized}` };
}

function diffLines(value) {
  const text = String(value ?? "").replace(/\r\n?/g, "\n");
  const hasTrailingNewline = text.endsWith("\n");
  const lines = text.split("\n");
  if (hasTrailingNewline) lines.pop();
  return { lines, hasTrailingNewline };
}

function createFileDiff(targetFile, content) {
  const paths = diffPath(targetFile);
  const { lines, hasTrailingNewline } = diffLines(content);
  const additions = lines.map((line) => `+${line}`);
  if (!hasTrailingNewline && lines.length > 0) additions.push("\\ No newline at end of file");
  const count = lines.length;
  return [
    `diff --git ${paths.old} ${paths.next}`,
    "new file mode 100644",
    "--- /dev/null",
    `+++ ${paths.next}`,
    `@@ -0,0 +1,${count} @@`,
    ...additions,
  ].join("\n");
}

function replaceFileDiff(targetFile, oldContent, newContent) {
  const paths = diffPath(targetFile);
  const oldLines = diffLines(oldContent).lines;
  const newLines = diffLines(newContent).lines;
  return [
    `diff --git ${paths.old} ${paths.next}`,
    `--- ${paths.old}`,
    `+++ ${paths.next}`,
    `@@ -1,${oldLines.length} +1,${newLines.length} @@`,
    ...oldLines.map((line) => `-${line}`),
    ...newLines.map((line) => `+${line}`),
  ].join("\n");
}

function skillContentFromViewFile(step, content) {
  if (step.type !== "VIEW_FILE") return null;
  const filePath = viewFilePath(content);
  if (!filePath || !/\/SKILL\.md$/i.test(filePath)) return null;
  const body = viewFileBody(content);
  if (!body || !/^---\s*$/m.test(body)) return null;
  return { path: filePath, body };
}

function viewFilePath(content) {
  const match = String(content).match(/File Path:\s*`(?:file:\/\/)?([^`]+)`/i);
  if (!match) return null;
  try {
    return decodeURIComponent(match[1].trim());
  } catch {
    return match[1].trim();
  }
}

function viewFileBody(content) {
  const lines = String(content).split(/\r?\n/);
  const firstDocumentLine = lines.findIndex((line) => /^\s*\d+:\s*---\s*$/.test(line));
  if (firstDocumentLine < 0) return null;
  const bodyLines = [];
  for (const line of lines.slice(firstDocumentLine)) {
    if (/^The above content shows the entire, complete file contents of the requested file\.?\s*$/i.test(line.trim())) {
      break;
    }
    const numbered = line.match(/^\s*\d+: ?(.*)$/);
    bodyLines.push(numbered ? numbered[1] : line);
  }
  return bodyLines.join("\n").trim();
}

// ---------------------------------------------------------------------------
// Antigravity transcript parsing
// ---------------------------------------------------------------------------

const IGNORED_STEP_TYPES = new Set([
  "CONVERSATION_HISTORY",
  "KNOWLEDGE_ARTIFACTS",
  "CHECKPOINT",
  "SYSTEM_MESSAGE",
]);

function parseTranscript(text) {
  const turns = [];
  let current = null;
  let projectPath = null;
  const pendingFileOperations = [];
  const knownFileContents = new Map();

  for (const line of text.split(/\r?\n/)) {
    if (!line.trim()) continue;
    let step;
    try {
      step = JSON.parse(line);
    } catch {
      continue;
    }

    const source = step.source ?? "";
    const type = step.type ?? "";
    const timestamp = step.created_at ?? null;

    // Skip system-only noise
    if (source === "SYSTEM" && IGNORED_STEP_TYPES.has(type)) continue;

    // User input → new turn boundary
    if (type === "USER_INPUT" && source === "USER_EXPLICIT") {
      const userText = extractUserRequestText(step.content);
      if (!userText) continue;
      if (current) turns.push(current);
      pendingFileOperations.length = 0;
      current = {
        external_id: `turn-${turns.length}`,
        turn_index: turns.length,
        user_text: userText,
        title: null,
        started_at: timestamp,
        ended_at: null,
        parts: [],
      };
      continue;
    }

    if (!current) continue;

    // PLANNER_RESPONSE → assistant text + tool calls
    if (type === "PLANNER_RESPONSE" && source === "MODEL") {
      const responseText = String(step.content ?? "").trim();
      if (responseText) {
        current.parts.push(...splitMarkdownParts("assistant", responseText));
      }
      // Extract tool call parts
      if (Array.isArray(step.tool_calls) && step.tool_calls.length > 0) {
        pendingFileOperations.push(
          ...step.tool_calls.map((call) => fileOperationFromToolCall(call, knownFileContents)).filter(Boolean),
        );
        current.parts.push(...toolCallParts(step.tool_calls));
        projectPath ??= inferProjectPathFromToolCalls(step.tool_calls);
      }
      current.ended_at = timestamp;
      continue;
    }

    // Tool results (RUN_COMMAND, CODE_ACTION, VIEW_FILE, GREP_SEARCH, LIST_DIRECTORY, ERROR_MESSAGE, GENERIC)
    if (source === "MODEL" || source === "SYSTEM") {
      const resultTypes = [
        "RUN_COMMAND", "CODE_ACTION", "VIEW_FILE",
        "GREP_SEARCH", "LIST_DIRECTORY", "ERROR_MESSAGE", "GENERIC",
      ];
      if (resultTypes.includes(type)) {
        const fileOperation = type === "CODE_ACTION"
          ? matchPendingFileOperation(pendingFileOperations, step.content)
          : null;
        current.parts.push(...toolResultPart(step, fileOperation));
        current.ended_at = timestamp;
      }
    }
  }
  if (current) turns.push(current);
  return { turns, projectPath };
}

// ---------------------------------------------------------------------------
// Session discovery
// ---------------------------------------------------------------------------

function findTranscriptFile(conversationDir) {
  // Prefer transcript_full.jsonl (complete content), fallback to transcript.jsonl
  const logsDir = path.join(conversationDir, ".system_generated", "logs");
  const fullPath = path.join(logsDir, "transcript_full.jsonl");
  if (existsSync(fullPath)) return fullPath;
  const shortPath = path.join(logsDir, "transcript.jsonl");
  if (existsSync(shortPath)) return shortPath;
  return null;
}

function isUuidDir(name) {
  return /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(name);
}

function discoverConversationDirs(brainDir) {
  if (!existsSync(brainDir)) return [];
  const dirs = [];
  try {
    for (const entry of readdirSync(brainDir, { withFileTypes: true })) {
      if (entry.isDirectory() && isUuidDir(entry.name)) {
        dirs.push(path.join(brainDir, entry.name));
      }
    }
  } catch {
    return [];
  }
  return dirs;
}

function inferProjectPath(turns) {
  for (const turn of turns) {
    for (const part of turn.parts) {
      if (part.cwd?.trim()) return part.cwd;
    }
  }
  return null;
}

function titleFromUserText(userText) {
  if (!userText) return null;
  const firstLine = userText.split(/\r?\n/).map((line) => line.trim()).find(Boolean) ?? "";
  if (firstLine.length <= 80) return firstLine;
  return `${firstLine.slice(0, 77)}...`;
}

function readSession() {
  let location = expandPath(input.source?.location);
  if (!location) return [];

  // Determine the brain directory
  let brainDir = location;
  try {
    const stat = statSync(location);
    if (stat.isFile()) {
      // Pointing to a transcript file directly
      const text = readFileSync(location, "utf8");
      const parsed = parseTranscript(text);
      const turns = displayTurns(parsed.turns);
      if (!turns.length) return [];
      const externalId = path.basename(path.resolve(location, "../../..")) || "antigravity-session";
      return [applyTextBudgets(finalizeStructuredContentCards({
        external_id: externalId,
        title: titleFromUserText(turns[0]?.user_text),
        project_path: parsed.projectPath ?? inferProjectPath(turns),
        started_at: turns[0]?.started_at ?? null,
        updated_at: turns.at(-1)?.ended_at ?? null,
        source_locator: location,
        source_fingerprint: sourceFingerprint(text),
        turns,
      }))];
    }
  } catch {
    return [];
  }

  // Check if this is a single conversation dir (contains .system_generated)
  const transcriptInDir = findTranscriptFile(brainDir);
  if (transcriptInDir) {
    const text = readFileSync(transcriptInDir, "utf8");
    const parsed = parseTranscript(text);
    const turns = displayTurns(parsed.turns);
    if (!turns.length) return [];
    const externalId = path.basename(brainDir) || "antigravity-session";
    return [applyTextBudgets(finalizeStructuredContentCards({
      external_id: externalId,
      title: titleFromUserText(turns[0]?.user_text),
      project_path: parsed.projectPath ?? inferProjectPath(turns),
      started_at: turns[0]?.started_at ?? null,
      updated_at: turns.at(-1)?.ended_at ?? null,
      source_locator: transcriptInDir,
      source_fingerprint: sourceFingerprint(text),
      turns,
    }))];
  }

  // Brain directory: enumerate conversation subdirectories
  const conversationDirs = discoverConversationDirs(brainDir);
  const sessions = [];
  for (const convDir of conversationDirs) {
    const transcriptPath = findTranscriptFile(convDir);
    if (!transcriptPath) continue;
    let text;
    try {
      text = readFileSync(transcriptPath, "utf8");
    } catch {
      continue;
    }
    const parsed = parseTranscript(text);
    const turns = displayTurns(parsed.turns);
    if (!turns.length) continue;
    const externalId = path.basename(convDir);
    sessions.push(applyTextBudgets(finalizeStructuredContentCards({
      external_id: externalId,
      title: titleFromUserText(turns[0]?.user_text),
      project_path: parsed.projectPath ?? inferProjectPath(turns),
      started_at: turns[0]?.started_at ?? null,
      updated_at: turns.at(-1)?.ended_at ?? null,
      source_locator: transcriptPath,
      source_fingerprint: sourceFingerprint(text),
      turns,
    })));
  }
  return sessions;
}

function executionTextValue(value, depth = 0) {
  if (value == null || depth > 6) return "";
  if (typeof value === "string") {
    const source = value.trim();
    if (/^[\[{\"]/.test(source)) {
      try {
        return executionTextValue(JSON.parse(source), depth + 1);
      } catch {
        // A normal terminal line may begin with a bracket; preserve it verbatim.
      }
    }
    return value;
  }
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  if (Array.isArray(value)) {
    const structuredFragments = value.length > 0 && value.every((entry) => (
      entry && typeof entry === "object" && !Array.isArray(entry)
      && ["stdout", "stderr", "output", "result", "content", "text", "message"].some((key) => key in entry)
    ));
    if (!structuredFragments) return JSON.stringify(value, null, 2);
    return value.map((entry) => executionTextValue(entry, depth + 1)).filter(Boolean).reduce(
      (text, fragment) => !text || text.endsWith("\n") || fragment.startsWith("\n")
        ? text + fragment
        : `${text}\n${fragment}`,
      "",
    );
  }
  if (typeof value === "object") {
    const streams = [value.stdout, value.stderr]
      .map((entry) => executionTextValue(entry, depth + 1))
      .filter((entry) => entry.trim());
    if (streams.length) return streams.join("\n");
    for (const key of ["output", "result", "content", "text", "message"]) {
      if (value[key] != null) return executionTextValue(value[key], depth + 1);
    }
    return JSON.stringify(value, null, 2);
  }
  return String(value);
}

function normalizeTerminalText(value) {
  return String(value ?? "")
    .replace(/\r\n/g, "\n")
    .replace(/\u001B\][^\u0007]*(?:\u0007|\u001B\\)/g, "")
    .replace(/\u001B\[[0-?]*[ -/]*[@-~]/g, "")
    .replace(/[\u0000\u0008\u000B\u000C\u000E-\u001F\u007F]/g, "")
    .split("\n")
    .map((line) => (line.includes("\r") ? line.slice(line.lastIndexOf("\r") + 1) : line).trimEnd())
    .join("\n")
    .trim()
    .replace(/\n{3,}/g, "\n\n");
}

function stripExecutionEnvelope(value) {
  const lines = String(value ?? "").split("\n");
  const header = /^(?:Created At:.*|Completed At:.*|Script completed(?: successfully)?(?: in .*)?|Script running with cell ID\b.*|Wall time\b.*|Chunk ID:.*|Process exited with code\b.*|Original token count:.*|The command completed successfully\.?|(?:Final |Original |Command )?Output:)$/i;
  let cursor = 0;
  let matched = false;
  while (cursor < lines.length) {
    const line = lines[cursor].trim();
    if (header.test(line)) {
      matched = true;
      cursor += 1;
    } else if (matched && !line) {
      cursor += 1;
    } else {
      break;
    }
  }
  return matched ? normalizeTerminalText(lines.slice(cursor).join("\n")) : value;
}

function normalizeExecutionResultText(value) {
  const withoutEnvelope = stripExecutionEnvelope(normalizeTerminalText(executionTextValue(value)));
  return normalizeTerminalText(executionTextValue(withoutEnvelope));
}

function normalizeExecutionCommandText(value) {
  const source = String(value ?? "").trim();
  if (/^[{\"]/.test(source)) {
    try {
      const parsed = JSON.parse(source);
      if (typeof parsed === "string") return normalizeTerminalText(parsed);
      if (parsed && typeof parsed === "object") {
        const command = parsed.command ?? parsed.cmd ?? parsed.shell_command;
        if (typeof command === "string") return normalizeTerminalText(command);
      }
    } catch {
      // Preserve command syntax that only resembles JSON.
    }
  }
  return normalizeTerminalText(value);
}

function structuredCardType(part, legacyCard) {
  if (typeof legacyCard?.type === "string") return legacyCard.type;
  const kind = part?.content_card?.kind;
  return typeof kind === "string" ? kind.slice(kind.lastIndexOf(".") + 1) : null;
}

function finalizeStructuredContentCards(session) {
  for (const turn of Array.isArray(session?.turns) ? session.turns : []) {
    for (const part of Array.isArray(turn?.parts) ? turn.parts : []) {
      const metadataValue = parseStructuredMetadata(part.metadata_json);
      const legacyCard = metadataValue.content_card ?? metadataValue.contentCard;
      const cardType = structuredCardType(part, legacyCard);
      if (cardType === "result" && typeof part.text === "string") {
        part.text = normalizeExecutionResultText(part.text) || null;
      } else if (cardType === "command") {
        const field = typeof part.command === "string" ? "command" : typeof part.text === "string" ? "text" : null;
        if (field) part[field] = normalizeExecutionCommandText(part[field]) || null;
      }
      if (!part.content_card && legacyCard && typeof legacyCard === "object" && typeof legacyCard.type === "string") {
        part.content_card = {
          schema_version: 1,
          kind: `antigravity.${legacyCard.type}`,
          renderer: structuredCardRenderer(legacyCard),
        };
      }
      delete metadataValue.content_card;
      delete metadataValue.contentCard;
      part.metadata_json = Object.keys(metadataValue).length > 0 ? JSON.stringify(metadataValue) : null;
    }
  }
  return normalizeSessionPayload(session);
}

function parseStructuredMetadata(value) {
  if (!value) return {};
  if (typeof value === "object" && !Array.isArray(value)) return { ...value };
  try {
    const parsed = JSON.parse(value);
    return parsed && typeof parsed === "object" && !Array.isArray(parsed) ? { ...parsed } : {};
  } catch {
    return {};
  }
}

function structuredCardRenderer(card) {
  if (card.type === "code") return "code";
  if (card.type === "command") return "command";
  if (card.type === "result") {
    return ["markdown", "json"].includes(card.format) ? card.format : "terminal_output";
  }
  if (["markdown", "plain", "json"].includes(card.format)) return card.format;
  if (card.type === "answer") return "markdown";
  if (card.type === "skill-content") return "markdown";
  return "plain";
}

try {
  if (input.method === "probe" || input.method === "list_sessions") {
    emit("complete", { item: { session_count: 0 } });
  } else if (input.method === "read_session") {
    const sessions = readSession();
    for (const session of sessions) emit("item", { item: { kind: "session", session: finalizeStructuredContentCards(session) } });
    emit("complete", { item: { session_count: sessions.length } });
  } else {
    fail(`unsupported method: ${input.method}`);
  }
} catch (error) {
  fail(error instanceof Error ? error.message : String(error));
}
