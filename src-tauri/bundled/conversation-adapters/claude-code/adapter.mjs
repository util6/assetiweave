#!/usr/bin/env node
import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { homedir } from "node:os";
import path from "node:path";

const input = JSON.parse(readFileSync(0, "utf8") || "{}");

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

function compact(value) {
  return JSON.stringify(value);
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

function collectJsonlFiles(root, limit = 1000) {
  if (!existsSync(root)) return [];
  if (statSync(root).isFile()) return root.endsWith(".jsonl") ? [root] : [];
  const files = [];
  const stack = [root];
  while (stack.length && files.length < limit) {
    const dir = stack.pop();
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const fullPath = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        stack.push(fullPath);
      } else if (entry.isFile() && entry.name.endsWith(".jsonl")) {
        files.push(fullPath);
        if (files.length >= limit) break;
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

function toolPart(value) {
  const recordType = stringField(value, ["type"]) ?? "";
  const toolName = stringField(value, ["tool_name", "toolName", "name"]);
  const command = stringField(value?.tool_input, ["command"]) ?? stringField(value, ["command", "cmd"]);
  const text = toolText(value) || (toolName ? `${recordType || "tool"}: ${toolName}` : "");
  if (!command && !text.trim()) return null;
  const lowerName = (toolName ?? "").toLowerCase();
  const kind = lowerName.includes("patch") || lowerName.includes("edit") || recordType === "patch"
    ? "file_change"
    : command || recordType.includes("shell")
      ? "command"
      : "tool";
  return {
    role: "tool",
    kind,
    text: text.trim() ? text : null,
    language: null,
    command,
    cwd: projectPathFromValue(value?.tool_input) ?? projectPathFromValue(value) ?? null,
    status: stringField(value, ["status"]) ?? null,
    exit_code: Number.isInteger(value?.exit_code) ? value.exit_code : null,
    metadata_json: metadata(
      kind === "command" ? { type: "command" } : { type: "result", format: "plain" },
      value,
    ),
  };
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
      const part = toolPart(payload);
      if (part) current.parts.push(part);
      current.ended_at = timestamp;
      continue;
    }
    if (role === "assistant") {
      const text = extractText(payload);
      if (text.trim()) {
        current.parts.push(...splitMarkdownParts("assistant", text));
      }
      current.ended_at = timestamp;
      continue;
    }
    if (recordType.includes("tool") || recordType.includes("shell") || recordType === "patch") {
      const part = toolPart(payload);
      if (part) current.parts.push(part);
      current.ended_at = timestamp;
    }
  }
  if (current) turns.push(current);
  return { turns, projectPath };
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
  return collectJsonlFiles(location).flatMap((filePath) => {
    const text = readFileSync(filePath, "utf8");
    const parsed = parseJsonl(text);
    const turns = parsed.turns;
    if (!turns.length) return [];
    return [{
      external_id: path.basename(filePath, ".jsonl") || "claude-session",
      title: titleFromFile(filePath),
      project_path: parsed.projectPath ?? inferProjectPathFromTurns(turns),
      started_at: turns[0]?.started_at ?? null,
      updated_at: turns.at(-1)?.ended_at ?? null,
      source_locator: filePath,
      source_fingerprint: sha256(text),
      turns,
    }];
  });
}

try {
  if (input.method === "probe" || input.method === "list_sessions") {
    emit("complete", { item: { session_count: 0 } });
  } else if (input.method === "read_session") {
    const sessions = readSession();
    for (const session of sessions) emit("item", { item: { kind: "session", session } });
    emit("complete", { item: { session_count: sessions.length } });
  } else {
    fail(`unsupported method: ${input.method}`);
  }
} catch (error) {
  fail(error instanceof Error ? error.message : String(error));
}
