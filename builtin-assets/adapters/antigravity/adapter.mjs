#!/usr/bin/env node
import { normalizeSessionPayload } from "./payload-policy.mjs";
import shellProjector from "./shell-projector.cjs";
const { projectCommandParts, SHELL_PROJECTOR_VERSION } = shellProjector;
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { homedir } from "node:os";
import path from "node:path";

let DatabaseSync = null;
try {
  const sqliteModule = await import("node:sqlite");
  DatabaseSync = sqliteModule.DatabaseSync ?? null;
} catch {}

const input = JSON.parse(readFileSync(0, "utf8") || "{}");
const CONTENT_CARD_SCHEMA_VERSION = "antigravity-content-cards-v9";
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

function emitProgress(progress = {}) {
  const p = {
    stage: progress.stage ?? "reading",
    operation: progress.operation ?? "scanning",
    worker: progress.worker ?? process.env.ASSETIWEAVE_WORKER_ID ?? undefined,
    path: progress.path,
    current: progress.current,
    total: progress.total,
  };
  emit("progress", {
    ...p,
    progress: p,
  });
}

function expandPath(value) {
  if (!value) return value;
  if (Array.isArray(value)) return value[0] ? expandPath(value[0]) : null;
  if (typeof value !== "string") return String(value);
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


function displayTurns(turns) {
  return turns
    .filter((turn) => (Array.isArray(turn.parts) && turn.parts.length > 0) || turn.turn_kind === "subagent" || turn.turn_kind === "continuation" || Boolean(turn.user_text?.trim()))
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
  if (startIndex >= 0) {
    const textStart = startIndex + openTag.length;
    const endIndex = content.indexOf(closeTag, textStart);
    const text = endIndex < 0 ? content.slice(textStart) : content.slice(textStart, endIndex);
    return text.trim() || null;
  }
  return String(content).trim() || null;
}

// ---------------------------------------------------------------------------
// Antigravity-specific: infer project path from tool call args
// ---------------------------------------------------------------------------

function findGitRoot(startDir) {
  if (!startDir || typeof startDir !== "string") return null;
  try {
    let current = path.resolve(startDir);
    const root = path.parse(current).root;
    while (current && current !== root) {
      if (existsSync(path.join(current, ".git"))) {
        return current;
      }
      const parent = path.dirname(current);
      if (parent === current) break;
      current = parent;
    }
  } catch {}
  return null;
}

function inferProjectPathFromToolCalls(toolCalls) {
  if (!Array.isArray(toolCalls)) return null;
  for (const call of toolCalls) {
    const args = call?.args;
    if (!args || typeof args !== "object") continue;
    for (const key of [
      "Cwd", "cwd",
      "DirectoryPath", "directoryPath",
      "SearchPath", "searchPath",
      "TargetFile", "targetFile",
      "AbsolutePath", "absolutePath",
      "Path", "path",
    ]) {
      const raw = args[key];
      const value = typeof raw === "string" ? raw.replace(/^"|"$/g, "") : null;
      // 应用私有数据目录（~/.assetiweave、~/.gemini 等）里的路径不具项目语义，
      // 继续看后面的工具调用，而不是把整条会话钉在数据目录上。
      if (value && value.startsWith("/") && !isAppDataPath(value)) {
        let dir = value;
        try {
          if (existsSync(value)) {
            dir = statSync(value).isDirectory() ? value : path.dirname(value);
          } else if (path.extname(value) || /[\/\\][^.\/\\]+\.[A-Za-z0-9_-]+$/.test(value)) {
            dir = path.dirname(value);
          }
        } catch {
          dir = path.dirname(value);
        }
        const gitRoot = findGitRoot(dir);
        if (gitRoot) return gitRoot;
        if (!isAppDataPath(dir)) return dir;
      }
    }
  }
  return null;
}

// ---------------------------------------------------------------------------
// Antigravity-specific: build tool parts from PLANNER_RESPONSE tool_calls
// ---------------------------------------------------------------------------

function toolCallExecutionId(call, turnId, toolIndex) {
  for (const key of ["id", "call_id", "callId", "execution_id", "executionId"]) {
    const value = call?.[key];
    if (typeof value === "string" && value.trim()) return value.trim();
  }
  const toolName = call?.name === "run_command" ? "run-command" : (call?.name || "tool");
  return `${turnId}:${toolName}:${toolIndex}`;
}

function runCommandExecutionIds(toolCalls, turnId) {
  let commandIndex = 0;
  return (Array.isArray(toolCalls) ? toolCalls : [])
    .filter((call) => call?.name === "run_command")
    .map((call) => {
      commandIndex += 1;
      return toolCallExecutionId(call, turnId, commandIndex);
    });
}

function toolCallParts(toolCalls, turnId) {
  if (!Array.isArray(toolCalls)) return [];
  const parts = [];
  let toolIndex = 0;
  for (const call of toolCalls) {
    const toolName = call?.name;
    if (!toolName) continue;
    toolIndex += 1;
    const executionId = toolCallExecutionId(call, turnId, toolIndex);
    const args = call?.args;
    const argsObj = args && typeof args === "object" ? args : null;

    // Reconstruct command for run_command calls
    if (toolName === "run_command") {
      const rawCmd = argsObj?.CommandLine;
      const command = typeof rawCmd === "string" ? rawCmd.replace(/^"|"$/g, "") : null;
      const rawCwd = argsObj?.Cwd;
      const cwd = typeof rawCwd === "string" ? rawCwd.replace(/^"|"$/g, "") : null;
      const rawSummary = typeof argsObj?.toolSummary === "string" ? argsObj.toolSummary.replace(/^"|"$/g, "").trim() : null;
      const rawAction = typeof argsObj?.toolAction === "string" ? argsObj.toolAction.replace(/^"|"$/g, "").trim() : null;
      const commandLabel = rawSummary || rawAction || null;
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
          source_execution_id: executionId,
          command_label: commandLabel,
          metadata_json: metadata(
            compactObject({ type: "command", cwd, source_execution_id: executionId, tool_name: toolName, command_label: commandLabel }),
            { name: toolName, tool_name: toolName, tool_input: argsObj, command_label: commandLabel },
          ),
        });
      }
      continue;
    }

    // Subagent dispatch is modeled as its own specialized turn; no inline card is emitted here.
    if (toolName === "invoke_subagent" || toolName === "call_subagent") {
      continue;
    }

    // Generic / structured tool call
    const summary = typeof argsObj?.toolSummary === "string"
      ? argsObj.toolSummary.replace(/^"|"$/g, "").trim()
      : null;
    const action = typeof argsObj?.toolAction === "string"
      ? argsObj.toolAction.replace(/^"|"$/g, "").trim()
      : null;
    const toolLabel = summary || action || null;
    const displayName = toolLabel || toolName;

    const isRead = /^(view_file|read_file|read_url_content|list_dir|list_directory|read_dir|read_resource)$/i.test(toolName);
    const isSearch = /^(grep_search|search_web|search_docs|find_files)$/i.test(toolName);
    const executionKind = isRead ? "read" : isSearch ? "search" : null;

    let targetLocation = argsObj?.AbsolutePath ?? argsObj?.TargetFile ?? argsObj?.Path ?? argsObj?.path ?? argsObj?.Url ?? null;
    if (typeof targetLocation === "string") {
      targetLocation = targetLocation.replace(/^"|"$/g, "");
    }

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
      source_execution_id: executionId,
      command_label: toolLabel,
      metadata_json: metadata(
        compactObject({
          type: "tool",
          format: "plain",
          tool_name: toolName,
          execution_kind: executionKind,
          signal: executionKind ? "ambient" : undefined,
          file_path: targetLocation,
          source_execution_id: executionId,
          command_label: toolLabel,
        }),
        {
          name: toolName,
          tool_name: toolName,
          tool_input: argsObj,
          execution_kind: executionKind,
          signal: executionKind ? "ambient" : undefined,
          command_label: toolLabel,
        },
      ),
    });
  }
  return parts;
}

// ---------------------------------------------------------------------------
// Antigravity-specific: build tool result parts from step output
// ---------------------------------------------------------------------------

function toolResultPart(step, fileOperation = null, sourceExecutionId = null, matchedCall = null) {
  const type = step.type ?? "";
  const content = String(step.content ?? "").trim();
  if (!content) return [];
  const toolName = String(matchedCall?.name ?? "").toLowerCase();

  const skillContent = skillContentFromViewFile(step, content, matchedCall);
  if (skillContent) {
    return [{
      role: "tool",
      kind: "metadata",
      text: skillContent.body,
      language: null,
      command: null,
      cwd: null,
      status: "success",
      exit_code: 0,
      source_execution_id: sourceExecutionId,
      content_card: {
        schema_version: 1,
        kind: "antigravity.skill-content",
        semantic_role: "skill",
        renderer: "markdown",
      },
      metadata_json: metadata(
        compactObject({ type: "skill-content", format: "markdown", status: "success" }),
        { source_type: type, skill_path: skillContent.path, detected_from_view_file: true },
      ),
    }];
  }

  const isRunCommand = type === "RUN_COMMAND"
    || toolName === "run_command"
    || /(?:The command|Process)\s+exited\s+with\s+code/i.test(content);

  if (isRunCommand) {
    let exitCode = null;
    const exitMatch = content.match(/(?:The command|Process)\s+exited\s+with\s+code\s+(-?\d+)/i);
    if (exitMatch) {
      exitCode = parseInt(exitMatch[1], 10);
    }
    const derivedStatus = exitCode !== null
      ? (exitCode === 0 ? "success" : "failed")
      : (step.status === "ERROR" ? "error" : "success");

    const cleanText = normalizeTerminalText(stripExecutionEnvelope(content));
    if (derivedStatus === "success" && !cleanText.trim()) {
      return [];
    }

    return [{
      role: "tool",
      kind: "tool",
      text: cleanText || content,
      language: null,
      command: null,
      cwd: null,
      status: derivedStatus,
      exit_code: exitCode,
      source_execution_id: sourceExecutionId,
      content_card: {
        schema_version: 1,
        kind: "antigravity.result",
        semantic_role: "result",
        renderer: "terminal_output",
      },
      metadata_json: metadata(
        compactObject({
          type: "result",
          format: "plain",
          status: derivedStatus,
          exit_code: exitCode,
          source_execution_id: sourceExecutionId,
        }),
        { source_type: type, tool_name: "run_command" },
      ),
    }];
  }

  const isCodeAction = type === "CODE_ACTION"
    || ["write_to_file", "write_file", "replace_file_content", "edit_file", "apply_patch"].includes(toolName)
    || (matchedCall && /write|replace|edit|patch/i.test(matchedCall.name));

  if (isCodeAction) {
    const diffText = materializeFileChangeText(content, fileOperation);
    if (hasConcreteDiff(diffText)) {
      return [{
        role: "tool",
        kind: "file_change",
        text: diffText,
        language: null,
        command: null,
        cwd: null,
        status: "success",
        exit_code: 0,
        source_execution_id: sourceExecutionId,
        content_card: {
          schema_version: 1,
          kind: "antigravity.file-change",
          semantic_role: "file-change",
          renderer: "diff",
        },
        metadata_json: metadata(
          compactObject({
            type: "result",
            format: "plain",
            status: "success",
            execution_kind: "file_change",
            source_execution_id: sourceExecutionId,
          }),
          { source_type: type, tool_name: toolName || "code_action" },
        ),
      }];
    }
  }

  const isViewFile = type === "VIEW_FILE"
    || ["view_file", "read_file", "read_url_content"].includes(toolName);

  if (isViewFile) {
    const filePath = viewFilePath(content) || firstString(matchedCall?.args?.AbsolutePath, matchedCall?.args?.TargetFile, matchedCall?.args?.Path, matchedCall?.args?.path, matchedCall?.args?.Url);
    return [{
      role: "tool",
      kind: "tool",
      text: content,
      language: null,
      command: null,
      cwd: null,
      status: "success",
      exit_code: 0,
      source_execution_id: sourceExecutionId,
      content_card: {
        schema_version: 1,
        kind: "antigravity.result",
        semantic_role: "result",
        renderer: "terminal_output",
      },
      metadata_json: metadata(
        compactObject({
          type: "result",
          format: "plain",
          status: "success",
          execution_kind: "read",
          signal: "ambient",
          file_path: filePath,
          source_execution_id: sourceExecutionId,
        }),
        { source_type: type, tool_name: toolName || "view_file", file_path: filePath, signal: "ambient", execution_kind: "read" },
      ),
    }];
  }

  const isSearch = type === "GREP_SEARCH"
    || ["grep_search", "search_web", "search_docs", "find_files"].includes(toolName);

  if (isSearch) {
    return [{
      role: "tool",
      kind: "tool",
      text: content,
      language: null,
      command: null,
      cwd: null,
      status: "success",
      exit_code: 0,
      source_execution_id: sourceExecutionId,
      content_card: {
        schema_version: 1,
        kind: "antigravity.result",
        semantic_role: "result",
        renderer: "terminal_output",
      },
      metadata_json: metadata(
        compactObject({
          type: "result",
          format: "plain",
          status: "success",
          execution_kind: "search",
          signal: "ambient",
          source_execution_id: sourceExecutionId,
        }),
        { source_type: type, tool_name: toolName || "search", signal: "ambient", execution_kind: "search" },
      ),
    }];
  }

  if (type === "ERROR_MESSAGE" || step.status === "ERROR") {
    const errorText = step.error ?? content;
    return [{
      role: "tool",
      kind: "tool",
      text: String(errorText).trim(),
      language: null,
      command: null,
      cwd: null,
      status: "error",
      exit_code: 1,
      source_execution_id: sourceExecutionId,
      metadata_json: metadata(
        compactObject({ type: "result", format: "plain", status: "error", exit_code: 1, source_execution_id: sourceExecutionId }),
        { source_type: type, tool_name: toolName || undefined },
      ),
    }];
  }

  const status = step.status === "DONE" ? "success" : (step.status === "ERROR" ? "error" : "success");
  return [{
    role: "tool",
    kind: "tool",
    text: content,
    language: null,
    command: null,
    cwd: null,
    status,
    exit_code: null,
    source_execution_id: sourceExecutionId,
    metadata_json: metadata(
      compactObject({ type: "result", format: "plain", status, source_execution_id: sourceExecutionId }),
      { source_type: type, tool_name: toolName || undefined },
    ),
  }];
}

