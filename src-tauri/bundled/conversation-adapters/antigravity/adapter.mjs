#!/usr/bin/env node
import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { homedir } from "node:os";
import path from "node:path";

const input = JSON.parse(readFileSync(0, "utf8") || "{}");
const CONTENT_CARD_SCHEMA_VERSION = "antigravity-content-cards-v1";
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

function toolResultPart(step) {
  const type = step.type ?? "";
  const content = String(step.content ?? "").trim();
  if (!content) return [];
  const status = step.status === "ERROR" ? "error" : step.status === "DONE" ? "success" : null;

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
        current.parts.push(...toolResultPart(step));
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
      return [applyTextBudgets({
        external_id: externalId,
        title: titleFromUserText(turns[0]?.user_text),
        project_path: parsed.projectPath ?? inferProjectPath(turns),
        started_at: turns[0]?.started_at ?? null,
        updated_at: turns.at(-1)?.ended_at ?? null,
        source_locator: location,
        source_fingerprint: sourceFingerprint(text),
        turns,
      })];
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
    return [applyTextBudgets({
      external_id: externalId,
      title: titleFromUserText(turns[0]?.user_text),
      project_path: parsed.projectPath ?? inferProjectPath(turns),
      started_at: turns[0]?.started_at ?? null,
      updated_at: turns.at(-1)?.ended_at ?? null,
      source_locator: transcriptInDir,
      source_fingerprint: sourceFingerprint(text),
      turns,
    })];
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
    sessions.push(applyTextBudgets({
      external_id: externalId,
      title: titleFromUserText(turns[0]?.user_text),
      project_path: parsed.projectPath ?? inferProjectPath(turns),
      started_at: turns[0]?.started_at ?? null,
      updated_at: turns.at(-1)?.ended_at ?? null,
      source_locator: transcriptPath,
      source_fingerprint: sourceFingerprint(text),
      turns,
    }));
  }
  return sessions;
}

// ---------------------------------------------------------------------------
// Markdown export
// ---------------------------------------------------------------------------

function exportMarkdown() {
  const params = input.params ?? {};
  const detail = params.session_detail ?? params.sessionDetail;
  if (!detail || typeof detail !== "object") {
    throw new Error("export_markdown requires params.session_detail");
  }
  return {
    content: renderSessionMarkdown(
      detail,
      params.question_ids ?? params.questionIds ?? [],
      params.content_filter ?? params.contentFilter ?? {},
    ),
    relative_path: String(params.default_relative_path ?? params.defaultRelativePath ?? "conversation-export.md").trim(),
  };
}

function renderSessionMarkdown(detail, questionIds, contentFilter) {
  const session = detail.session ?? {};
  const selectedIds = new Set(Array.isArray(questionIds) ? questionIds.map(String) : []);
  const questions = Array.isArray(detail.questions)
    ? detail.questions.filter((entry) => !selectedIds.size || selectedIds.has(String(entry?.question?.id ?? "")))
    : [];
  const lines = [
    `# ${headingText(session.title || session.external_id || "Conversation export")}`,
    "",
    "**Session Metadata**",
    "",
    `- Adapter: \`${session.adapter_id ?? ""}\``,
    `- Source: \`${session.source_id ?? ""}\``,
    `- External Session: \`${session.external_id ?? ""}\``,
  ];
  if (session.project_path) lines.push(`- Project: \`${session.project_path}\``);
  if (session.updated_at) lines.push(`- Updated: \`${session.updated_at}\``);
  lines.push("");
  questions.forEach((entry, index) => {
    const question = entry.question ?? {};
    const title = question.title || firstMarkdownLine(question.question_text) || `Question ${index + 1}`;
    lines.push(`## ${index + 1}. ${headingText(title)}`, "");
    const partsByTurn = new Map();
    for (const part of [...(entry.parts ?? [])].sort((a, b) => Number(a.part_index ?? 0) - Number(b.part_index ?? 0))) {
      const turnId = String(part.turn_id ?? "");
      if (!partsByTurn.has(turnId)) partsByTurn.set(turnId, []);
      partsByTurn.get(turnId).push(part);
    }
    for (const turn of [...(entry.turns ?? [])].sort((a, b) => Number(a.turn_index ?? 0) - Number(b.turn_index ?? 0))) {
      for (const part of partsByTurn.get(String(turn.id ?? "")) ?? []) {
        const rendered = renderContentCard(part, contentFilter);
        if (rendered) lines.push(rendered);
      }
    }
  });
  return lines.join("\n").replace(/\n{3,}/g, "\n\n").trimEnd() + "\n";
}

function renderContentCard(part, contentFilter) {
  const card = contentCardMetadata(part.metadata_json);
  const type = typeof card?.type === "string" ? card.type : null;
  if (!["answer", "tool", "command", "code", "result"].includes(type)) return "";
  if (contentFilter?.[type] === false) return "";
  const text = cardString(card.text) ?? defaultCardText(part, type);
  if (!text?.trim()) return "";
  if (type === "code") return fencedSection("Code", cardString(card.language) ?? part.language, text);
  if (type === "command") return fencedSection("Command", "sh", text);
  if (type === "result") {
    return fencedSection("Result", card.format === "markdown" ? "markdown" : "", text);
  }
  return fencedSection(type === "tool" ? "Tool" : "Answer", card.format === "markdown" ? "markdown" : "", text);
}

function contentCardMetadata(value) {
  const meta = typeof value === "string" ? parseJsonValue(value) : value;
  const card = meta?.content_card ?? meta?.contentCard;
  return card && typeof card === "object" && !Array.isArray(card) ? card : null;
}

function defaultCardText(part, type) {
  if (type === "command") return String(part.command ?? part.text ?? "").trim();
  return String(part.text ?? part.command ?? "").trim();
}

function fencedSection(label, language, value) {
  const text = String(value ?? "").trim();
  if (!text) return "";
  const fence = markdownFenceFor(text);
  const suffix = language ? String(language).trim() : "";
  return `### ${label}\n\n${fence}${suffix}\n${text}\n${fence}\n`;
}

function markdownFenceFor(text) {
  const runs = String(text).match(/`+/g) ?? [];
  const longest = runs.reduce((max, run) => Math.max(max, run.length), 0);
  return "`".repeat(Math.max(3, longest + 1));
}

function headingText(value) {
  return String(value ?? "").replace(/\s+/g, " ").trim() || "Untitled";
}

function firstMarkdownLine(value) {
  return String(value ?? "").split(/\r?\n/).map((line) => line.trim()).find(Boolean) ?? "";
}

function cardString(value) {
  return typeof value === "string" && value.trim() ? value : null;
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

try {
  if (input.method === "probe" || input.method === "list_sessions") {
    emit("complete", { item: { session_count: 0 } });
  } else if (input.method === "read_session") {
    const sessions = readSession();
    for (const session of sessions) emit("item", { item: { kind: "session", session } });
    emit("complete", { item: { session_count: sessions.length } });
  } else if (input.method === "export_markdown") {
    emit("item", { item: { kind: "markdown_export", ...exportMarkdown() } });
    emit("complete", { item: { export_count: 1 } });
  } else {
    fail(`unsupported method: ${input.method}`);
  }
} catch (error) {
  fail(error instanceof Error ? error.message : String(error));
}
