#!/usr/bin/env node
import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { homedir } from "node:os";
import path from "node:path";

const input = JSON.parse(readFileSync(0, "utf8") || "{}");
const CONTENT_CARD_SCHEMA_VERSION = "claude-code-content-cards-v7";

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

function toolUsePart(value) {
  const toolName = stringField(value, ["tool_name", "toolName", "name"]);
  const input = toolInput(value);
  const command = stringField(input, ["command"]) ?? stringField(value, ["command", "cmd"]);
  const cwd = projectPathFromValue(input) ?? projectPathFromValue(value) ?? null;
  if (command?.trim()) {
    return [{
      role: "tool",
      kind: "command",
      text: null,
      language: null,
      command,
      cwd,
      status: null,
      exit_code: null,
      source_execution_id: sourceExecutionId(value),
      metadata_json: metadata(compactObject({ type: "command", cwd }), compactToolMetadata(value)),
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
    metadata_json: metadata({ type: "tool", format: input && typeof input === "object" ? "json" : "plain" }, compactToolMetadata(value)),
  }];
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
  const text = toolResultText(value, parent);
  if (!text.trim()) return [];
  const result = parent?.toolUseResult ?? parent?.tool_use_result;
  const status = stringField(result, ["status"]) ?? (result?.interrupted === true ? "interrupted" : null);
  const exitCode = Number.isInteger(result?.exit_code)
    ? result.exit_code
    : Number.isInteger(result?.exitCode)
      ? result.exitCode
      : null;
  return [{
    role: "tool",
    kind: "tool",
    text,
    language: null,
    command: null,
    cwd: null,
    status,
    exit_code: exitCode,
    source_execution_id: sourceExecutionId(value),
    metadata_json: metadata(
      compactObject({ type: "result", format: "plain", status, exit_code: exitCode }),
      compactToolMetadata(value),
    ),
  }];
}

function assistantMessageParts(payload, parent) {
  if (!Array.isArray(payload?.content)) {
    const text = extractText(payload);
    return text.trim() ? splitMarkdownParts("assistant", text) : [];
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
    if (text.trim()) parts.push(...splitMarkdownParts("assistant", text));
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
    const executionId = sourceExecutionId(value);
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

function parseJsonl(text) {
  const turns = [];
  let current = null;
  let projectPath = null;
  for (const line of text.split(/\r?\n/)) {
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
    if (value.isSidechain === true) {
      const text = extractText(payload);
      if (current && text.trim()) {
        current.parts.push({
          role: "assistant",
          kind: "subagent",
          text,
          language: null,
          command: null,
          cwd: null,
          status: null,
          exit_code: null,
          metadata_json: metadata({ type: "result", format: "plain" }, value),
        });
      }
      continue;
    }
    const role = roleOf(payload) ?? roleOf(value);
    const recordType = stringField(payload, ["type"]) ?? stringField(value, ["type"]) ?? "";
    const userText = realUserText(value, payload, role, recordType);
    if (userText) {
      if (current) turns.push(current);
      current = {
        external_id: stringField(payload, ["id", "uuid"]) ?? stringField(value, ["id", "uuid"]) ?? `turn-${turns.length}`,
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
    if (isUserToolResultMessage(payload)) {
      const resultParts = contentItemsOfType(payload, "tool_result").flatMap((item) => toolResultPart(item, value));
      current.parts.push(...resultParts);
      current.ended_at = timestamp;
      continue;
    }
    if (role === "assistant") {
      current.parts.push(...assistantMessageParts(payload, value));
      current.ended_at = timestamp;
      continue;
    }
    if (recordType.includes("tool") || recordType.includes("shell") || recordType === "patch") {
      current.parts.push(...toolPart(payload));
      current.ended_at = timestamp;
    }
  }
  if (current) turns.push(current);
  return { turns, projectPath };
}

function displayTurns(turns) {
  return turns
    .filter((turn) => Array.isArray(turn.parts) && turn.parts.length > 0)
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

function titleFromFile(filePath) {
  const parentName = path.basename(path.dirname(filePath));
  return parentName ? parentName.replaceAll("-", "/") : null;
}

function readSession() {
  const location = expandPath(input.source?.location);
  if (!location || !existsSync(location)) return [];
  const requestedSessionId = input.params?.session_id ?? null;
  return collectJsonlFiles(location).filter((filePath) =>
    !requestedSessionId || path.basename(filePath, ".jsonl") === String(requestedSessionId)
  ).flatMap((filePath) => {
    const { text, versionToken } = readStableFile(filePath);
    const parsed = parseJsonl(text);
    const turns = displayTurns(parsed.turns);
    if (!turns.length) return [];
    return [{
      external_id: path.basename(filePath, ".jsonl") || "claude-session",
      title: titleFromFile(filePath),
      project_path: parsed.projectPath ?? inferProjectPathFromTurns(turns),
      started_at: turns[0]?.started_at ?? null,
      updated_at: turns.at(-1)?.ended_at ?? null,
      source_locator: filePath,
      source_fingerprint: versionToken,
      turns,
    }];
  });
}

function listSessions() {
  const location = expandPath(input.source?.location);
  if (!location || !existsSync(location)) return [];
  return collectJsonlFiles(location).map((filePath) => {
    const stat = statSync(filePath);
    return {
      external_id: path.basename(filePath, ".jsonl") || "claude-session",
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
  return session;
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

try {
  if (input.method === "probe") {
    emit("complete", { item: { session_count: 0 } });
  } else if (input.method === "list_sessions") {
    const descriptors = listSessions();
    for (const descriptor of descriptors) emit("item", { item: { kind: "session_descriptor", ...descriptor } });
    emit("complete", { item: { session_count: descriptors.length, snapshot_complete: true } });
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