function fileOperationFromToolCall(call, knownFileContents) {
  const name = String(call?.name ?? "").toLowerCase();
  const args = call?.args;
  if (!args || typeof args !== "object") return null;
  if (![
    "write_to_file", "write_file",
    "replace_file_content", "edit_file",
    "delete_file", "move_file",
  ].includes(name)) {
    return null;
  }

  const targetFile = firstString(
    args.TargetFile, args.targetFile,
    args.Path, args.path,
    args.AbsolutePath, args.absolutePath,
  );
  if (!targetFile) return null;

  if (name === "edit_file") {
    const target = firstString(args.Target, args.target);
    const replacement = firstString(args.Replacement, args.replacement) ?? "";
    return {
      name: "edit_file",
      targetFile,
      target,
      replacement,
    };
  }

  if (name === "write_file") {
    const content = firstString(args.Content, args.content, args.CodeContent, args.codeContent);
    const isAppend = args.Append === true || args.Append === "true" || args.append === true;
    const previous = knownFileContents.get(targetFile);
    let finalContent = content;
    let oldContent = previous;
    if (isAppend && content != null) {
      finalContent = (previous ?? "") + content;
    }
    if (finalContent != null) knownFileContents.set(targetFile, finalContent);
    return {
      name: "write_file",
      targetFile,
      content: finalContent,
      oldContent,
      isAppend,
    };
  }

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
  if ((operation.name === "write_to_file" || operation.name === "write_file") && operation.content != null) {
    return operation.oldContent != null
      ? replaceFileDiff(operation.targetFile, operation.oldContent, operation.content)
      : createFileDiff(operation.targetFile, operation.content);
  }
  if (operation.name === "replace_file_content" && operation.content != null && operation.oldContent != null) {
    return replaceFileDiff(operation.targetFile, operation.oldContent, operation.content);
  }
  if (operation.name === "edit_file" && (operation.target != null || operation.replacement != null)) {
    return editFileDiff(operation.targetFile, operation.target ?? "", operation.replacement ?? "", resultText);
  }
  return resultText;
}

