#!/usr/bin/env node
import { normalizeSessionPayload } from "./payload-policy.mjs";
import shellProjector from "./shell-projector.cjs";
const { projectCommandParts, SHELL_PROJECTOR_VERSION } = shellProjector;
import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { homedir } from "node:os";
import path from "node:path";

const input = JSON.parse(readFileSync(0, "utf8") || "{}");
const CONTENT_CARD_SCHEMA_VERSION = "claude-code-content-cards-v8";

process.stdout.on("error", (err) => {
  if (err.code === "EPIPE") process.exit(0);
});

function emit(type, payload = {}) {
  process.stdout.write(`${JSON.stringify({ type, ...payload })}\n`);
}

function fail(message) {
  emit("error", { message });
  emit("complete", { item: {} });
}

function emitProgress(progress = {}) {
  emit("progress", {
    progress: {
      stage: progress.stage ?? "reading",
      operation: progress.operation ?? "scanning",
      worker: progress.worker ?? process.env.ASSETIWEAVE_WORKER_ID ?? undefined,
      path: progress.path,
      current: progress.current,
      total: progress.total,
    },
  });
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

function fileVersionToken(filePath) {
  const stat = statSync(filePath);
  return sha256(`${CONTENT_CARD_SCHEMA_VERSION}\0${stat.size}\0${stat.mtimeMs}`);
}

function readStableFile(filePath) {
  for (let attempt = 0; attempt < 2; attempt++) {
    const before = fileVersionToken(filePath);
    const text = readFileSync(filePath, "utf8");
    const after = fileVersionToken(filePath);
    if (before === after) return { text, versionToken: after };
  }
  throw new Error(`session changed while being read: ${filePath}`);
}

function compact(value) {
  return JSON.stringify(value);
}

function compactObject(value) {
  return Object.fromEntries(
    Object.entries(value).filter(([, entry]) => entry !== null && entry !== undefined && entry !== ""),
  );
}

function metadata(contentCard, extra = {}) {
  return JSON.stringify({
    ...(extra && typeof extra === "object" && !Array.isArray(extra) ? extra : {}),
    content_card: contentCard,
  });
}

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


function isSubagentSessionPath(filePath) {
  if (!filePath) return false;
  const norm = filePath.replace(/\\/g, "/").toLowerCase();
  return (
    norm.includes("/subagents/") ||
    norm.includes("/subagent/") ||
    norm.includes(".subagent.") ||
    norm.endsWith(".subagent.jsonl")
  );
}

function collectJsonlFiles(root) {
  if (!existsSync(root)) return [];
  if (statSync(root).isFile()) return root.endsWith(".jsonl") ? [root] : [];
  const files = [];
  const stack = [root];
  while (stack.length) {
    const dir = stack.pop();
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const fullPath = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        stack.push(fullPath);
      } else if (entry.isFile() && entry.name.endsWith(".jsonl")) {
        files.push(fullPath);
      }
    }
  }
  return files;
}

function eventPayload(value) {
  return value.item ?? value.message ?? value.msg ?? value.payload ?? value;
}

function stringField(value, names) {
  for (const name of names) {
    const candidate = value?.[name];
    if (typeof candidate === "string") return candidate;
    if (typeof candidate === "number") return String(candidate);
  }
  return null;
}