function editFileDiff(targetFile, target, replacement, resultText = "") {
  const paths = diffPath(targetFile);
  const oldLines = target ? diffLines(target).lines : [];
  const newLines = replacement ? diffLines(replacement).lines : [];

  let startLine = 1;
  const lineMatch = String(resultText).match(/starting at line\s+(\d+)/i);
  if (lineMatch) {
    startLine = parseInt(lineMatch[1], 10) || 1;
  }

  const oldLen = oldLines.length;
  const newLen = newLines.length;
  const deletions = oldLines.map((line) => `-${line}`);
  const additions = newLines.map((line) => `+${line}`);

  return [
    `diff --git ${paths.old} ${paths.next}`,
    `--- ${paths.old}`,
    `+++ ${paths.next}`,
    `@@ -${startLine},${oldLen} +${startLine},${newLen} @@`,
    ...deletions,
    ...additions,
  ].join("\n");
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

function operationFromMatchedCall(call, knownFileContents) {
  if (!call || !call.args) return null;
  return fileOperationFromToolCall(call, knownFileContents);
}

function skillContentFromViewFile(step, content, matchedCall = null) {
  const isView = step.type === "VIEW_FILE" || matchedCall?.name === "view_file" || /File Path:\s*`.*?SKILL\.md`/i.test(content);
  if (!isView) return null;
  const filePath = viewFilePath(content) || firstString(matchedCall?.args?.AbsolutePath, matchedCall?.args?.TargetFile, matchedCall?.args?.Path, matchedCall?.args?.path);
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
// Antigravity-specific: subagent registry tree
//
// Antigravity records every dispatched subagent in
// `<conversation>/.system_generated/subagents/<conversationId>.json`, and each
// subagent keeps its own conversation directory. The registry is the source of
// truth for the tree; the main transcript only points at the dispatching step.
// ---------------------------------------------------------------------------

const SUBAGENT_REGISTRY_DIR = path.join(".system_generated", "subagents");
const MAX_SUBAGENT_DEPTH = 4;
const SUBAGENT_PROMPT_FALLBACK = "Subagent execution";
const INJECTED_PROMPT_BLOCK =
  /^\s*<(?:SYSTEM_MESSAGE|system_message|system_information|environment_context|user_instructions|additional_metadata|context|plugins?|plugin_list|plugin_inventory|available_plugins|rules|repository_context|INSTRUCTIONS)\b/i;

function emitWarning(message) {
  emit("warning", { message });
}

function isSubagentCall(call) {
  return call?.name === "invoke_subagent" || call?.name === "call_subagent";
}

function firstNonEmptyString(...values) {
  for (const value of values) {
    if (typeof value === "string" && value.trim()) return value.trim();
  }
  return null;
}

/**
 * Normalizes the dispatch payload into the per-subagent descriptors the host was asked to spawn.
 * Both the `Subagents` array shape and the flat `{ role, prompt }` shape are supported.
 */
function subagentItemsFromCall(call) {
  let argsObj = call?.args;
  if (typeof argsObj === "string") {
    try {
      argsObj = JSON.parse(argsObj);
    } catch {
      argsObj = null;
    }
  }
  const argsObject = argsObj && typeof argsObj === "object" ? argsObj : {};
  const rawItems = Array.isArray(argsObject.Subagents) && argsObject.Subagents.length > 0
    ? argsObject.Subagents
    : (argsObject.Prompt || argsObject.prompt || argsObject.task || argsObject.Role || argsObject.role ? [argsObject] : []);
  const items = [];
  for (const raw of rawItems) {
    if (!raw || typeof raw !== "object") continue;
    items.push({
      role: firstNonEmptyString(raw.Role, raw.role, argsObject.Role, argsObject.role),
      typeName: firstNonEmptyString(raw.TypeName, raw.typeName, argsObject.TypeName, argsObject.typeName),
      prompt: firstNonEmptyString(raw.Prompt, raw.prompt, raw.task, argsObject.Prompt, argsObject.prompt, argsObject.task),
      model: firstNonEmptyString(raw.Model, raw.model),
    });
  }
  return items;
}

/**
 * 派发参数里没有可读提示词时的占位文本；这里只负责告警与兜底，
 * 不再假装"解析"入参，命名需如实反映行为。
 */
function subagentPromptPlaceholder(item) {
  emitWarning(
    `Antigravity subagent ${item?.role ?? item?.typeName ?? "unknown"} has no dispatch prompt; using placeholder text`,
  );
  return SUBAGENT_PROMPT_FALLBACK;
}

function readSubagentRegistry(conversationDir) {
  if (!conversationDir) return [];
  const registryDir = path.join(conversationDir, SUBAGENT_REGISTRY_DIR);
  if (!existsSync(registryDir)) return [];
  let names = [];
  try {
    names = readdirSync(registryDir).filter((name) => name.endsWith(".json")).sort();
  } catch {
    return [];
  }
  const entries = [];
  for (const name of names) {
    let raw = null;
    try {
      raw = JSON.parse(readFileSync(path.join(registryDir, name), "utf8"));
    } catch {
      continue;
    }
    const conversationId = firstNonEmptyString(raw?.conversationId);
    if (!conversationId) continue;
    const descriptor = raw?.subagentDescriptor && typeof raw.subagentDescriptor === "object"
      ? raw.subagentDescriptor
      : {};
    entries.push({
      conversationId,
      role: firstNonEmptyString(descriptor.role),
      typeName: firstNonEmptyString(descriptor.typeName),
      spawnStepIndex: Number.isInteger(raw?.spawnStepIndex) ? raw.spawnStepIndex : null,
    });
  }
  entries.sort(
    (a, b) =>
      (a.spawnStepIndex ?? Number.MAX_SAFE_INTEGER) - (b.spawnStepIndex ?? Number.MAX_SAFE_INTEGER)
      || a.conversationId.localeCompare(b.conversationId),
  );
  return entries;
}

function readTranscriptSteps(transcriptPath) {
  const steps = [];
  let text = "";
  try {
    text = readFileSync(transcriptPath, "utf8");
  } catch {
    return steps;
  }
  for (const line of text.split(/\r?\n/)) {
    if (!line.trim()) continue;
    try {
      steps.push(JSON.parse(line));
    } catch {
      continue;
    }
  }
  return steps;
}

function isInjectedPromptText(text) {
  const trimmed = String(text ?? "").trim();
  if (!trimmed) return true;
  return INJECTED_PROMPT_BLOCK.test(trimmed);
}

function firstRealUserInputFromSteps(steps) {
  for (const step of steps) {
    if (!step || step.type !== "USER_INPUT" || step.source !== "USER_EXPLICIT") continue;
    const text = extractUserRequestText(step.content);
    if (!text || isInjectedPromptText(text)) continue;
    return text;
  }
  return null;
}

function resolveSubagentModel(value, inheritedModel) {
  const model = firstNonEmptyString(value);
  if (!model || /^inherit$/i.test(model)) return inheritedModel ?? null;
  return model;
}

function resolveDispatchForStep(stepIndex, dispatches) {
  if (!Array.isArray(dispatches) || dispatches.length === 0) return null;
  if (!Number.isInteger(stepIndex)) return dispatches.length === 1 ? dispatches[0] : null;
  const exact = dispatches.find((dispatch) => dispatch.stepIndex === stepIndex);
  if (exact) return exact;
  let best = null;
  let bestAnchor = -Infinity;
  for (const dispatch of dispatches) {
    const anchor = Number.isInteger(dispatch.stepIndex) ? dispatch.stepIndex : dispatch.plannerStepIndex;
    if (!Number.isInteger(anchor) || anchor > stepIndex || anchor < bestAnchor) continue;
    best = dispatch;
    bestAnchor = anchor;
  }
  return best;
}

function buildSubagentConversationContent({ conversationDir, transcriptPath, dispatchPrompt }) {
  const steps = readTranscriptSteps(transcriptPath);
  const entries = readSubagentRegistry(conversationDir);
  const resolvedPrompt = firstNonEmptyString(dispatchPrompt) ?? firstRealUserInputFromSteps(steps);
  if (!resolvedPrompt) {
    emitWarning(
      `Antigravity subagent ${path.basename(conversationDir)} has no task prompt; using placeholder text`,
    );
  }
  const userText = resolvedPrompt ?? SUBAGENT_PROMPT_FALLBACK;
  let parts = [];
  let dispatches = [];
  if (steps.length > 0) {
    const parsed = buildTurnsFromSteps(
      [
        {
          source: "USER_EXPLICIT",
          type: "USER_INPUT",
          content: `<USER_REQUEST>${userText}</USER_REQUEST>`,
          created_at: steps[0]?.created_at ?? null,
        },
        ...steps,
      ],
      { registryMode: entries.length > 0 },
    );
    parts = parsed.turns.flatMap((turn) => turn.parts);
    dispatches = parsed.dispatches;
  }
  return {
    userText,
    parts,
    dispatches,
    entries,
    startedAt: steps[0]?.created_at ?? null,
    endedAt: steps.at(-1)?.created_at ?? null,
  };
}

function takeMatchingSubagentItem(dispatch, usedItems, entry) {
  if (!dispatch) return null;
  const items = Array.isArray(dispatch.subagents) ? dispatch.subagents : [];
  const used = usedItems.get(dispatch) ?? new Set();
  usedItems.set(dispatch, used);
  let index = entry.role
    ? items.findIndex((item, candidate) => !used.has(candidate) && item.role === entry.role)
    : -1;
  if (index < 0 && entry.typeName) {
    index = items.findIndex((item, candidate) => !used.has(candidate) && item.typeName === entry.typeName);
  }
  if (index < 0) index = items.findIndex((_, candidate) => !used.has(candidate));
  if (index < 0) return null;
  used.add(index);
  return items[index];
}

/**
 * Turns the registry into one subagent turn per registered subagent, recursively.
 * Parent pointers come from the dispatching step index, so a subagent always hangs
 * off the turn that actually dispatched it.
 */
function attachSubagentTurns({ conversationDir, registry, turns, dispatches }) {
  const brainDir = path.dirname(conversationDir);
  const visitedConversationIds = new Set([path.basename(conversationDir)]);
  const childrenByTurnId = new Map();
  const ordinalByTurnId = new Map();
  const usedItems = new Map();

  const expand = ({ entries, conversationDispatches, depth, ownerTurnId, ownerModel }) => {
    const ownerDispatch = ownerTurnId
      ? { parentTurnId: ownerTurnId, subagents: conversationDispatches.flatMap((dispatch) => dispatch.subagents ?? []) }
      : null;
    for (const entry of entries) {
      const dispatch = ownerDispatch ?? resolveDispatchForStep(entry.spawnStepIndex, conversationDispatches);
      const item = takeMatchingSubagentItem(dispatch, usedItems, entry);
      const parentTurnId = ownerTurnId ?? dispatch?.parentTurnId ?? null;
      if (!parentTurnId) {
        emitWarning(`Antigravity subagent ${entry.conversationId} has no dispatching turn; skipped`);
        continue;
      }

      const ordinal = (ordinalByTurnId.get(parentTurnId) ?? 0) + 1;
      ordinalByTurnId.set(parentTurnId, ordinal);
      const subagentTurn = {
        external_id: `${parentTurnId}-subagent-${ordinal}-${entry.conversationId}`,
        turn_index: 0,
        user_text: item?.prompt ?? SUBAGENT_PROMPT_FALLBACK,
        turn_kind: "subagent",
        agent_role: entry.role ?? item?.role ?? undefined,
        parent_turn_id: parentTurnId,
        title: entry.typeName ?? item?.typeName ?? null,
        started_at: null,
        ended_at: null,
        model: resolveSubagentModel(item?.model, ownerModel),
        parts: [],
      };
      const siblings = childrenByTurnId.get(parentTurnId);
      if (siblings) siblings.push(subagentTurn);
      else childrenByTurnId.set(parentTurnId, [subagentTurn]);

      const childDepth = depth + 1;
      if (childDepth > MAX_SUBAGENT_DEPTH) {
        emitWarning(
          `Antigravity subagent ${entry.conversationId} exceeds the depth limit of ${MAX_SUBAGENT_DEPTH}; kept as a leaf turn`,
        );
        continue;
      }
      if (visitedConversationIds.has(entry.conversationId)) {
        emitWarning(
          `Antigravity subagent ${entry.conversationId} is already expanded (cycle); kept as a leaf turn`,
        );
        continue;
      }
      const childConversationDir = path.join(brainDir, entry.conversationId);
      const childTranscriptPath = findTranscriptFile(childConversationDir);
      if (!childTranscriptPath) {
        emitWarning(
          `Antigravity subagent ${entry.conversationId} has no child session record; kept the dispatch turn only`,
        );
        continue;
      }

      visitedConversationIds.add(entry.conversationId);
      const content = buildSubagentConversationContent({
        conversationDir: childConversationDir,
        transcriptPath: childTranscriptPath,
        dispatchPrompt: item?.prompt ?? null,
      });
      subagentTurn.user_text = content.userText;
      subagentTurn.parts = content.parts;
      subagentTurn.started_at = content.startedAt;
      subagentTurn.ended_at = content.endedAt ?? content.startedAt;
      subagentTurn.model ??= ownerModel ?? null;
      expand({
        entries: content.entries,
        conversationDispatches: content.dispatches,
        depth: childDepth,
        ownerTurnId: subagentTurn.external_id,
        ownerModel: subagentTurn.model,
      });
    }
  };

  expand({ entries: registry, conversationDispatches: dispatches, depth: 0, ownerTurnId: null, ownerModel: null });

  const ordered = [];
  const appendWithChildren = (turn) => {
    ordered.push(turn);
    for (const child of childrenByTurnId.get(turn.external_id) ?? []) appendWithChildren(child);
  };
  for (const turn of turns) appendWithChildren(turn);
  return ordered;
}

function buildSessionTurnsFromSteps(steps, conversationDir) {
  const registry = readSubagentRegistry(conversationDir);
  const parsed = buildTurnsFromSteps(steps, { registryMode: registry.length > 0 });
  if (registry.length === 0 || parsed.turns.length === 0) return parsed;
  return {
    ...parsed,
    turns: attachSubagentTurns({
      conversationDir,
      registry,
      turns: parsed.turns,
      dispatches: parsed.dispatches,
    }),
  };
}

function conversationDirForTranscript(transcriptPath) {
  const logsDir = path.dirname(transcriptPath);
  const systemDir = path.dirname(logsDir);
  if (path.basename(systemDir) === ".system_generated") return path.dirname(systemDir);
  return logsDir;
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

function buildTurnsFromSteps(steps, options = {}) {
  const registryMode = Boolean(options.registryMode);
  const turns = [];
  const dispatches = [];
  let current = null;
  let activeSubagent = null;
  let anchorTurnId = null;
  let subagentCount = 0;
  let dispatchCount = 0;
  let continuationCount = 0;
  let projectPath = null;
  let currentModel = null;
  const pendingFileOperations = [];
  const pendingToolCalls = [];
  const knownFileContents = new Map();

  for (const step of steps) {
    if (!step) continue;
    const source = step.source ?? "";
    const type = step.type ?? "";
    const timestamp = step.created_at ?? null;

    if (typeof step.content === "string") {
      const match = step.content.match(/The user changed setting [`']?Model Selection[`']? from .+? to (.+?)(?:\.\s+[A-Z]|\n|$)/i);
      if (match) {
        currentModel = match[1].replace(/\.+$/, "").trim();
        if (current && !current.model) current.model = currentModel;
        if (activeSubagent && !activeSubagent.model) activeSubagent.model = currentModel;
      }
    }
    if (step.model) {
      currentModel = String(step.model);
      if (current && !current.model) current.model = currentModel;
      if (activeSubagent && !activeSubagent.model) activeSubagent.model = currentModel;
    }

    // Skip system-only noise
    if (source === "SYSTEM" && IGNORED_STEP_TYPES.has(type)) continue;

    // User input → new turn boundary
    if (type === "USER_INPUT" && source === "USER_EXPLICIT") {
      const userText = extractUserRequestText(step.content);
      if (!userText) continue;
      if (activeSubagent) {
        turns.push(activeSubagent);
        activeSubagent = null;
      }
      if (current) turns.push(current);
      pendingFileOperations.length = 0;
      pendingToolCalls.length = 0;
      anchorTurnId = `turn-${turns.length}`;
      subagentCount = 0;
      continuationCount = 0;
      current = {
        external_id: anchorTurnId,
        turn_index: turns.length,
        user_text: userText,
        title: null,
        started_at: timestamp,
        ended_at: null,
        model: currentModel,
        parts: [],
      };
      continue;
    }

    if (!current && !activeSubagent && turns.length === 0) continue;

    // PLANNER_RESPONSE → assistant text + tool calls
    if (type === "PLANNER_RESPONSE" && source === "MODEL") {
      const responseText = String(step.content ?? "").trim();
      const subagentCalls = Array.isArray(step.tool_calls) ? step.tool_calls.filter(isSubagentCall) : [];

      if (subagentCalls.length > 0) {
        if (current) {
          if (responseText) {
            const part = textPart("assistant", responseText);
            if (part) current.parts.push(part);
          }
          current.ended_at = timestamp;
          turns.push(current);
          anchorTurnId = current.external_id;
          current = null;
        } else if (activeSubagent) {
          turns.push(activeSubagent);
          activeSubagent = null;
        }

        const parentAnchorTurnId = anchorTurnId || (turns.at(-1)?.external_id ?? turns[0]?.external_id ?? "turn-0");

        if (registryMode) {
          // The registry owns subagent turn creation; only remember which step dispatched what.
          const dispatch = {
            parentTurnId: parentAnchorTurnId,
            plannerStepIndex: Number.isInteger(step.step_index) ? step.step_index : null,
            stepIndex: null,
            subagents: subagentCalls.flatMap((call) => subagentItemsFromCall(call)),
          };
          dispatches.push(dispatch);
          for (const call of subagentCalls) {
            pendingToolCalls.push({
              executionId: toolCallExecutionId(call, parentAnchorTurnId, ++dispatchCount),
              name: call.name,
              args: call.args,
              isSubagentDispatch: true,
              dispatch,
            });
          }
          continue;
        }

        for (const call of subagentCalls) {
          const execId = toolCallExecutionId(call, parentAnchorTurnId, ++dispatchCount);
          call.execution_id = execId;
          pendingToolCalls.push({ executionId: execId, name: call.name, args: call.args, isSubagentDispatch: true });
        }

        const subagentItems = subagentCalls.flatMap((call) => subagentItemsFromCall(call));
        for (let sIdx = 0; sIdx < subagentItems.length; sIdx++) {
          const item = subagentItems[sIdx];
          const prompt = item.prompt ?? subagentPromptPlaceholder(item);
          const subTurn = {
            external_id: `${parentAnchorTurnId}-subagent-${++subagentCount}`,
            turn_index: turns.length,
            user_text: prompt,
            turn_kind: "subagent",
            agent_role: item.role ?? undefined,
            parent_turn_id: parentAnchorTurnId,
            title: item.typeName ?? null,
            started_at: timestamp,
            ended_at: sIdx < subagentItems.length - 1 ? timestamp : null,
            model: resolveSubagentModel(item.model, currentModel),
            parts: [],
          };

          if (sIdx < subagentItems.length - 1) {
            turns.push(subTurn);
          } else {
            activeSubagent = subTurn;
          }
        }

        const otherCalls = step.tool_calls.filter((c) => !isSubagentCall(c));
        if (otherCalls.length > 0 && activeSubagent) {
          pendingFileOperations.push(
            ...otherCalls.map((call) => fileOperationFromToolCall(call, knownFileContents)).filter(Boolean),
          );
          for (const call of otherCalls) {
            if (!call?.name) continue;
            activeSubagent.toolCallCount = (activeSubagent.toolCallCount || 0) + 1;
            const cExecId = toolCallExecutionId(call, activeSubagent.external_id, activeSubagent.toolCallCount);
            call.execution_id = cExecId;
            pendingToolCalls.push({ executionId: cExecId, name: call.name, args: call.args });
          }
          activeSubagent.parts.push(...toolCallParts(otherCalls, activeSubagent.external_id));
          projectPath ??= inferProjectPathFromToolCalls(otherCalls);
        }
        continue;
      }

      const parentAnchorTurnId = anchorTurnId || (turns[0]?.external_id ?? "turn-0");
      if (!current && !activeSubagent) {
        current = {
          external_id: `${parentAnchorTurnId}-cont-${++continuationCount}`,
          turn_index: turns.length,
          user_text: "Continue following subagent execution",
          turn_kind: "continuation",
          parent_turn_id: parentAnchorTurnId,
          title: null,
          started_at: timestamp,
          ended_at: null,
          model: currentModel,
          parts: [],
        };
      }

      const targetTurn = activeSubagent || current;
      if (responseText && targetTurn) {
        const part = textPart("assistant", responseText);
        if (part) targetTurn.parts.push(part);
      }
      if (Array.isArray(step.tool_calls) && step.tool_calls.length > 0 && targetTurn) {
        pendingFileOperations.push(
          ...step.tool_calls.map((call) => fileOperationFromToolCall(call, knownFileContents)).filter(Boolean),
        );
        for (const call of step.tool_calls) {
          if (!call?.name) continue;
          targetTurn.toolCallCount = (targetTurn.toolCallCount || 0) + 1;
          const execId = toolCallExecutionId(call, targetTurn.external_id, targetTurn.toolCallCount);
          call.execution_id = execId;
          pendingToolCalls.push({ executionId: execId, name: call.name, args: call.args });
        }
        targetTurn.parts.push(...toolCallParts(step.tool_calls, targetTurn.external_id));
        projectPath ??= inferProjectPathFromToolCalls(step.tool_calls);
      }
      if (targetTurn) targetTurn.ended_at = timestamp;
      continue;
    }

    // Tool results (RUN_COMMAND, CODE_ACTION, VIEW_FILE, GREP_SEARCH, LIST_DIRECTORY, ERROR_MESSAGE, GENERIC)
    if (source === "MODEL" || source === "SYSTEM") {
      const resultTypes = [
        "RUN_COMMAND", "CODE_ACTION", "VIEW_FILE",
        "GREP_SEARCH", "LIST_DIRECTORY", "ERROR_MESSAGE", "GENERIC",
      ];
      if (resultTypes.includes(type)) {
        let matchedIndex = -1;
        if (type === "RUN_COMMAND" || (type === "GENERIC" && /(?:The command|Process)\s+exited\s+with\s+code/i.test(step.content))) {
          matchedIndex = pendingToolCalls.findIndex((c) => c.name === "run_command");
        } else if (type === "VIEW_FILE" || (type === "GENERIC" && /File Path:\s*`/i.test(step.content))) {
          matchedIndex = pendingToolCalls.findIndex((c) => /view_file|read_file|read_url/i.test(c.name));
        } else if (type === "GREP_SEARCH" || (type === "GENERIC" && /grep|search|find/i.test(step.content))) {
          matchedIndex = pendingToolCalls.findIndex((c) => /grep|search|find/i.test(c.name));
        } else if (type === "LIST_DIRECTORY") {
          matchedIndex = pendingToolCalls.findIndex((c) => /list_dir|list_directory/i.test(c.name));
        } else if (type === "CODE_ACTION") {
          matchedIndex = pendingToolCalls.findIndex((c) => /write|replace|edit|patch/i.test(c.name));
        }
        if (matchedIndex < 0 && pendingToolCalls.length > 0) {
          matchedIndex = 0;
        }
        const matchedCall = matchedIndex >= 0 ? pendingToolCalls.splice(matchedIndex, 1)[0] : null;
        const sourceExecutionId = matchedCall?.executionId ?? null;

        if (matchedCall?.isSubagentDispatch || (matchedCall && ["invoke_subagent", "call_subagent"].includes(matchedCall.name))) {
          // The dispatch acknowledgement step is the anchor the host registry points at (`spawnStepIndex`).
          if (matchedCall?.dispatch && Number.isInteger(step.step_index)) {
            matchedCall.dispatch.stepIndex = step.step_index;
          }
          if (activeSubagent) {
            const reportText = typeof step.content === "string" ? step.content.trim() : JSON.stringify(step.content);
            if (reportText) {
              const part = textPart("assistant", reportText);
              if (part) activeSubagent.parts.push(part);
            }
            activeSubagent.ended_at = timestamp;
            turns.push(activeSubagent);
            activeSubagent = null;
          }
          continue;
        }

        const isCodeAction = type === "CODE_ACTION" || (matchedCall && /write|replace|edit|patch/i.test(matchedCall.name));
        const fileOperation = isCodeAction
          ? (matchPendingFileOperation(pendingFileOperations, step.content) || operationFromMatchedCall(matchedCall, knownFileContents))
          : null;

        const targetTurn = activeSubagent || current;
        if (targetTurn) {
          targetTurn.parts.push(...toolResultPart(step, fileOperation, sourceExecutionId, matchedCall));
          targetTurn.ended_at = timestamp;
        }
      }
    }
  }
  if (activeSubagent) turns.push(activeSubagent);
  if (current) turns.push(current);
  for (let i = 0; i < turns.length; i++) {
    turns[i].turn_index = i;
  }
  if (projectPath) {
    for (const turn of turns) {
      for (const part of turn.parts) {
        if (part.kind === "command" && !part.cwd) {
          part.cwd = projectPath;
        }
      }
    }
  }
  return { turns, projectPath, dispatches };
}

// 内部运行时目录使用白名单片段，而不是整个 `~/.assetiweave/` 前缀：
// 用户的正常会话只要读写 memory/library 就会落在 `~/.assetiweave/` 下，
// 用前缀判定会把「用户会话」误判成「应用自己的内部执行」并整条丢弃。
const INTERNAL_RUNTIME_DIR_SEGMENTS = [
  "/agent-executions/",
  "/agent-runs/",
  "/agent-runtimes/",
  "/smoke_workspace/",
  "/.system_generated/",
  "/test-scratch/",
];

function isInternalExecutionPath(cwd) {
  if (!cwd || typeof cwd !== "string") return false;
  const norm = cwd.replace(/\\/g, "/").toLowerCase();
  return (
    INTERNAL_RUNTIME_DIR_SEGMENTS.some((segment) => norm.includes(segment)) ||
    norm.includes("/mock_")
  );
}

// 应用自身的私有数据目录：可以出现在会话里（读 memory、写 library），
// 但不能被当成会话的 projectPath，否则会污染项目归属与内部执行判定。
const APP_DATA_DIR_SEGMENTS = [
  "/.assetiweave/",
  "/library/application support/assetiweave/",
  "/.gemini/",
  "/.codebuddy/",
  "/.claude/",
  "/.codex/",
];

function isAppDataPath(candidate) {
  if (!candidate || typeof candidate !== "string") return false;
  const norm = candidate.replace(/\\/g, "/").toLowerCase();
  const trimmed = norm.replace(/\/+$/, "");
  return (
    APP_DATA_DIR_SEGMENTS.some((segment) => norm.includes(segment)) ||
    trimmed.endsWith("/.assetiweave")
  );
}

function isInternalSessionMeta(metaPath) {
  if (!metaPath || !existsSync(metaPath)) return false;
  try {
    const raw = readFileSync(metaPath, "utf8");
    if (
      raw.includes("agent-executions") ||
      raw.includes("agent-runs") ||
      raw.includes("agent-runtimes") ||
      raw.includes("smoke_workspace") ||
      raw.includes(".system_generated") ||
      raw.includes("test-scratch") ||
      raw.includes('"is_subagent": true') ||
      raw.includes('"is_subagent":true') ||
      raw.includes('"parent_session_id"') ||
      raw.includes('"parent_thread_id"') ||
      raw.includes('"subagent": true') ||
      raw.includes('"subagent":true')
    ) {
      return true;
    }
    const meta = JSON.parse(raw);
    if (isInternalExecutionPath(meta.cwd)) return true;
    if (
      meta.is_subagent ||
      meta.subagent ||
      meta.parent_session_id ||
      meta.parent_id ||
      meta.parent_thread_id ||
      meta.execution_origin === "subagent" ||
      meta.role === "subagent"
    ) {
      return true;
    }
  } catch {}
  return false;
}

function isSyntheticWorkOrderText(text) {
  if (!text || typeof text !== "string") return false;
  const trimmed = text.trim();
  if (
    trimmed.startsWith("{") &&
    (trimmed.includes('"work_order_id"') ||
      trimmed.includes('"contract_version"') ||
      trimmed.includes('"output_schema"') ||
      trimmed.includes("memory.contract"))
  ) {
    return true;
  }
  if (trimmed.includes("Consolidate the successful Session Memory")) return true;
  if (trimmed.includes("ASSETIWEAVE_ACP_OK")) return true;
  return false;
}

function hasValidConversationSteps(dbPath) {
  if (!existsSync(dbPath)) return false;
  try {
    const stat = statSync(dbPath);
    if (stat.size === 0) return false;
  } catch {
    return false;
  }

  if (DatabaseSync) {
    let db;
    try {
      db = new DatabaseSync(dbPath, { readOnly: true });
      const stmt = db.prepare("SELECT count(*) as count FROM steps;");
      const row = stmt.get();
      return (row?.count ?? 0) > 0;
    } catch {
      return false;
    } finally {
      if (db) db.close();
    }
  } else {
    try {
      const raw = execFileSync("sqlite3", [dbPath, "SELECT count(*) FROM steps;"], {
        encoding: "utf8",
        timeout: 500,
      });
      return parseInt(raw.trim(), 10) > 0;
    } catch {
      return false;
    }
  }
}

function parseTranscript(text, conversationDir = null) {
  const steps = [];
  for (const line of text.split(/\r?\n/)) {
    if (!line.trim()) continue;
    try {
      steps.push(JSON.parse(line));
    } catch {
      continue;
    }
  }
  const result = conversationDir
    ? buildSessionTurnsFromSteps(steps, conversationDir)
    : buildTurnsFromSteps(steps);
  if (result.turns.length > 0 && isSyntheticWorkOrderText(result.turns[0]?.user_text)) {
    return { turns: [], projectPath: null };
  }
  if (isInternalExecutionPath(result.projectPath)) {
    return { turns: [], projectPath: null };
  }
  return result;
}

function decodeProto(buf) {
  if (!buf || !Buffer.isBuffer(buf)) return {};
  let pos = 0;
  const fields = {};
  while (pos < buf.length) {
    let key = 0, shift = 0;
    while (pos < buf.length) {
      const b = buf[pos++];
      key |= (b & 0x7f) << shift;
      if (!(b & 0x80)) break;
      shift += 7;
    }
    const fieldNum = key >> 3;
    const wireType = key & 0x07;
    if (wireType === 0) {
      let val = 0n, vShift = 0n;
      while (pos < buf.length) {
        const b = BigInt(buf[pos++]);
        val |= (b & 0x7fn) << vShift;
        if (!(b & 0x80n)) break;
        vShift += 7n;
      }
      fields[fieldNum] = fields[fieldNum] || [];
      fields[fieldNum].push({ type: "varint", val });
    } else if (wireType === 2) {
      let len = 0, lShift = 0;
      while (pos < buf.length) {
        const b = buf[pos++];
        len |= (b & 0x7f) << lShift;
        if (!(b & 0x80)) break;
        lShift += 7;
      }
      const slice = buf.subarray(pos, pos + len);
      pos += len;
      fields[fieldNum] = fields[fieldNum] || [];
      fields[fieldNum].push({ type: "bytes", len, buf: slice });
    } else if (wireType === 1) {
      pos += 8;
    } else if (wireType === 5) {
      pos += 4;
    } else {
      break;
    }
  }
  return fields;
}

function getProtoTimestamp(top) {
  const f5 = top[5]?.[0];
  if (!f5) return null;
  const sub5 = decodeProto(f5.buf);
  const f1 = sub5[1]?.[0];
  if (!f1) return null;
  const sub1 = decodeProto(f1.buf);
  const sec = sub1[1]?.[0]?.val;
  const nanos = sub1[2]?.[0]?.val || 0n;
  if (sec != null) {
    return new Date(Number(sec) * 1000 + Math.round(Number(nanos) / 1e6)).toISOString();
  }
  return null;
}

function parseSqliteSteps(dbPath) {
  emitProgress({
    stage: "scan",
    operation: "query_sqlite_steps",
    path: `${path.basename(dbPath)}: SELECT idx, step_type, hex(step_payload) FROM steps`,
  });
  let rows = [];
  if (DatabaseSync) {
    let db;
    try {
      db = new DatabaseSync(dbPath, { readOnly: true });
      const stmt = db.prepare("SELECT idx, step_type, hex(step_payload) as hex_payload FROM steps ORDER BY idx ASC;");
      rows = stmt.all();
    } catch {
      rows = [];
    } finally {
      if (db) db.close();
    }
  } else {
    try {
      const raw = execFileSync("sqlite3", ["-json", dbPath, "SELECT idx, step_type, hex(step_payload) as hex_payload FROM steps ORDER BY idx ASC;"], {
        encoding: "utf8",
        maxBuffer: 100 * 1024 * 1024,
      });
      rows = JSON.parse(raw || "[]");
    } catch {
      return [];
    }
  }
  const steps = [];
  for (const r of rows) {
    if (!r.hex_payload) continue;
    const buf = Buffer.from(r.hex_payload, "hex");
    const top = decodeProto(buf);
    const ts = getProtoTimestamp(top);

    if (r.step_type === 14) {
      const f19 = top[19]?.[0];
      let userText = "";
      if (f19) {
        const sub19 = decodeProto(f19.buf);
        userText = sub19[2]?.[0]?.buf?.toString("utf8") || sub19[3]?.[0]?.buf?.toString("utf8") || "";
      }
      if (userText) {
        steps.push({
          source: "USER_EXPLICIT",
          type: "USER_INPUT",
          content: userText,
          created_at: ts,
        });
      }
    } else if (r.step_type === 15) {
      const f20 = top[20]?.[0];
      if (!f20) continue;
      const sub20 = decodeProto(f20.buf);
      const textBuf = sub20[1]?.[0]?.buf || sub20[8]?.[0]?.buf;
      const responseText = textBuf ? textBuf.toString("utf8") : "";
      const toolCalls = [];
      const tcList = sub20[7] || [];
      for (const tc of tcList) {
        const stc = decodeProto(tc.buf);
        const callId = stc[1]?.[0]?.buf?.toString("utf8") || "";
        const toolName = stc[2]?.[0]?.buf?.toString("utf8") || stc[9]?.[0]?.buf?.toString("utf8") || "";
        const argsRaw = stc[3]?.[0]?.buf?.toString("utf8") || "{}";
        let parsedArgs = {};
        try { parsedArgs = JSON.parse(argsRaw); } catch {}
        toolCalls.push({
          id: callId,
          name: toolName,
          args: parsedArgs,
          type: "function",
          function: { name: toolName, arguments: argsRaw },
        });
      }
      if (responseText || toolCalls.length > 0) {
        steps.push({
          source: "MODEL",
          type: "PLANNER_RESPONSE",
          content: responseText,
          tool_calls: toolCalls,
          created_at: ts,
        });
      }
    } else if (r.step_type === 21) {
      const f28 = top[28]?.[0];
      let output = "";
      if (f28) {
        const sub28 = decodeProto(f28.buf);
        output = sub28[21]?.[0]?.buf?.toString("utf8") || "";
      }
      steps.push({
        source: "MODEL",
        type: "RUN_COMMAND",
        content: output,
        status: "DONE",
        created_at: ts,
      });
    } else if (r.step_type === 103) {
      const f116 = top[116]?.[0];
      let toolName = "";
      let resText = "";
      if (f116) {
        const sub116 = decodeProto(f116.buf);
        toolName = sub116[2]?.[0]?.buf?.toString("utf8") || "";
        const resRaw = sub116[4]?.[0];
        if (resRaw) {
          const subRes = decodeProto(resRaw.buf);
          const resSub2 = subRes[2]?.[0];
          if (resSub2) {
            const subResSub2 = decodeProto(resSub2.buf);
            resText = subResSub2[2]?.[0]?.buf?.toString("utf8") || "";
          }
        }
      }
      let type = "GENERIC";
      if (toolName === "view_file") type = "VIEW_FILE";
      else if (toolName === "grep_search") type = "GREP_SEARCH";
      else if (toolName === "list_dir") type = "LIST_DIRECTORY";
      else if (toolName.includes("file") || toolName.includes("patch") || toolName.includes("edit") || toolName.includes("write")) type = "CODE_ACTION";
      steps.push({
        source: "MODEL",
        type,
        content: resText,
        status: "DONE",
        created_at: ts,
      });
    } else if (r.step_type === 7) {
      const sub13 = decodeProto(top[13]?.[0]?.buf);
      const matches = sub13[4] || [];
      const lines = [];
      for (const m of matches) {
        const sm = decodeProto(m.buf);
        const file = sm[1]?.[0]?.buf?.toString("utf8") || "";
        const lineNo = sm[2]?.[0]?.val != null ? String(sm[2][0].val) : "";
        const content = sm[3]?.[0]?.buf?.toString("utf8") || "";
        if (file || content) lines.push(`${file}:${lineNo}:${content}`);
      }
      steps.push({
        source: "MODEL",
        type: "GREP_SEARCH",
        content: lines.join("\n"),
        status: "DONE",
        created_at: ts,
      });
    } else if (r.step_type === 17) {
      const errorStr = buf.toString("utf8");
      steps.push({
        source: "MODEL",
        type: "ERROR_MESSAGE",
        content: errorStr,
        status: "ERROR",
        created_at: ts,
      });
    }
  }
  return steps;
}

function parseSqliteDb(dbPath) {
  const metaPath = dbPath.replace(/\.db$/, ".meta");
  if (isInternalSessionMeta(metaPath)) {
    return { turns: [], projectPath: null };
  }
  let metaProjectPath = null;
  try {
    if (existsSync(metaPath)) {
      const meta = JSON.parse(readFileSync(metaPath, "utf8"));
      if (meta.cwd && typeof meta.cwd === "string") {
        if (isInternalExecutionPath(meta.cwd)) {
          return { turns: [], projectPath: null };
        }
        metaProjectPath = meta.cwd;
      }
    }
  } catch {}
  const steps = parseSqliteSteps(dbPath);
  const { turns, projectPath } = buildTurnsFromSteps(steps);
  if (turns.length > 0 && isSyntheticWorkOrderText(turns[0]?.user_text)) {
    return { turns: [], projectPath: null };
  }
  const finalProjectPath = metaProjectPath || projectPath;
  if (isInternalExecutionPath(finalProjectPath)) {
    return { turns: [], projectPath: null };
  }
  return { turns, projectPath: finalProjectPath };
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
  const directFull = path.join(conversationDir, "transcript_full.jsonl");
  if (existsSync(directFull)) return directFull;
  const directShort = path.join(conversationDir, "transcript.jsonl");
  if (existsSync(directShort)) return directShort;
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

function detectClientFlavor(targetPath) {
  if (!targetPath) return "antigravity";
  const norm = targetPath.toLowerCase();
  if (norm.includes("antigravity-acp")) return "antigravity-acp";
  if (norm.includes("antigravity-cli")) return "antigravity-cli";
  if (norm.includes("antigravity-ide")) return "antigravity-ide";
  if (norm.includes("antigravity")) return "antigravity";
  return "antigravity-custom";
}

function resolveCandidateRoots(startLocation) {
  const candidateRoots = new Set();
  const geminiDir = path.join(homedir(), ".gemini");
  const standardFlavors = [
    "antigravity",
    "antigravity-cli",
    "antigravity-ide",
    "antigravity-acp",
  ];

  if (startLocation) {
    const resolved = path.resolve(expandPath(startLocation));
    if (existsSync(resolved)) {
      let baseDir = resolved;
      try {
        const stat = statSync(resolved);
        if (stat.isFile()) {
          baseDir = path.dirname(resolved);
        }
      } catch {}

      if (path.basename(baseDir) === "brain" || path.basename(baseDir) === "conversations") {
        baseDir = path.dirname(baseDir);
      }
      candidateRoots.add(baseDir);

      const parent = path.dirname(baseDir);
      for (const f of standardFlavors) {
        const full = path.join(parent, f);
        if (existsSync(full)) candidateRoots.add(full);
      }
      const grandParent = path.dirname(parent);
      for (const f of standardFlavors) {
        const full = path.join(grandParent, f);
        if (existsSync(full)) candidateRoots.add(full);
      }
    }
  } else {
    for (const f of standardFlavors) {
      const full = path.join(geminiDir, f);
      if (existsSync(full)) {
        candidateRoots.add(full);
      }
    }
  }

  return Array.from(candidateRoots);
}

function discoverBrainDirs(startLocation) {
  const roots = resolveCandidateRoots(startLocation);
  const validBrainDirs = [];
  const seen = new Set();

  for (const root of roots) {
    const directBrain = path.join(root, "brain");
    const target = existsSync(directBrain) ? directBrain : root;
    if (!existsSync(target)) continue;
    try {
      const entries = readdirSync(target, { withFileTypes: true });
      if (entries.some((e) => e.isDirectory() && isUuidDir(e.name))) {
        const p = path.resolve(target);
        if (!seen.has(p)) {
          seen.add(p);
          validBrainDirs.push(target);
        }
      }
    } catch {}
  }

  if (validBrainDirs.length === 0 && startLocation) {
    const resolved = path.resolve(expandPath(startLocation));
    if (existsSync(resolved) && statSync(resolved).isDirectory()) {
      return [resolved];
    }
  }
  return validBrainDirs;
}

function discoverConversationDbFiles(startLocation) {
  if (startLocation) {
    const resolved = path.resolve(expandPath(startLocation));
    if (existsSync(resolved) && statSync(resolved).isFile() && resolved.endsWith(".db")) {
      const metaP = resolved.replace(/\.db$/, ".meta");
      if (isInternalSessionMeta(metaP) || !hasValidConversationSteps(resolved)) {
        return [];
      }
      return [resolved];
    }
  }
  const roots = resolveCandidateRoots(startLocation);
  const dbFiles = new Set();

  for (const root of roots) {
    const convDir = path.join(root, "conversations");
    const targetDir = existsSync(convDir) ? convDir : root;
    if (!existsSync(targetDir)) continue;
    try {
      const entries = readdirSync(targetDir, { withFileTypes: true });
      for (const entry of entries) {
        if (entry.isFile() && entry.name.endsWith(".db")) {
          const baseName = entry.name.slice(0, -3);
          if (isUuidDir(baseName)) {
            const fullDbPath = path.join(targetDir, entry.name);
            const metaP = path.join(targetDir, `${baseName}.meta`);
            if (isInternalSessionMeta(metaP)) continue;
            if (!hasValidConversationSteps(fullDbPath)) continue;
            dbFiles.add(fullDbPath);
          }
        }
      }
    } catch {}
  }
  return Array.from(dbFiles);
}

function inferProjectPath(turns) {
  for (const turn of turns) {
    for (const part of turn.parts) {
      if (part.cwd?.trim()) return part.cwd;
    }
  }
  return null;
}

function sanitizeSessionShortTitle(text, maxLength = 36) {
  if (!text) return null;
  let cleaned = String(text).trim();
  cleaned = cleaned.replace(/<USER_REQUEST>([\s\S]*?)<\/USER_REQUEST>/i, "$1");
  cleaned = cleaned.replace(/<\/?(?:USER_REQUEST|ADDITIONAL_METADATA|system_information|context|prompt)[^>]*>/gi, " ");
  cleaned = cleaned.replace(/^#+\s+/gm, "");
  cleaned = cleaned.replace(/```[\s\S]*?```/g, " ");
  cleaned = cleaned.replace(/`([^`]+)`/g, "$1");
  cleaned = cleaned.replace(/^\s*[-*+]\s+/gm, "");
  const firstLine = cleaned.split(/\r?\n/).map((line) => line.trim()).find((line) => Boolean(line) && !line.startsWith("<!--")) ?? "";
  if (!firstLine) return null;
  if (firstLine.length <= maxLength) return firstLine;
  return `${firstLine.slice(0, maxLength - 3).trim()}...`;
}

function titleFromUserText(userText) {
  return sanitizeSessionShortTitle(userText, 36);
}

function sessionVersionToken(locator, fallbackKey = null) {
  if (locator && existsSync(locator)) {
    const st = statSync(locator);
    return `${st.size}:${st.mtimeMs}`;
  }
  return `summary-${fallbackKey ?? "none"}`;
}

let cachedSummaryTitles = null;
function getSummaryTitle(externalId, startLocation = null) {
  if (!externalId) return null;
  if (cachedSummaryTitles === null) {
    cachedSummaryTitles = new Map();
    const roots = resolveCandidateRoots(startLocation || expandPath(input.source?.location));
    for (const root of roots) {
      const sumDbPath = path.join(root, "conversation_summaries.db");
      if (existsSync(sumDbPath)) {
        let rows = [];
        if (DatabaseSync) {
          let db;
          try {
            db = new DatabaseSync(sumDbPath, { readOnly: true });
            const stmt = db.prepare("SELECT conversation_id, title FROM conversation_summaries;");
            rows = stmt.all();
          } catch {
            rows = [];
          } finally {
            if (db) db.close();
          }
        } else {
          try {
            const raw = execFileSync("sqlite3", ["-json", sumDbPath, "SELECT conversation_id, title FROM conversation_summaries;"], {
              encoding: "utf8",
              maxBuffer: 50 * 1024 * 1024,
            });
            rows = JSON.parse(raw || "[]");
          } catch {
            rows = [];
          }
        }
        for (const row of rows) {
          if (row.conversation_id && row.title && !isSyntheticWorkOrderText(row.title)) {
            if (!cachedSummaryTitles.has(row.conversation_id)) {
              cachedSummaryTitles.set(row.conversation_id, row.title.trim());
            }
          }
        }
      }
    }
  }
  return cachedSummaryTitles.get(externalId) || null;
}

let cachedSummaryWorkspaces = null;
function getSummaryWorkspacePath(externalId, startLocation = null) {
  if (!externalId) return null;
  if (cachedSummaryWorkspaces === null) {
    cachedSummaryWorkspaces = new Map();
    const roots = resolveCandidateRoots(startLocation || expandPath(input.source?.location));
    for (const root of roots) {
      const sumDbPath = path.join(root, "conversation_summaries.db");
      if (!existsSync(sumDbPath)) continue;
      let rows = [];
      if (DatabaseSync) {
        let db;
        try {
          db = new DatabaseSync(sumDbPath, { readOnly: true });
          rows = db.prepare("SELECT conversation_id, workspace_uris FROM conversation_summaries;").all();
        } catch {
          rows = [];
        } finally {
          if (db) db.close();
        }
      } else {
        try {
          const raw = execFileSync("sqlite3", ["-json", sumDbPath, "SELECT conversation_id, workspace_uris FROM conversation_summaries;"], {
            encoding: "utf8",
            maxBuffer: 50 * 1024 * 1024,
          });
          rows = JSON.parse(raw || "[]");
        } catch {
          rows = [];
        }
      }
      for (const row of rows) {
        if (!row.conversation_id || cachedSummaryWorkspaces.has(row.conversation_id)) continue;
        let uris = row.workspace_uris;
        if (typeof uris === "string") {
          try {
            uris = JSON.parse(uris);
          } catch {
            uris = [uris];
          }
        }
        const first = Array.isArray(uris) ? uris.find((uri) => typeof uri === "string" && uri.trim()) : null;
        const resolved = fileUriToProjectPath(first);
        if (resolved && !isAppDataPath(resolved)) {
          cachedSummaryWorkspaces.set(row.conversation_id, resolved);
        }
      }
    }
  }
  return cachedSummaryWorkspaces.get(externalId) || null;
}

function fileUriToProjectPath(uri) {
  if (!uri || typeof uri !== "string") return null;
  const trimmed = uri.trim();
  if (!trimmed.startsWith("file://")) return trimmed.startsWith("/") ? trimmed : null;
  let rest = trimmed.slice("file://".length);
  if (!rest.startsWith("/")) {
    const slash = rest.indexOf("/");
    rest = slash >= 0 ? rest.slice(slash) : `/${rest}`;
  }
  try {
    return decodeURIComponent(rest);
  } catch {
    return rest;
  }
}

// 会话的 projectPath 优先级：工具调用推断的 git 根 → 摘要库 workspace_uris → 原始推断值。
// 关键点是不要用应用私有数据目录冒充项目路径。
function resolveProjectPath(inferred, externalId, startLocation = null) {
  if (inferred && !isAppDataPath(inferred) && !isInternalExecutionPath(inferred)) return inferred;
  const fallback = getSummaryWorkspacePath(externalId, startLocation);
  return fallback || inferred || null;
}

// 单条会话解析被丢弃时记录原因，供 read_session 显式回传 `session_skipped`。
// 只有显式标注的跳过才会被上层当作「结构性跳过」写进增量水位；
// 没有信号的零产出会被上层当成解析失败并重试。
let lastParseSkipReason = null;
function skipParsedSession(reason) {
  lastParseSkipReason = reason;
  return null;
}

function parseSingleSessionFromTarget(targetPath, externalIdHint = null) {
  if (!targetPath || !existsSync(targetPath)) return skipParsedSession("missing_source");
  const flavor = detectClientFlavor(targetPath);
  try {
    const stat = statSync(targetPath);
    if (stat.isFile()) {
      if (targetPath.endsWith(".db")) {
        const metaP = targetPath.replace(/\.db$/, ".meta");
        if (isInternalSessionMeta(metaP)) return skipParsedSession("internal_session_meta");
        if (!hasValidConversationSteps(targetPath)) return skipParsedSession("no_valid_steps");

        const externalId = externalIdHint || path.basename(targetPath, ".db");
        emitProgress({
          stage: "parse",
          operation: "syncing_session_db",
          path: `${externalId} (${path.basename(targetPath)})`,
          current: 1,
          total: 1,
        });
        const parsed = parseSqliteDb(targetPath);
        const turns = displayTurns(parsed.turns);
        if (!turns.length) return skipParsedSession("no_turns");
        if (isInternalExecutionPath(parsed.projectPath)) return skipParsedSession("internal_execution");
        if (isSyntheticWorkOrderText(turns[0]?.user_text)) return skipParsedSession("synthetic_work_order");
        const title = getSummaryTitle(externalId) || titleFromUserText(turns[0]?.user_text);
        return applyTextBudgets(finalizeStructuredContentCards({
          external_id: externalId,
          title,
          project_path: resolveProjectPath(parsed.projectPath ?? inferProjectPath(turns), externalId),
          started_at: turns[0]?.started_at ?? null,
          updated_at: turns.at(-1)?.ended_at ?? turns.at(-1)?.started_at ?? null,
          source_locator: targetPath,
          source_fingerprint: sessionVersionToken(targetPath, externalId),
          execution_origin: flavor,
          turns,
        }));
      }

      // Pointing to a transcript file directly (e.g. transcript.jsonl or transcript_full.jsonl)
      emitProgress({
        stage: "parse",
        operation: "parsing_transcript",
        path: path.basename(targetPath),
        current: 1,
        total: 1,
      });
      const text = readFileSync(targetPath, "utf8");
      const parsed = parseTranscript(text, conversationDirForTranscript(targetPath));
      const turns = displayTurns(parsed.turns);
      if (!turns.length) return skipParsedSession("no_turns");
      if (isInternalExecutionPath(parsed.projectPath)) return skipParsedSession("internal_execution");
      if (isSyntheticWorkOrderText(turns[0]?.user_text)) return skipParsedSession("synthetic_work_order");
      const externalId = externalIdHint || path.basename(path.resolve(targetPath, "../../..")) || "antigravity-session";
      const title = getSummaryTitle(externalId) || titleFromUserText(turns[0]?.user_text);
      return applyTextBudgets(finalizeStructuredContentCards({
        external_id: externalId,
        title,
        project_path: resolveProjectPath(parsed.projectPath ?? inferProjectPath(turns), externalId),
        started_at: turns[0]?.started_at ?? null,
        updated_at: turns.at(-1)?.ended_at ?? turns.at(-1)?.started_at ?? null,
        source_locator: targetPath,
        source_fingerprint: sessionVersionToken(targetPath, externalId),
        execution_origin: flavor,
        turns,
      }));
    }

    // Check if targetPath is a conversation directory
    const transcriptInDir = findTranscriptFile(targetPath);
    if (transcriptInDir) {
      const externalId = externalIdHint || path.basename(targetPath) || "antigravity-session";
      emitProgress({
        stage: "parse",
        operation: "parsing_transcript",
        path: `${externalId}/${path.basename(transcriptInDir)}`,
        current: 1,
        total: 1,
      });
      const text = readFileSync(transcriptInDir, "utf8");
      const parsed = parseTranscript(text, targetPath);
      const turns = displayTurns(parsed.turns);
      if (!turns.length) return skipParsedSession("no_turns");
      if (isInternalExecutionPath(parsed.projectPath)) return skipParsedSession("internal_execution");
      if (isSyntheticWorkOrderText(turns[0]?.user_text)) return skipParsedSession("synthetic_work_order");
      const title = getSummaryTitle(externalId) || titleFromUserText(turns[0]?.user_text);
      return applyTextBudgets(finalizeStructuredContentCards({
        external_id: externalId,
        title,
        project_path: resolveProjectPath(parsed.projectPath ?? inferProjectPath(turns), externalId),
        started_at: turns[0]?.started_at ?? null,
        updated_at: turns.at(-1)?.ended_at ?? turns.at(-1)?.started_at ?? null,
        source_locator: transcriptInDir,
        source_fingerprint: sessionVersionToken(transcriptInDir, externalId),
        execution_origin: flavor,
        turns,
      }));
    }
  } catch {
    return skipParsedSession("parse_error");
  }
  return skipParsedSession("not_found");
}

function listSessions() {
  emitProgress({ stage: "reading", operation: "list_sessions" });
  let location = expandPath(input.source?.location);
  const roots = resolveCandidateRoots(location);
  const seenExternalIds = new Set();
  let count = 0;

  for (const root of roots) {
    const flavor = detectClientFlavor(root);
    const sumDbPath = path.join(root, "conversation_summaries.db");

    // 1. 尝试从 conversation_summaries.db 读取摘要
    if (existsSync(sumDbPath)) {
      let rows = [];
      if (DatabaseSync) {
        let db;
        try {
          db = new DatabaseSync(sumDbPath, { readOnly: true });
          const stmt = db.prepare("SELECT conversation_id, title, last_modified_time FROM conversation_summaries;");
          rows = stmt.all();
        } catch {
          rows = [];
        } finally {
          if (db) db.close();
        }
      } else {
        try {
          const raw = execFileSync("sqlite3", ["-json", sumDbPath, "SELECT conversation_id, title, last_modified_time FROM conversation_summaries;"], {
            encoding: "utf8",
            maxBuffer: 50 * 1024 * 1024,
          });
          rows = JSON.parse(raw || "[]");
        } catch {
          rows = [];
        }
      }

      for (const row of rows) {
        const id = row.conversation_id;
        if (!id || seenExternalIds.has(id)) continue;
        if (isSyntheticWorkOrderText(row.title)) continue;
        seenExternalIds.add(id);

        // 摘要库只在该 flavor 下索引到自己目录里的文件；会话文件可能已经被
        // 迁移到别的 flavor 目录（antigravity ↔ antigravity-cli ↔ antigravity-ide）。
        // 这里必须跨 root 定位，保证 descriptor 的版本令牌与 read_session
        // 实际解析的文件是同一个，否则每轮都会判成版本漂移。
        let locator = null;
        for (const candidateRoot of roots) {
          const transcript = findTranscriptFile(path.join(candidateRoot, "brain", id));
          if (transcript) {
            locator = transcript;
            break;
          }
          const dbF = path.join(candidateRoot, "conversations", `${id}.db`);
          if (existsSync(dbF)) {
            locator = dbF;
            break;
          }
        }

        const updatedAt = row.last_modified_time ? new Date(row.last_modified_time).toISOString() : new Date().toISOString();
        const descriptor = {
          external_id: id,
          title: row.title || undefined,
          updated_at: updatedAt,
          source_locator: locator || undefined,
          version_token: sessionVersionToken(locator, row.last_modified_time || id),
        };
        emit("item", { item: { kind: "session_descriptor", ...descriptor } });
        count++;
      }
    }

    // 2. 从 brain/ 目录提取带 transcript 的会话
    const brainDir = path.join(root, "brain");
    if (existsSync(brainDir)) {
      try {
        for (const entry of readdirSync(brainDir, { withFileTypes: true })) {
          if (!entry.isDirectory() || !isUuidDir(entry.name)) continue;
          const id = entry.name;
          if (seenExternalIds.has(id)) continue;
          const metaJson = path.join(brainDir, id, "meta.json");
          const dotMeta = path.join(brainDir, id, ".meta");
          if (isInternalSessionMeta(metaJson) || isInternalSessionMeta(dotMeta)) continue;
          const transcript = findTranscriptFile(path.join(brainDir, id));
          if (transcript) {
            seenExternalIds.add(id);
            const st = statSync(transcript);
            const descriptor = {
              external_id: id,
              title: getSummaryTitle(id, [root]) || undefined,
              updated_at: st.mtime.toISOString(),
              source_locator: transcript,
              version_token: sessionVersionToken(transcript, id),
            };
            emit("item", { item: { kind: "session_descriptor", ...descriptor } });
            count++;
          }
        }
      } catch {}
    }

    // 3. 从 conversations/*.db 提取未被收录的会话（例如 ACP 会话）
    const convDir = path.join(root, "conversations");
    if (existsSync(convDir)) {
      try {
        for (const entry of readdirSync(convDir, { withFileTypes: true })) {
          if (!entry.isFile() || !entry.name.endsWith(".db")) continue;
          const id = entry.name.slice(0, -3);
          if (!isUuidDir(id) || seenExternalIds.has(id)) continue;
          const full = path.join(convDir, entry.name);
          const metaP = path.join(convDir, `${id}.meta`);
          if (isInternalSessionMeta(metaP)) continue;
          if (!hasValidConversationSteps(full)) continue;

          seenExternalIds.add(id);
          const st = statSync(full);
          const descriptor = {
            external_id: id,
            title: getSummaryTitle(id, [root]) || undefined,
            updated_at: st.mtime.toISOString(),
            source_locator: full,
            version_token: sessionVersionToken(full, id),
          };
          emit("item", { item: { kind: "session_descriptor", ...descriptor } });
          count++;
        }
      } catch {}
    }
  }

  emit("complete", { item: { snapshot_complete: true, session_count: count } });
}

function readSession(onSession = null) {
  const requestedLocator = expandPath(input.params?.source_locator);
  const requestedSessionId = input.params?.session_id ?? null;

  // 1. 如果指定了 source_locator，优先直连定位解析
  if (requestedLocator && existsSync(requestedLocator)) {
    const single = parseSingleSessionFromTarget(requestedLocator, requestedSessionId);
    if (single) {
      if (onSession) onSession(single, 1, 1);
      return [single];
    }
  }

  let location = expandPath(input.source?.location);
  const roots = resolveCandidateRoots(location);

  // 2. 如果指定了 session_id，尝试快速候选探测（跨所有 candidate roots）
  if (requestedSessionId) {
    const candidates = [];
    if (location) {
      candidates.push(
        path.join(location, "brain", requestedSessionId),
        path.join(location, requestedSessionId),
        path.join(location, "conversations", `${requestedSessionId}.db`),
        path.join(location, `${requestedSessionId}.db`),
      );
    }
    for (const root of roots) {
      candidates.push(
        path.join(root, "brain", requestedSessionId, ".system_generated", "logs", "transcript_full.jsonl"),
        path.join(root, "brain", requestedSessionId, ".system_generated", "logs", "transcript.jsonl"),
        path.join(root, "brain", requestedSessionId),
        path.join(root, "conversations", `${requestedSessionId}.db`),
      );
    }
    for (const candidate of candidates) {
      if (existsSync(candidate)) {
        const single = parseSingleSessionFromTarget(candidate, requestedSessionId);
        if (single) {
          if (onSession) onSession(single, 1, 1);
          return [single];
        }
      }
    }
  }

  // 3. 判断 location 本身是否直接指向单条记录
  if (location) {
    const directSession = parseSingleSessionFromTarget(location, requestedSessionId);
    if (directSession) {
      if (onSession) onSession(directSession, 1, 1);
      return [directSession];
    }
  }

  // 4. 全量遍历发现的所有脑区与 DB
  emitProgress({
    stage: "scan",
    operation: "discovering_sessions",
    path: path.basename(location || "all_flavors"),
  });
  const brainDirs = discoverBrainDirs(location);
  const seenDirPaths = new Set();
  const seenExternalIds = new Set();
  const sessions = [];

  const allConvDirs = [];
  for (const bDir of brainDirs) {
    const conversationDirs = discoverConversationDirs(bDir);
    for (const convDir of conversationDirs) {
      const canonicalConvDir = path.resolve(convDir);
      if (seenDirPaths.has(canonicalConvDir)) continue;
      seenDirPaths.add(canonicalConvDir);
      allConvDirs.push(convDir);
    }
  }

  const dbFiles = discoverConversationDbFiles(location);
  const totalItems = allConvDirs.length + dbFiles.length;
  let currentItemIdx = 0;

  for (const convDir of allConvDirs) {
    currentItemIdx += 1;
    const externalId = path.basename(convDir);
    if (requestedSessionId && externalId !== requestedSessionId) continue;
    if (seenExternalIds.has(externalId)) continue;

    const transcriptPath = findTranscriptFile(convDir);
    if (!transcriptPath) continue;

    emitProgress({
      stage: "parse",
      operation: "parsing_transcript",
      path: `${externalId}/transcript.jsonl`,
      current: currentItemIdx,
      total: totalItems,
    });

    let text;
    try {
      text = readFileSync(transcriptPath, "utf8");
    } catch {
      continue;
    }
    const parsed = parseTranscript(text, convDir);
    const turns = displayTurns(parsed.turns);
    if (!turns.length) {
      lastParseSkipReason = "no_turns";
      continue;
    }
    if (isInternalExecutionPath(parsed.projectPath)) {
      lastParseSkipReason = "internal_execution";
      continue;
    }
    if (isSyntheticWorkOrderText(turns[0]?.user_text)) {
      lastParseSkipReason = "synthetic_work_order";
      continue;
    }
    seenExternalIds.add(externalId);

    const s = applyTextBudgets(finalizeStructuredContentCards({
      external_id: externalId,
      title: getSummaryTitle(externalId) || titleFromUserText(turns[0]?.user_text),
      project_path: resolveProjectPath(parsed.projectPath ?? inferProjectPath(turns), externalId),
      started_at: turns[0]?.started_at ?? null,
      updated_at: turns.at(-1)?.ended_at ?? turns.at(-1)?.started_at ?? null,
      source_locator: transcriptPath,
      source_fingerprint: sessionVersionToken(transcriptPath, externalId),
      execution_origin: detectClientFlavor(transcriptPath),
      turns,
    }));
    if (onSession) onSession(s, currentItemIdx, totalItems);
    sessions.push(s);
    if (requestedSessionId && externalId === requestedSessionId) {
      return [s];
    }
  }

  // Conversation DB files (e.g. Antigravity ACP or standalone sqlite conversations)
  for (const dbPath of dbFiles) {
    currentItemIdx += 1;
    const externalId = path.basename(dbPath, ".db");
    if (requestedSessionId && externalId !== requestedSessionId) continue;
    if (seenExternalIds.has(externalId)) continue;

    emitProgress({
      stage: "parse",
      operation: "syncing_session_db",
      path: `${externalId} (${path.basename(dbPath)})`,
      current: currentItemIdx,
      total: totalItems,
    });

    try {
      const parsed = parseSqliteDb(dbPath);
      const turns = displayTurns(parsed.turns);
      if (!turns.length) {
        lastParseSkipReason = "no_turns";
        continue;
      }
      if (isInternalExecutionPath(parsed.projectPath)) {
        lastParseSkipReason = "internal_execution";
        continue;
      }
      if (isSyntheticWorkOrderText(turns[0]?.user_text)) {
        lastParseSkipReason = "synthetic_work_order";
        continue;
      }
      seenExternalIds.add(externalId);
      const s = applyTextBudgets(finalizeStructuredContentCards({
        external_id: externalId,
        title: getSummaryTitle(externalId) || titleFromUserText(turns[0]?.user_text),
        project_path: resolveProjectPath(parsed.projectPath ?? inferProjectPath(turns), externalId),
        started_at: turns[0]?.started_at ?? null,
        updated_at: turns.at(-1)?.ended_at ?? turns.at(-1)?.started_at ?? null,
        source_locator: dbPath,
        source_fingerprint: sessionVersionToken(dbPath, externalId),
        execution_origin: detectClientFlavor(dbPath),
        turns,
      }));
      if (onSession) onSession(s, currentItemIdx, totalItems);
      sessions.push(s);
      if (requestedSessionId && externalId === requestedSessionId) {
        return [s];
      }
    } catch {}
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

function extractSessionIdFromTranscriptPath(filePath, conversationDir = null) {
  const convDir = conversationDir || conversationDirForTranscript(filePath);
  for (const metaName of ["meta.json", ".meta"]) {
    const metaPath = path.join(convDir, metaName);
    if (existsSync(metaPath)) {
      try {
        const meta = JSON.parse(readFileSync(metaPath, "utf8"));
        const id = meta.session_id || meta.id || meta.conversation_id || meta.external_id;
        if (id && typeof id === "string") return id;
      } catch {}
    }
  }
  const base = path.basename(convDir);
  if (isUuidDir(base)) return base;
  const parts = filePath.split(path.sep);
  for (let i = parts.length - 1; i >= 0; i--) {
    if (isUuidDir(parts[i])) return parts[i];
  }
  return base || "antigravity-session";
}

function collectTranscriptFiles(startLocation) {
  const results = [];
  const seenFiles = new Set();

  if (startLocation) {
    const resolved = path.resolve(expandPath(startLocation));
    if (existsSync(resolved)) {
      const st = statSync(resolved);
      // 1. Direct file
      if (st.isFile()) {
        if (resolved.endsWith(".jsonl")) {
          const sid = extractSessionIdFromTranscriptPath(resolved);
          return [{ filePath: resolved, sessionId: sid }];
        }
        return [];
      }
      // 2. Direct conversation directory
      const directTranscript = findTranscriptFile(resolved);
      if (directTranscript) {
        const metaJson = path.join(resolved, "meta.json");
        const dotMeta = path.join(resolved, ".meta");
        if (!isInternalSessionMeta(metaJson) && !isInternalSessionMeta(dotMeta)) {
          const sid = extractSessionIdFromTranscriptPath(directTranscript, resolved);
          return [{ filePath: directTranscript, sessionId: sid }];
        }
        return [];
      }
    }
  }

  // 3. Brain directories discovery (supporting multi-brain roots and standard flavors)
  const brainDirs = discoverBrainDirs(startLocation);
  for (const bDir of brainDirs) {
    if (!existsSync(bDir)) continue;
    try {
      const entries = readdirSync(bDir, { withFileTypes: true });
      for (const entry of entries) {
        if (!entry.isDirectory() || entry.name.startsWith(".")) continue;
        const convDir = path.join(bDir, entry.name);
        const metaJson = path.join(convDir, "meta.json");
        const dotMeta = path.join(convDir, ".meta");
        if (isInternalSessionMeta(metaJson) || isInternalSessionMeta(dotMeta)) continue;

        const transcript = findTranscriptFile(convDir);
        if (transcript) {
          const resolvedPath = path.resolve(transcript);
          if (!seenFiles.has(resolvedPath)) {
            seenFiles.add(resolvedPath);
            const sid = extractSessionIdFromTranscriptPath(resolvedPath, convDir);
            results.push({ filePath: resolvedPath, sessionId: sid });
          }
        }
      }
    } catch {}
  }

  return results;
}

function readUsage() {
  const location = expandPath(input.source?.location);
  if (!location) {
    emit("complete", {
      item: {
        snapshot_complete: true,
        usage_event_count: 0,
        next_cursor: null,
        decoder_profile: "antigravity-transcript-v1",
        diagnostics: [],
      },
    });
    return;
  }

  const cursorRaw = input.params?.cursor;
  let cursor = null;
  if (cursorRaw) {
    try {
      cursor = typeof cursorRaw === "string" ? JSON.parse(cursorRaw) : cursorRaw;
    } catch {}
  }
  const cursorMtime = cursor?.last_mtime ? Number(cursor.last_mtime) : 0;
  const processedFiles = new Set(cursor?.processed_files || cursor?.processed_dbs || []);

  const files = collectTranscriptFiles(location);
  const requestedSessionId = input.params?.session_id ?? null;
  if (requestedSessionId) {
    if (files.length === 1) {
      files[0].sessionId = requestedSessionId;
    } else {
      const filtered = files.filter((f) => f.sessionId === requestedSessionId);
      files.length = 0;
      files.push(...filtered);
    }
  }

  let maxMtime = cursorMtime;
  let eventCount = 0;
  const newProcessedFiles = [];

  for (const item of files) {
    const { filePath, sessionId } = item;
    try {
      const stat = statSync(filePath);
      const mtimeMs = stat.mtimeMs;
      if (mtimeMs > maxMtime) {
        maxMtime = mtimeMs;
      }
      if (cursorMtime > 0 && mtimeMs <= cursorMtime && processedFiles.has(filePath)) {
        newProcessedFiles.push(filePath);
        continue;
      }

      const text = readFileSync(filePath, "utf8");
      const lines = text.split("\n");
      let currentModel = "gemini-2.5-pro";
      let turnIndex = -1;
      let currentTurn = null;

      const finalizeTurn = () => {
        if (!currentTurn) return;
        const totalTokens =
          currentTurn.input_tokens +
          currentTurn.cache_read_tokens +
          currentTurn.cache_write_tokens +
          currentTurn.reasoning_tokens +
          currentTurn.output_tokens;

        if (totalTokens > 0) {
          const turnId = `turn-${currentTurn.turn_index}`;
          const eventId = `${sessionId}-${turnId}`;
          const rawTime =
            currentTurn.first_response_timestamp ||
            currentTurn.user_timestamp ||
            stat.mtime.toISOString();
          let timestamp;
          try {
            timestamp = new Date(rawTime).toISOString();
          } catch {
            timestamp = stat.mtime.toISOString();
          }

          emit("item", {
            item: {
              kind: "usage_event",
              event: {
                external_event_id: eventId,
                session_id: sessionId,
                turn_id: turnId,
                logical_request_id: eventId,
                attempt_index: 0,
                timestamp,
                provider: "google",
                model: currentTurn.model || currentModel,
                status: "completed",
                input_tokens: currentTurn.input_tokens,
                cache_read_tokens: currentTurn.cache_read_tokens,
                cache_write_tokens: currentTurn.cache_write_tokens,
                reasoning_tokens: currentTurn.reasoning_tokens,
                output_tokens: currentTurn.output_tokens,
                total_tokens: totalTokens,
                host_reported_cost: null,
                currency: null,
              },
            },
          });
          eventCount++;
        }
        currentTurn = null;
      };

      for (const line of lines) {
        if (!line.trim()) continue;
        let step;
        try {
          step = JSON.parse(line);
        } catch {
          continue;
        }

        const source = step.source ?? "";
        const type = step.type ?? "";
        const createdAt = step.created_at ?? step.timestamp ?? null;

        if (typeof step.content === "string") {
          const match = step.content.match(
            /The user changed setting [`']?Model Selection[`']? from .+? to (.+?)(?:\.\s+[A-Z]|\n|$)/i
          );
          if (match) {
            currentModel = match[1].replace(/\.+$/, "").trim();
            if (currentTurn) currentTurn.model = currentModel;
          }
        }
        if (step.model) {
          currentModel = String(step.model);
          if (currentTurn) currentTurn.model = currentModel;
        }

        if (type === "USER_INPUT" && source === "USER_EXPLICIT") {
          finalizeTurn();
          turnIndex++;
          currentTurn = {
            turn_index: turnIndex,
            user_timestamp: createdAt,
            first_response_timestamp: null,
            model: null,
            input_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            reasoning_tokens: 0,
            output_tokens: 0,
          };
          continue;
        }

        if (!currentTurn) {
          // Defensive protection: if first row is not USER_INPUT but has PLANNER_RESPONSE with tokens
          if (
            type === "PLANNER_RESPONSE" &&
            (Number(step.input_tokens || 0) > 0 || Number(step.output_tokens || 0) > 0)
          ) {
            turnIndex++;
            currentTurn = {
              turn_index: turnIndex,
              user_timestamp: createdAt,
              first_response_timestamp: createdAt,
              model: null,
              input_tokens: 0,
              cache_read_tokens: 0,
              cache_write_tokens: 0,
              reasoning_tokens: 0,
              output_tokens: 0,
            };
          } else {
            continue;
          }
        }

        if (type === "PLANNER_RESPONSE" && (source === "MODEL" || source === "PLANNER" || !source)) {
          if (!currentTurn.first_response_timestamp && createdAt) {
            currentTurn.first_response_timestamp = createdAt;
          }
          currentTurn.input_tokens += Number(step.input_tokens || 0);
          currentTurn.cache_read_tokens += Number(step.cache_read_tokens || 0);
          currentTurn.cache_write_tokens += Number(step.cache_write_tokens || 0);
          currentTurn.reasoning_tokens += Number(step.reasoning_tokens || 0);
          currentTurn.output_tokens += Number(step.output_tokens || 0);
        }
      }

      finalizeTurn();
      newProcessedFiles.push(filePath);
    } catch {
      // Ignore individual corrupted file
    }
  }

  const nextCursor = {
    last_mtime: maxMtime,
    processed_files: newProcessedFiles.slice(-2000),
  };

  emit("complete", {
    item: {
      snapshot_complete: true,
      usage_event_count: eventCount,
      next_cursor: JSON.stringify(nextCursor),
      decoder_profile: "antigravity-transcript-v1",
      diagnostics: [],
    },
  });
}

try {
  if (input.method === "project_command_parts") {
    const projections = projectCommandParts(input.params?.parts ?? input.params?.command_parts);
    for (const projection of projections) emit("item", { item: { kind: "command_projection", ...projection } });
    emit("complete", { item: { projection_count: projections.length, projector_version: SHELL_PROJECTOR_VERSION } });
  } else if (input.method === "list_sessions") {
    listSessions();
  } else if (input.method === "probe") {
    emitProgress({ stage: "reading", operation: "probe" });
    listSessions();
  } else if (input.method === "read_session") {
    emitProgress({ stage: "reading", operation: "read_session" });
    let emittedCount = 0;
    readSession((session, current, total) => {
      emittedCount += 1;
      emitProgress({
        stage: "reading",
        operation: "read_session",
        current,
        total,
        path: session.external_id,
      });
      emit("item", { item: { kind: "session", session: finalizeStructuredContentCards(session) } });
    });
    const requestedSessionId = input.params?.session_id ?? null;
    if (emittedCount === 0 && requestedSessionId) {
      emit("item", {
        item: {
          kind: "session_skipped",
          external_id: requestedSessionId,
          reason: lastParseSkipReason || "no_turns",
        },
      });
    }
    emit("complete", { item: { session_count: emittedCount } });
  } else if (input.method === "read_usage") {
    emitProgress({ stage: "reading", operation: "read_usage" });
    readUsage();
  } else {
    fail(`unsupported method: ${input.method}`);
  }
} catch (error) {
  fail(error instanceof Error ? error.message : String(error));
}