function parseJsonString(value) {
  if (typeof value !== "string") return null;
  const text = value.trim();
  if (!text.startsWith("{") && !text.startsWith("[")) return null;
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

function nestedStringField(value, names, depth = 0) {
  if (depth > 6) return null;
  const direct = stringField(value, names);
  if (direct?.trim()) return direct.trim();
  if (!value || typeof value !== "object") return null;
  for (const key of ["tool_input", "input", "arguments", "args", "payload", "message"]) {
    const child = value[key];
    if (child == null) continue;
    const parsed = parseJsonString(child);
    const found = nestedStringField(parsed ?? child, names, depth + 1);
    if (found) return found;
  }
  return null;
}

function projectPathFromValue(value) {
  return nestedStringField(value, [
    "cwd",
    "workdir",
    "working_directory",
    "workingDirectory",
    "directory",
    "project_path",
    "projectPath",
  ]);
}

function roleOf(value) {
  return stringField(value, ["role"]) ?? stringField(value?.message, ["role"]);
}

function objectFlagTrue(value, names) {
  return names.some((name) => value?.[name] === true);
}

function valueContainsType(value, expectedType) {
  if (Array.isArray(value)) return value.some((item) => valueContainsType(item, expectedType));
  if (value && typeof value === "object") {
    return value.type === expectedType || valueContainsType(value.content, expectedType);
  }
  return false;
}

function isUserToolResultMessage(value) {
  return valueContainsType(value?.content, "tool_result");
}

function contentItemsOfType(value, expectedType) {
  const content = value?.content;
  if (!Array.isArray(content)) return [];
  return content.filter((item) => item && typeof item === "object" && item.type === expectedType);
}

function isIgnoredContentValue(value) {
  const type = stringField(value, ["type"]) ?? "";
  return [
    "attachment",
    "auth_status",
    "compaction",
    "compaction_summary",
    "context_compaction",
    "custom_tool_call",
    "custom_tool_call_output",
    "event_msg",
    "function_call",
    "function_call_output",
    "grouped_tool_use",
    "hook_result",
    "image_generation_call",
    "local_shell_call",
    "mcp_tool_call",
    "mcp_tool_call_output",
    "progress",
    "rate_limit_event",
    "reasoning",
    "result",
    "system",
    "tombstone",
    "tool_result",
    "tool_search_call",
    "tool_search_output",
    "tool_use",
    "tool_use_summary",
    "turn_context",
    "web_search_call",
  ].includes(type) || value?.tool_use_id != null || value?.toolUseID != null || value?.call_id != null || value?.callID != null || value?.tool_name != null || value?.toolName != null;
}

function collectUserContentText(value, texts) {
  if (typeof value === "string") {
    if (value.trim()) texts.push(value);
    return;
  }
  if (Array.isArray(value)) {
    for (const item of value) collectUserContentText(item, texts);
    return;
  }
  if (!value || typeof value !== "object") return;
  const itemType = value.type ?? "";
  if (objectFlagTrue(value, ["synthetic", "ignored", "isSynthetic", "isMeta"])) return;
  if (["attachment", "file", "hook_result", "image", "input_image", "reasoning", "thinking", "tool_result", "tool_use"].includes(itemType)) return;
  if (["", "text", "input_text", "user", "message"].includes(itemType) && typeof value.text === "string") {
    if (value.text.trim()) texts.push(value.text);
    return;
  }
  if (typeof value.input_text === "string") {
    if (value.input_text.trim()) texts.push(value.input_text);
    return;
  }
  collectUserContentText(value.content, texts);
}

function extractUserMessageText(value) {
  const texts = [];
  if (value?.content != null) collectUserContentText(value.content, texts);
  else if (typeof value?.text === "string") texts.push(value.text);
  return texts.join("\n\n").trim();
}

function extractText(value) {
  const texts = [];
  collectUserContentText(value?.content ?? value?.text ?? value, texts);
  return texts.join("\n\n").trim();
}

function extractReasoningText(value) {
  if (!value || typeof value !== "object") return "";
  const candidates = [value.thinking, value.reasoning, value.text, value.content];
  const texts = [];
  const collect = (candidate) => {
    if (typeof candidate === "string") {
      if (candidate.trim()) texts.push(candidate.trim());
      return;
    }
    if (Array.isArray(candidate)) {
      candidate.forEach(collect);
      return;
    }
    if (candidate && typeof candidate === "object") {
      collect(candidate.text ?? candidate.thinking ?? candidate.reasoning ?? candidate.content);
    }
  };
  candidates.forEach(collect);
  return [...new Set(texts)].join("\n\n").trim();
}

function reasoningPart(value) {
  const text = extractReasoningText(value);
  if (!text) return null;
  return {
    role: "assistant",
    kind: "text",
    text,
    language: null,
    command: null,
    cwd: null,
    status: null,
    exit_code: null,
    content_card: {
      schema_version: 1,
      kind: "claude-code.reasoning",
      renderer: "markdown",
    },
    metadata_json: JSON.stringify({ source_type: value.type }),
  };
}

function isMessageLikePayload(payload, recordType) {
  return recordType === "message" || payload?.content != null || payload?.text != null;
}

function realUserText(value, payload, role, recordType) {
  if (isIgnoredContentValue(value) || isIgnoredContentValue(payload)) return null;
  if (isUserToolResultMessage(payload)) return null;
  const boundary = (recordType === "user" && value.content != null) || (role === "user" && isMessageLikePayload(payload, recordType));
  if (!boundary) return null;
  const text = extractUserMessageText(payload);
  return text.trim() ? text.trim() : null;
}

function toolText(value) {
  if (typeof value?.tool_output === "string") return value.tool_output;
  if (value?.tool_output != null) return compact(value.tool_output);
  if (typeof value?.output === "string") return value.output;
  if (typeof value?.content === "string") return value.content;
  return "";
}

function toolInput(value) {
  const candidate = value?.tool_input ?? value?.input ?? value?.arguments ?? value?.args;
  return parseJsonString(candidate) ?? candidate ?? null;
}

function compactToolMetadata(value, extra = {}) {
  return compactObject({
    type: stringField(value, ["type"]),
    id: stringField(value, ["id", "uuid", "call_id", "callID"]),
    tool_use_id: stringField(value, ["tool_use_id", "toolUseID"]),
    name: stringField(value, ["tool_name", "toolName", "name"]),
    ...extra,
  });
}

function sourceExecutionId(value) {
  const candidate = stringField(value, ["tool_use_id", "toolUseID", "id", "uuid", "call_id", "callID"]);
  return candidate?.trim() || null;
}

function executionKindForTool(toolName) {
  const normalized = String(toolName ?? "").trim().toLowerCase();
  if (/^(read|view|list|ls)/.test(normalized)) return "read";
  if (/^(grep|glob|search|find)/.test(normalized)) return "search";
  if (/^(edit|write|patch|notebookedit)/.test(normalized)) return "file_change";
  if (/^(bash|shell|exec|command)/.test(normalized)) return "shell";
  return null;
}

function toolFilePath(input) {
  return stringField(input, ["file_path", "filePath", "path", "AbsolutePath"])
    ?? projectPathFromValue(input)
    ?? null;
}

function toolLineRange(input) {
  const lineStart = Number.isInteger(input?.offset)
    ? input.offset
    : Number.isInteger(input?.line_start)
      ? input.line_start
      : Number.isInteger(input?.lineStart)
        ? input.lineStart
        : null;
  const limit = Number.isInteger(input?.limit) ? input.limit : null;
  const lineEnd = Number.isInteger(input?.line_end)
    ? input.line_end
    : Number.isInteger(input?.lineEnd)
      ? input.lineEnd
      : lineStart != null && limit != null && limit > 0
        ? lineStart + limit - 1
        : null;
  return { lineStart, lineEnd, limit };
}

function toolUsePart(value) {
  const toolName = stringField(value, ["tool_name", "toolName", "name"]);
  const input = toolInput(value);
  const command = stringField(input, ["command"]) ?? stringField(value, ["command", "cmd"]);
  const cwd = projectPathFromValue(input) ?? projectPathFromValue(value) ?? null;
  const executionKind = executionKindForTool(toolName);
  const filePath = toolFilePath(input);
  const range = toolLineRange(input);
  const executionCommand = command?.trim() || filePath;
  const rawDescription = stringField(input, ["description", "desc"]);
  const commandLabel = rawDescription?.trim() || toolName || null;
  if (executionKind && executionCommand?.trim()) {
    return [{
      role: "tool",
      kind: "command",
      text: null,
      language: null,
      command: executionCommand,
      cwd,
      status: null,
      exit_code: null,
      source_execution_id: sourceExecutionId(value),
      command_label: commandLabel,
      metadata_json: metadata(
        compactObject({ type: "command", cwd }),
        compactObject({
          ...compactToolMetadata(value),
          source_type: toolName,
          execution_kind: executionKind,
          file_path: filePath,
          line_start: range.lineStart,
          line_end: range.lineEnd,
          line_limit: range.limit,
          command_label: commandLabel,
        }),
      ),
    }];
  }

  const inputText = input == null
    ? ""
    : typeof input === "string"
      ? input
      : JSON.stringify(input, null, 2);
  const text = [toolName ? `Tool: ${toolName}` : "Tool", inputText].filter((entry) => entry.trim()).join("\n\n");
  if (!text.trim()) return [];
  return [{
    role: "tool",
    kind: "tool",
    text,
    language: null,
    command: null,
    cwd,
    status: null,
    exit_code: null,
    source_execution_id: sourceExecutionId(value),
    metadata_json: metadata(
      { type: "tool", format: input && typeof input === "object" ? "json" : "plain" },
      compactObject({ ...compactToolMetadata(value), source_type: toolName }),
    ),
  }];
}

function unifiedDiffFromStructuredPatch(result) {
  const hunks = result?.structuredPatch ?? result?.structured_patch;
  const filePath = stringField(result, ["filePath", "file_path", "path"]);
  if (!filePath || !Array.isArray(hunks) || hunks.length === 0) return null;
  const gitPath = filePath.replace(/^[./\\]+/, "").replaceAll("\\", "/");
  const lines = [
    `diff --git a/${gitPath} b/${gitPath}`,
    `--- a/${gitPath}`,
    `+++ b/${gitPath}`,
  ];
  for (const hunk of hunks) {
    const oldStart = Number.isInteger(hunk?.oldStart) ? hunk.oldStart : 1;
    const oldLines = Number.isInteger(hunk?.oldLines) ? hunk.oldLines : 0;
    const newStart = Number.isInteger(hunk?.newStart) ? hunk.newStart : 1;
    const newLines = Number.isInteger(hunk?.newLines) ? hunk.newLines : 0;
    lines.push(`@@ -${oldStart},${oldLines} +${newStart},${newLines} @@`);
    if (Array.isArray(hunk?.lines)) lines.push(...hunk.lines.map((line) => String(line)));
  }
  return lines.join("\n");
}

function toolResultText(value, parent) {
  if (typeof value?.content === "string") return value.content;
  if (Array.isArray(value?.content)) return extractText({ content: value.content });
  const result = parent?.toolUseResult ?? parent?.tool_use_result;
  if (typeof result?.stdout === "string" || typeof result?.stderr === "string") {
    return [result.stdout, result.stderr].filter((entry) => typeof entry === "string" && entry.trim()).join("\n\n");
  }
  if (typeof result?.content === "string") return result.content;
  if (Array.isArray(result?.content)) return extractText({ content: result.content });
  if (result != null) return compact(result);
  return toolText(value);
}

function toolResultPart(value, parent) {
  const result = parent?.toolUseResult ?? parent?.tool_use_result;
  const structuredDiff = unifiedDiffFromStructuredPatch(result);
  const text = structuredDiff ?? toolResultText(value, parent);
  if (!text.trim()) return [];
  const status = stringField(result, ["status"])
    ?? (value?.is_error === true || value?.isError === true ? "failed" : null)
    ?? (result?.interrupted === true ? "interrupted" : null);
  const exitCode = Number.isInteger(result?.exit_code)
    ? result.exit_code
    : Number.isInteger(result?.exitCode)
      ? result.exitCode
      : null;
  return [{
    role: "tool",
    kind: structuredDiff ? "file_change" : "tool",
    text,
    language: null,
    command: null,
    cwd: null,
    status,
    exit_code: exitCode,
    source_execution_id: sourceExecutionId(value),
    metadata_json: metadata(
      compactObject({ type: "result", format: "plain", status, exit_code: exitCode }),
      compactObject({
        ...compactToolMetadata(value),
        source_type: structuredDiff ? "file_change" : undefined,
        execution_kind: structuredDiff ? "file_change" : undefined,
        file_path: stringField(result, ["filePath", "file_path", "path"]),
      }),
    ),
  }];
}

function assistantMessageParts(payload, parent) {
  if (!Array.isArray(payload?.content)) {
    const text = extractText(payload);
    const part = text.trim() ? textPart("assistant", text) : null;
    return part ? [part] : [];
  }

  const parts = [];
  for (const item of payload.content) {
    if (!item || typeof item !== "object") {
      const textPartValue = textPart("assistant", item);
      if (textPartValue) parts.push(textPartValue);
      continue;
    }
    if (item.type === "tool_use") {
      parts.push(...toolUsePart(item));
      continue;
    }
    if (item.type === "tool_result") {
      parts.push(...toolResultPart(item, parent));
      continue;
    }
    if (item.type === "reasoning" || item.type === "thinking") {
      const part = reasoningPart(item);
      if (part) parts.push(part);
      continue;
    }
    if (isIgnoredContentValue(item)) continue;
    const text = extractText(item);
    if (text.trim()) {
      const part = textPart("assistant", text);
      if (part) parts.push(part);
    }
  }
  return parts;
}

function toolPart(value) {
  const recordType = stringField(value, ["type"]) ?? "";
  const toolName = stringField(value, ["tool_name", "toolName", "name"]);
  const command = stringField(value?.tool_input, ["command"]) ?? stringField(value, ["command", "cmd"]);
  const text = toolText(value) || (toolName ? `${recordType || "tool"}: ${toolName}` : "");
  if (!command && !text.trim()) return [];
  const lowerName = (toolName ?? "").toLowerCase();
  const kind = lowerName.includes("patch") || lowerName.includes("edit") || recordType === "patch"
    ? "file_change"
    : command || recordType.includes("shell")
      ? "command"
      : "tool";
  const cwd = projectPathFromValue(value?.tool_input) ?? projectPathFromValue(value) ?? null;
  const status = stringField(value, ["status"]) ?? null;
  const exitCode = Number.isInteger(value?.exit_code) ? value.exit_code : null;
  if (kind === "command") {
    const parts = [];
    const executionId = sourceExecutionId(value)
      ?? (command?.trim() && text.trim()
        ? `inline-${sha256(JSON.stringify(value)).slice(0, 24)}`
        : null);
    if (command?.trim()) {
      parts.push({
        role: "tool",
        kind: "command",
        text: null,
        language: null,
        command,
        cwd,
        status: null,
        exit_code: null,
        source_execution_id: executionId,
        metadata_json: metadata(compactObject({ type: "command", cwd }), value),
      });
    }
    if (text.trim()) {
      parts.push({
        role: "tool",
        kind: "tool",
        text,
        language: null,
        command: null,
        cwd: null,
        status,
        exit_code: exitCode,
        source_execution_id: executionId,
        metadata_json: metadata(
          compactObject({ type: "result", format: "plain", status, exit_code: exitCode }),
          value,
        ),
      });
    }
    return parts;
  }
  return [{
    role: "tool",
    kind,
    text: text.trim() ? text : null,
    language: null,
    command: null,
    cwd,
    status,
    exit_code: exitCode,
    metadata_json: metadata({ type: "result", format: "plain" }, value),
  }];
}

// ---------------------------------------------------------------------------
// 子代理：独立子会话 + 父轮次委派锚点（ADR-0022）
// ---------------------------------------------------------------------------

/** 子代理会话 `external_id` 的确定性分隔符：`<parentExternalId>:subagent:<agentId>` */
const SUBAGENT_EXTERNAL_ID_SEPARATOR = ":subagent:";
/** 内联子代理（无独立磁盘文件）的虚拟定位器标记 */
const SUBAGENT_INLINE_LOCATOR_MARKER = "#subagent:";
/** 汇报摘要上限：仅用于父轮次锚点，完整汇报留在子会话里 */
const SUBAGENT_REPORT_SUMMARY_MAX = 240;

/**
 * 合成子代理会话的确定性 `external_id`。
 * @param {string} parentExternalId - 父会话 external_id
 * @param {string} agentId - 子代理标识（子文件名去掉 `agent-` 前缀或行内 agentId）
 * @returns {string}
 */
function subagentExternalId(parentExternalId, agentId) {
  return `${parentExternalId}${SUBAGENT_EXTERNAL_ID_SEPARATOR}${agentId}`;
}

/**
 * 逆解析子代理会话 external_id。
 * @param {string} externalId
 * @returns {{parentExternalId: string, agentId: string}|null}
 */
function parseSubagentExternalId(externalId) {
  const text = String(externalId ?? "");
  const index = text.indexOf(SUBAGENT_EXTERNAL_ID_SEPARATOR);
  if (index <= 0) return null;
  const agentId = text.slice(index + SUBAGENT_EXTERNAL_ID_SEPARATOR.length);
  if (!agentId) return null;
  return { parentExternalId: text.slice(0, index), agentId };
}

function collapseWhitespace(text) {
  return String(text ?? "").replace(/\s+/g, " ").trim();
}

function summarizeReport(text, max = SUBAGENT_REPORT_SUMMARY_MAX) {
  const trimmed = String(text ?? "").trim();
  if (!trimmed) return null;
  if (trimmed.length <= max) return trimmed;
  return `${trimmed.slice(0, max).trim()}...`;
}

function anchorText(title, summary) {
  const head = collapseWhitespace(title) || "子代理任务";
  return summary ? `[委派任务] ${head}\n[汇报结论] ${summary}` : `[委派任务] ${head}`;
}

/**
 * 构造父轮次里的轻量委派锚点 Part。
 *
 * 锚点不复制子代理的执行过程，只留任务标题与结论摘要，并把子会话的确定性
 * `external_id` 写进 metadata，后端的 `stable_id` 会在入库时算出子会话主键。
 * @param {object} options
 * @returns {object} NormalizedConversationPart
 */
function subagentAnchorPart({ childExternalId, spawnAnchor, agentRole, title, status }) {
  const normalizedTitle = collapseWhitespace(title) || null;
  return {
    role: "assistant",
    kind: "subagent",
    text: anchorText(normalizedTitle, null),
    language: null,
    command: null,
    cwd: null,
    status: status ?? "running",
    exit_code: null,
    source_execution_id: spawnAnchor ?? null,
    metadata_json: JSON.stringify(compactObject({
      child_external_id: childExternalId,
      agent_role: agentRole ?? null,
      title: normalizedTitle,
      status: status ?? "running",
      spawn_anchor: spawnAnchor ?? null,
    })),
  };
}

function parseMetadataObject(part) {
  try {
    const parsed = JSON.parse(part?.metadata_json ?? "{}");
    return parsed && typeof parsed === "object" && !Array.isArray(parsed) ? parsed : {};
  } catch {
    return {};
  }
}

/**
 * 就地更新锚点：补汇报摘要、状态与步骤数，并同步刷新正文。
 * @param {object} part - 锚点 Part
 * @param {object} patch - 待合并的字段
 */
function updateSubagentAnchor(part, patch) {
  if (!part) return;
  const metadata = { ...parseMetadataObject(part), ...patch };
  for (const key of Object.keys(metadata)) {
    if (metadata[key] === null || metadata[key] === undefined) delete metadata[key];
  }
  part.metadata_json = JSON.stringify(metadata);
  part.text = anchorText(metadata.title, metadata.report_summary);
  if (metadata.status) part.status = metadata.status;
}

/**
 * 解析一段会话事件流为交互轮次。
 *
 * 与 ADR-0022 之前的实现相比：派发子代理不再切分轮次，也不再生成
 * `continuation` 续接伪轮次；Agent/Task 工具调用处只落一个锚点 Part，
 * 主代理在子代理返回后的所有动作都留在同一个父轮次里。
 * @param {string} text - 事件流行文本
 * @param {object} [options]
 * @param {string|null} [options.parentExternalId] - 父会话 external_id（用于锚点 id）
 * @param {Map<string, object>} [options.subagentMetaByToolUseId] - toolUseId → 子代理元数据
 * @param {string[]} [options.inlineAgentIds] - 内联子代理流标识（按出现顺序）
 * @returns {{turns: object[], projectPath: string|null}}
 */
function parseJsonl(text, options = {}) {
  const parentExternalId = options.parentExternalId ?? null;
  const subagentMetaByToolUseId = options.subagentMetaByToolUseId ?? new Map();
  const collectSidechain = options.collectSidechain === true;
  const ignoreSidechain = options.ignoreSidechain === true;
  const turns = [];
  let current = null;
  let projectPath = null;
  let pendingSubagentCallId = null;
  let pendingInlineAnchor = null;
  const pendingAnchors = new Map();
  const sidechain = new Map();

  for (const line of String(text ?? "").split(/\r?\n/)) {
    if (!line.trim()) continue;
    let value;
    try {
      value = JSON.parse(line);
    } catch {
      continue;
    }
    const payload = eventPayload(value);
    projectPath ??= projectPathFromValue(value) ?? projectPathFromValue(payload);
    const timestamp = stringField(value, ["timestamp", "created_at", "updated_at"]);
    const role = roleOf(payload) ?? roleOf(value);
    const recordType = stringField(payload, ["type"]) ?? stringField(value, ["type"]) ?? "";
    const userText = realUserText(value, payload, role, recordType);

    // 0. 内联子代理流：缓存到独立桶，稍后产出隐藏子会话
    if (value?.isSidechain === true && collectSidechain && !ignoreSidechain) {
      const agentId = stringField(value, ["agentId", "agent_id"]) ?? "inline";
      let bucket = sidechain.get(agentId);
      if (!bucket) {
        bucket = { rows: [], anchor: null, turn: current, insertAt: current?.parts.length ?? 0 };
        sidechain.set(agentId, bucket);
      }
      if (!bucket.anchor && pendingInlineAnchor) {
        bucket.anchor = pendingInlineAnchor;
        pendingInlineAnchor = null;
        updateSubagentAnchor(bucket.anchor, {
          child_external_id: parentExternalId
            ? subagentExternalId(parentExternalId, agentId)
            : agentId,
        });
      }
      bucket.rows.push(line);
      continue;
    }
    if (value?.isSidechain === true && !ignoreSidechain) continue;

    // 1. 真实人类提问 = 轮次边界
    if (userText) {
      if (current && current.parts.length > 0) turns.push(current);
      current = {
        external_id: stringField(payload, ["id", "uuid"]) ?? stringField(value, ["id", "uuid"]) ?? `turn-${turns.length}`,
        turn_index: turns.length,
        user_text: userText,
        title: null,
        started_at: timestamp,
        ended_at: null,
        model: null,
        parts: [],
      };
      pendingSubagentCallId = null;
      pendingInlineAnchor = null;
      continue;
    }

    // 2. 派发子代理的工具结果：回填锚点，不再额外落一份结果零件
    if (pendingSubagentCallId && isUserToolResultMessage(payload)) {
      const toolResults = contentItemsOfType(payload, "tool_result");
      const matched = toolResults.find(
        (item) => item.tool_use_id === pendingSubagentCallId || item.id === pendingSubagentCallId,
      );
      if (matched) {
        const outputText = toolText(matched)
          || (typeof matched?.content === "string" ? matched.content : "")
          || extractText(matched)
          || "";
        updateSubagentAnchor(pendingAnchors.get(pendingSubagentCallId), {
          status: "completed",
          report_summary: summarizeReport(outputText),
        });
        pendingAnchors.delete(pendingSubagentCallId);
        pendingSubagentCallId = null;
        if (current) current.ended_at = timestamp;
        continue;
      }
    }

    if (!current) continue;

    if (isUserToolResultMessage(payload)) {
      const resultParts = contentItemsOfType(payload, "tool_result").flatMap((item) => toolResultPart(item, value));
      current.parts.push(...resultParts);
      current.ended_at = timestamp;
      continue;
    }

    if (role === "assistant") {
      const agentTool = Array.isArray(payload?.content)
        ? payload.content.find((item) => item?.type === "tool_use" && (item.name === "Agent" || item.name === "Task"))
        : null;

      if (agentTool) {
        const preItems = payload.content.filter((item) => item !== agentTool);
        if (preItems.length > 0) {
          current.parts.push(...assistantMessageParts({ ...payload, content: preItems }, value));
        }
        const spawnAnchor = agentTool.id ?? null;
        const inputObj = agentTool.input || {};
        const meta = spawnAnchor ? subagentMetaByToolUseId.get(spawnAnchor) : null;
        const agentId = meta?.agentId ?? spawnAnchor ?? `subagent-${current.parts.length}`;
        const childExternalId = parentExternalId
          ? subagentExternalId(parentExternalId, agentId)
          : String(agentId);
        const title = meta?.description
          ?? inputObj.description
          ?? inputObj.subagent_type
          ?? inputObj.role
          ?? agentTool.name;
        const anchor = subagentAnchorPart({
          childExternalId,
          spawnAnchor,
          agentRole: meta?.agentType ?? inputObj.subagent_type ?? inputObj.role ?? agentTool.name,
          title,
          status: "running",
        });
        current.parts.push(anchor);
        if (spawnAnchor) {
          pendingAnchors.set(spawnAnchor, anchor);
          pendingSubagentCallId = spawnAnchor;
        }
        // 没有 meta.json 时子代理身份只能从随后的内联流里得知
        if (!meta) pendingInlineAnchor = anchor;
        current.ended_at = timestamp;
        continue;
      }

      current.parts.push(...assistantMessageParts(payload, value));
      current.ended_at = timestamp;
      if (!current.model) {
        current.model = stringField(payload?.message, ["model"]) ?? stringField(payload, ["model"]) ?? stringField(value?.message, ["model"]) ?? stringField(value, ["model"]) ?? null;
      }
      continue;
    }

    if (recordType.includes("tool") || recordType.includes("shell") || recordType === "patch") {
      current.parts.push(...toolPart(payload));
      current.ended_at = timestamp;
    }
  }
  if (current && current.parts.length > 0) turns.push(current);

  // 没有对应 Agent 工具调用的内联子代理：在它出现的位置补一个锚点，
  // 保证"派发必有锚点"这一契约在纯内联宿主数据上也成立。
  for (const [agentId, bucket] of sidechain) {
    if (bucket.anchor || !bucket.turn) continue;
    const promptText = bucket.rows
      .map((raw) => {
        try {
          const parsed = JSON.parse(raw);
          return realUserText(parsed, eventPayload(parsed), roleOf(eventPayload(parsed)) ?? roleOf(parsed), "user");
        } catch {
          return null;
        }
      })
      .find(Boolean);
    const anchor = subagentAnchorPart({
      childExternalId: parentExternalId ? subagentExternalId(parentExternalId, agentId) : agentId,
      spawnAnchor: null,
      agentRole: "Sidechain Subagent",
      title: promptText ? sanitizeSessionShortTitle(promptText) : "Sidechain Subagent",
      status: "completed",
    });
    bucket.turn.parts.splice(Math.min(bucket.insertAt, bucket.turn.parts.length), 0, anchor);
    bucket.anchor = anchor;
  }

  for (let i = 0; i < turns.length; i++) {
    turns[i].turn_index = i;
  }
  return { turns, projectPath, sidechain };
}

// ---------------------------------------------------------------------------
// 子代理文件发现与子会话组装
// ---------------------------------------------------------------------------

/**
 * 列出某个父会话文件对应的子代理文件。
 *
 * 兼容两种宿主布局：`<sessionId>/subagents/agent-*.jsonl`（现行）与
 * `<同目录>/subagents/*.jsonl`（早期版本与测试夹具）。
 * @param {string} parentFilePath
 * @returns {string[]}
 */
function discoverSubagentFiles(parentFilePath) {
  if (!parentFilePath) return [];
  const sessionId = path.basename(parentFilePath, ".jsonl");
  const dir = path.dirname(parentFilePath);
  const candidates = [
    path.join(dir, sessionId, "subagents"),
    path.join(dir, "subagents"),
  ];
  const files = new Set();
  for (const subagentsDir of candidates) {
    if (!existsSync(subagentsDir)) continue;
    try {
      for (const entry of readdirSync(subagentsDir, { withFileTypes: true })) {
        if (!entry.isFile() || !entry.name.endsWith(".jsonl")) continue;
        files.add(path.join(subagentsDir, entry.name));
      }
    } catch {}
  }
  return [...files];
}

function agentIdFromFilePath(agentFile) {
  const stem = path.basename(agentFile, ".jsonl");
  return stem.startsWith("agent-") ? stem.slice("agent-".length) : stem;
}

function readSubagentMeta(agentFile) {
  const metaPath = agentFile.replace(/\.jsonl$/, ".meta.json");
  if (!existsSync(metaPath)) return null;
  try {
    return JSON.parse(readFileSync(metaPath, "utf8"));
  } catch {
    return null;
  }
}

function subagentMetaIndex(agentFiles) {
  const map = new Map();
  for (const file of agentFiles) {
    const meta = readSubagentMeta(file);
    if (meta?.toolUseId) map.set(meta.toolUseId, { ...meta, agentId: agentIdFromFilePath(file) });
  }
  return map;
}

/**
 * 解析子代理文件所属的父会话 external_id。
 *
 * 子代理文件内部每一行的 `sessionId` 与父会话相同（这是宿主既有事实），
 * 因此优先取行内 `sessionId`；取不到时退回目录名推断。
 * @param {string} agentFile
 * @returns {string}
 */
function resolveParentExternalIdForSubagent(agentFile) {
  try {
    const { text } = readStableFile(agentFile);
    for (const line of text.split(/\r?\n/)) {
      if (!line.trim()) continue;
      let value;
      try {
        value = JSON.parse(line);
      } catch {
        continue;
      }
      const sessionId = stringField(value, ["sessionId", "session_id"]);
      if (sessionId) return sessionId;
    }
  } catch {}
  const subagentsDir = path.dirname(agentFile);
  if (path.basename(subagentsDir) === "subagents") {
    const parentDir = path.dirname(subagentsDir);
    const candidate = path.basename(parentDir);
    if (candidate && existsSync(path.join(path.dirname(parentDir), `${candidate}.jsonl`))) {
      return candidate;
    }
  }
  return path.basename(agentFile, ".jsonl");
}

function summarizeChildSession(turns) {
  let stepCount = 0;
  let reportSummary = null;
  for (const turn of turns) {
    for (const part of turn.parts ?? []) {
      if (part.kind === "command" || part.kind === "tool" || part.kind === "file_change") stepCount += 1;
      if (part.role === "assistant" && (part.kind === "text" || part.kind === "code") && part.text?.trim()) {
        reportSummary = part.text.trim();
      }
    }
  }
  return { stepCount, reportSummary: summarizeReport(reportSummary) };
}

/**
 * 把一个子代理文件产出为独立的 `Subagent Session`。
 * @param {string} agentFile
 * @param {string} parentExternalId
 * @returns {object|null}
 */
function buildSubagentSession(agentFile, parentExternalId) {
  const agentId = agentIdFromFilePath(agentFile);
  const meta = readSubagentMeta(agentFile);
  const { text, versionToken } = readStableFile(agentFile);
  const parsed = parseJsonl(text, { parentExternalId, ignoreSidechain: true });
  const turns = displayTurns(parsed.turns);
  if (!turns.length) return null;
  const projectPath = parsed.projectPath ?? inferProjectPathFromTurns(turns);
  const title = collapseWhitespace(meta?.description)
    || resolveSessionTitle(parsed, turns, agentFile);
  return {
    external_id: subagentExternalId(parentExternalId, agentId),
    title,
    project_path: projectPath,
    started_at: turns[0]?.started_at ?? null,
    updated_at: turns.at(-1)?.ended_at ?? turns.at(-1)?.started_at ?? null,
    source_locator: agentFile,
    source_fingerprint: versionToken,
    user_visible: false,
    execution_origin: "subagent",
    execution_purpose: JSON.stringify(compactObject({
      parent_external_id: parentExternalId,
      agent_role: meta?.agentType ?? null,
      spawn_anchor: meta?.toolUseId ?? null,
    })),
    turns,
  };
}

/**
 * 把内联 `isSidechain` 事件流产出为独立的 `Subagent Session`。
 * @param {string} agentId
 * @param {string[]} rawRows
 * @param {string} parentExternalId
 * @param {string} parentFilePath
 * @returns {object|null}
 */
function buildInlineSubagentSession(agentId, rawRows, parentExternalId, parentFilePath) {
  const parsed = parseJsonl(rawRows.join("\n"), { parentExternalId, ignoreSidechain: true });
  const turns = displayTurns(parsed.turns);
  if (!turns.length) return null;
  return {
    external_id: subagentExternalId(parentExternalId, agentId),
    title: resolveSessionTitle(parsed, turns, parentFilePath),
    project_path: parsed.projectPath ?? inferProjectPathFromTurns(turns),
    started_at: turns[0]?.started_at ?? null,
    updated_at: turns.at(-1)?.ended_at ?? turns.at(-1)?.started_at ?? null,
    source_locator: `${parentFilePath}${SUBAGENT_INLINE_LOCATOR_MARKER}${agentId}`,
    source_fingerprint: null,
    user_visible: false,
    execution_origin: "subagent",
    execution_purpose: JSON.stringify(compactObject({
      parent_external_id: parentExternalId,
      agent_role: "Sidechain Subagent",
      spawn_anchor: null,
    })),
    turns,
  };
}

/**
 * 用子会话内容回填父轮次锚点：补角色、标题、步骤数与汇报摘要，
 * 并把父轮次序号写回子会话的 `execution_purpose`。
 * @param {object[]} turns - 父会话轮次
 * @param {object[]} children - 子会话列表
 */
function linkChildrenToAnchors(turns, children) {
  if (!children.length) return;
  const byExternalId = new Map(children.map((child) => [child.external_id, child]));
  for (const turn of turns) {
    for (const part of turn.parts ?? []) {
      if (part.kind !== "subagent") continue;
      const metadata = parseMetadataObject(part);
      const child = byExternalId.get(metadata.child_external_id);
      if (!child) continue;
      const { stepCount, reportSummary } = summarizeChildSession(child.turns);
      // 子代理自己的最终汇报才是权威来源；父文件里那条 tool_result 往往只是
      // "Async agent launched successfully" 之类的启动回执，不能拿去当结论摘要。
      updateSubagentAnchor(part, {
        agent_role: metadata.agent_role ?? null,
        title: metadata.title ?? child.title ?? null,
        status: metadata.status ?? "completed",
        model: child.turns.map((entry) => entry.model).find(Boolean) ?? null,
        step_count: stepCount,
        report_summary: reportSummary ?? metadata.report_summary ?? null,
      });
      if (metadata.spawn_anchor) {
        const purpose = JSON.parse(child.execution_purpose ?? "{}");
        purpose.parent_turn_index = turn.turn_index;
        child.execution_purpose = JSON.stringify(purpose);
      }
    }
  }
}

/**
 * 解析一个父会话文件：产出父会话本身 + 它名下全部子代理会话。
 * @param {string} filePath
 * @returns {object[]}
 */
function buildParentSessionWithChildren(filePath) {
  const parentExternalId = path.basename(filePath, ".jsonl") || "claude-session";
  const agentFiles = discoverSubagentFiles(filePath);
  const { text, versionToken } = readStableFile(filePath);
  const parsed = parseJsonl(text, {
    parentExternalId,
    subagentMetaByToolUseId: subagentMetaIndex(agentFiles),
    collectSidechain: true,
  });
  const turns = displayTurns(parsed.turns);
  if (!turns.length) return [];
  const projectPath = parsed.projectPath ?? inferProjectPathFromTurns(turns);
  if (isInternalExecutionPath(projectPath)) return [];

  const children = agentFiles
    .map((agentFile) => buildSubagentSession(agentFile, parentExternalId))
    .filter(Boolean);
  for (const [agentId, bucket] of parsed.sidechain) {
    const inline = buildInlineSubagentSession(agentId, bucket.rows, parentExternalId, filePath);
    if (inline) children.push(inline);
  }
  linkChildrenToAnchors(turns, children);

  return [{
    external_id: parentExternalId,
    title: resolveSessionTitle(parsed, turns, filePath),
    project_path: projectPath,
    started_at: turns[0]?.started_at ?? null,
    updated_at: turns.at(-1)?.ended_at ?? turns.at(-1)?.started_at ?? null,
    source_locator: filePath,
    source_fingerprint: versionToken,
    user_visible: true,
    execution_origin: "user",
    execution_purpose: null,
    turns,
  }, ...children];
}

function resolvePrompt(...candidates) {
  for (const c of candidates) {
    if (typeof c === "string" && c.trim().length > 0) return c.trim();
  }
  return "Subagent execution";
}

function displayTurns(turns) {
  // 宿主注入的上下文行（如 "Repository context only"）没有内容事实，若后面还有
  // 真实提问轮次就不能占一个物理轮次号；但最后一轮即使是尚未回答的提问也要保留，
  // 否则 "1 次人类提问 = 1 个 Turn" 和占位子会话都会丢。
  const lastIndex = turns.length - 1;
  return turns
    .filter((turn, index) => (Array.isArray(turn.parts) && turn.parts.length > 0) || index === lastIndex)
    .map((turn, index) => ({
      ...turn,
      turn_index: index,
    }));
}

function inferProjectPathFromTurns(turns) {
  for (const turn of turns) {
    for (const part of turn.parts) {
      if (part.cwd?.trim()) return part.cwd.trim();
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

function titleFromFile(filePath) {
  const parentName = path.basename(path.dirname(filePath));
  return parentName ? parentName.replaceAll("-", "/") : null;
}

function resolveSessionTitle(parsed, turns, filePath) {
  if (parsed?.title?.trim()) return parsed.title.trim();
  if (parsed?.summary?.trim()) return sanitizeSessionShortTitle(parsed.summary);
  const promptTitle = sanitizeSessionShortTitle(turns[0]?.user_text);
  if (promptTitle) return promptTitle;
  return titleFromFile(filePath);
}

function readSession() {
  const location = expandPath(input.source?.location);
  if (!location || !existsSync(location)) return [];
  const requestedSessionId = input.params?.session_id ?? null;
  const requestedLocator = input.params?.source_locator ? expandPath(input.params.source_locator) : null;

  // 子代理文件只产出它自己那个隐藏子会话，父会话由父文件负责产出。
  if (requestedLocator && existsSync(requestedLocator)) {
    if (isSubagentSessionPath(requestedLocator)) {
      const child = buildSubagentSession(
        requestedLocator,
        resolveParentExternalIdForSubagent(requestedLocator),
      );
      return child ? [child] : [];
    }
    return buildParentSessionWithChildren(requestedLocator);
  }

  if (requestedSessionId) {
    const directCandidate = path.join(location, `${requestedSessionId}.jsonl`);
    if (existsSync(directCandidate)) {
      return buildParentSessionWithChildren(directCandidate);
    }
  }

  const matchedFiles = collectJsonlFiles(location).filter((filePath) =>
    !requestedSessionId || path.basename(filePath, ".jsonl") === String(requestedSessionId)
  );
  const seen = new Set();
  const sessions = [];
  for (const filePath of matchedFiles) {
    const batch = isSubagentSessionPath(filePath)
      ? [buildSubagentSession(filePath, resolveParentExternalIdForSubagent(filePath))].filter(Boolean)
      : buildParentSessionWithChildren(filePath);
    for (const session of batch) {
      if (seen.has(session.external_id)) continue;
      seen.add(session.external_id);
      sessions.push(session);
    }
  }
  return sessions;
}

function isInternalExecutionPath(cwd) {
  if (!cwd || typeof cwd !== "string") return false;
  const norm = cwd.replace(/\\/g, "/").toLowerCase();
  return (
    norm.includes("/agent-executions/") ||
    norm.includes("/agent-runs/") ||
    norm.includes("/.assetiweave/") ||
    norm.includes("agent-executions") ||
    norm.includes("agent-runs") ||
    norm.includes(".assetiweave")
  );
}

function listSessions() {
  const location = expandPath(input.source?.location);
  if (!location || !existsSync(location)) return [];
  // 子代理文件也要出现在描述符里：增量同步靠这份集合给缺失会话打 missing 标记，
  // 漏掉它们会让隐藏子会话每轮都被误判为"源已消失"。
  return collectJsonlFiles(location).map((filePath) => {
    const stat = statSync(filePath);
    const externalId = isSubagentSessionPath(filePath)
      ? subagentExternalId(
        resolveParentExternalIdForSubagent(filePath),
        agentIdFromFilePath(filePath),
      )
      : path.basename(filePath, ".jsonl") || "claude-session";
    return {
      external_id: externalId,
      updated_at: stat.mtime.toISOString(),
      source_locator: filePath,
      version_token: fileVersionToken(filePath),
    };
  });
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
  const header = /^(?:Created At:.*|Completed At:.*|Script completed(?: successfully)?(?: in .*)?|Wall time\b.*|Chunk ID:.*|Process exited with code\b.*|Original token count:.*|The command completed successfully\.?|(?:Final |Original |Command )?Output:)$/i;
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
          kind: `claude-code.${legacyCard.type}`,
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
  return "plain";
}

/**
 * 【Token 用量提取入口】：供 Rust 侧 `read_usage` IPC 方法调用。
 * 遍历 Claude Code 转录和项目日志 (.jsonl)，提取每个模型的实际 Token 消耗。
 */
function readUsage() {
  const location = expandPath(input.source?.location);
  if (!location || !existsSync(location)) {
    emit("complete", { item: { snapshot_complete: true, usage_event_count: 0, next_cursor: null, decoder_profile: "claude-code-transcript-v1", diagnostics: [] } });
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
  const processedFiles = new Set(cursor?.processed_files || []);

  const files = collectJsonlFiles(location);
  let maxMtime = cursorMtime;
  let eventCount = 0;
  const newProcessedFiles = [];

  for (const filePath of files) {
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

      const sessionId = path.basename(filePath, ".jsonl");
      const text = readFileSync(filePath, "utf8");
      const lines = text.split("\n");

      let turnIndex = -1;
      let currentTurn = null;

      const flushTurn = () => {
        if (!currentTurn) return;
        const totalTokens =
          currentTurn.inputTokens +
          currentTurn.outputTokens +
          currentTurn.cacheReadTokens +
          currentTurn.cacheWriteTokens +
          currentTurn.reasoningTokens;

        if (totalTokens > 0) {
          const turnId = `turn-${currentTurn.turnIndex}`;
          const eventId = `${sessionId}-${turnId}`;
          const timestamp =
            currentTurn.firstResponseTimestamp ||
            currentTurn.userTimestamp ||
            stat.mtime.toISOString();

          const event = {
            external_event_id: eventId,
            session_id: sessionId,
            turn_id: turnId,
            logical_request_id: eventId,
            attempt_index: 0,
            timestamp,
            provider: "anthropic",
            model: currentTurn.model || "claude-3-7-sonnet-20250219",
            input_tokens: currentTurn.inputTokens,
            output_tokens: currentTurn.outputTokens,
            cache_read_tokens: currentTurn.cacheReadTokens,
            cache_write_tokens: currentTurn.cacheWriteTokens,
            reasoning_tokens: currentTurn.reasoningTokens,
            total_tokens: totalTokens,
            status: "success",
            currency: null,
            cost: null,
            metadata: {
              file: path.basename(filePath),
            },
          };
          emit("usage_event", { usage_event: event });
          eventCount++;
        }
        currentTurn = null;
      };

      for (const line of lines) {
        if (!line.trim()) continue;
        let p;
        try {
          p = JSON.parse(line);
        } catch {
          continue;
        }

        const payload = eventPayload(p);
        const role = roleOf(payload) ?? roleOf(p);
        const recordType = stringField(payload, ["type"]) ?? stringField(p, ["type"]) ?? "";
        const userText = realUserText(p, payload, role, recordType);
        const isUser = Boolean(userText) || (
          !p.isSidechain &&
          (role === "user" || recordType === "user" || p.type === "user") &&
          !isUserToolResultMessage(payload) &&
          !isIgnoredContentValue(p) &&
          !isIgnoredContentValue(payload)
        );

        if (isUser && !p.isSidechain) {
          flushTurn();
          turnIndex++;
          let userTs = null;
          const rawTs = p.timestamp || p.created_at;
          if (rawTs) {
            try {
              userTs = new Date(rawTs).toISOString();
            } catch {}
          }
          currentTurn = {
            turnIndex,
            userTimestamp: userTs,
            firstResponseTimestamp: null,
            model: null,
            inputTokens: 0,
            outputTokens: 0,
            cacheReadTokens: 0,
            cacheWriteTokens: 0,
            reasoningTokens: 0,
          };
          continue;
        }

        const usage = p.message?.usage || p.usage || p.payload?.usage || p.item?.usage;
        const inputTokens = Number(usage?.input_tokens || 0);
        const outputTokens = Number(usage?.output_tokens || 0);
        const cacheReadTokens = Number(usage?.cache_read_input_tokens || usage?.cache_read_tokens || 0);
        const cacheWriteTokens = Number(usage?.cache_creation_input_tokens || usage?.cache_write_tokens || 0);
        const reasoningTokens = Number(usage?.reasoning_tokens || 0);
        const hasTokens = (inputTokens + outputTokens + cacheReadTokens + cacheWriteTokens + reasoningTokens) > 0;

        if (!currentTurn) {
          if (hasTokens) {
            turnIndex++;
            let initTs = null;
            const rawTs = p.timestamp || p.created_at;
            if (rawTs) {
              try {
                initTs = new Date(rawTs).toISOString();
              } catch {}
            }
            currentTurn = {
              turnIndex,
              userTimestamp: initTs,
              firstResponseTimestamp: null,
              model: null,
              inputTokens: 0,
              outputTokens: 0,
              cacheReadTokens: 0,
              cacheWriteTokens: 0,
              reasoningTokens: 0,
            };
          } else {
            continue;
          }
        }

        const modelStr = String(p.message?.model || p.model || p.payload?.model || "");
        if (modelStr && !currentTurn.model) {
          currentTurn.model = modelStr;
        }

        const respRawTs = p.timestamp || p.created_at || p.message?.created_at;
        if (!currentTurn.firstResponseTimestamp && respRawTs) {
          try {
            currentTurn.firstResponseTimestamp = new Date(respRawTs).toISOString();
          } catch {}
        }

        if (usage) {
          currentTurn.inputTokens += inputTokens;
          currentTurn.outputTokens += outputTokens;
          currentTurn.cacheReadTokens += cacheReadTokens;
          currentTurn.cacheWriteTokens += cacheWriteTokens;
          currentTurn.reasoningTokens += reasoningTokens;
        }
      }
      flushTurn();
      newProcessedFiles.push(filePath);
    } catch {}
  }

  emit("complete", {
    item: {
      snapshot_complete: true,
      usage_event_count: eventCount,
      next_cursor: JSON.stringify({
        last_mtime: maxMtime,
        processed_files: newProcessedFiles.slice(-2000),
      }),
      decoder_profile: "claude-code-transcript-v1",
      diagnostics: [],
    },
  });
}

try {
  if (input.method === "project_command_parts") {
    const projections = projectCommandParts(input.params?.parts ?? input.params?.command_parts);
    for (const projection of projections) emit("item", { item: { kind: "command_projection", ...projection } });
    emit("complete", { item: { projection_count: projections.length, projector_version: SHELL_PROJECTOR_VERSION } });
  } else if (input.method === "probe") {
    emit("complete", { item: { session_count: 0 } });
  } else if (input.method === "list_sessions") {
    emitProgress({ stage: "reading", operation: "list_sessions" });
    const descriptors = listSessions();
    for (let i = 0; i < descriptors.length; i += 1) {
      const descriptor = descriptors[i];
      if (i === 0 || i === descriptors.length - 1 || (i + 1) % 10 === 0) {
        emitProgress({
          stage: "reading",
          operation: "list_sessions",
          current: i + 1,
          total: descriptors.length,
          path: descriptor.external_id,
        });
      }
      emit("item", { item: { kind: "session_descriptor", ...descriptor } });
    }
    emit("complete", { item: { session_count: descriptors.length, snapshot_complete: true } });
  } else if (input.method === "read_session") {
    emitProgress({ stage: "reading", operation: "read_session" });
    const sessions = readSession();
    for (let i = 0; i < sessions.length; i += 1) {
      const session = sessions[i];
      emitProgress({
        stage: "reading",
        operation: "read_session",
        current: i + 1,
        total: sessions.length,
        path: session.external_id,
      });
      emit("item", { item: { kind: "session", session: finalizeStructuredContentCards(session) } });
    }
    emit("complete", { item: { session_count: sessions.length } });
  } else if (input.method === "read_usage") {
    emitProgress({ stage: "reading", operation: "read_usage" });
    readUsage();
  } else {
    fail(`unsupported method: ${input.method}`);
  }
} catch (error) {
  fail(error instanceof Error ? error.message : String(error));
}
