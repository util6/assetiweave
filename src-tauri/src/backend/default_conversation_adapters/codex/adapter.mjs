#!/usr/bin/env node
import { createHash } from "node:crypto";
import { existsSync, readFileSync, statSync } from "node:fs";
import { homedir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

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

function sqliteJson(dbPath, sql) {
  const result = spawnSync("sqlite3", ["-json", dbPath, sql], { encoding: "utf8" });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error((result.stderr || `sqlite3 exited with ${result.status}`).trim());
  }
  const text = result.stdout.trim();
  return text ? JSON.parse(text) : [];
}

function quoteIdent(name) {
  return `"${String(name).replaceAll("\"", "\"\"")}"`;
}

function pick(columns, candidates) {
  return candidates.find((name) => columns.includes(name));
}

function contentText(content) {
  if (typeof content === "string") return content;
  if (!Array.isArray(content)) return "";
  return content
    .map((item) => {
      if (typeof item === "string") return item;
      return item?.text ?? item?.content ?? "";
    })
    .filter(Boolean)
    .join("\n\n");
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

function valueAsString(value) {
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return null;
}

function metadata(contentCard, extra = {}) {
  return JSON.stringify({
    ...(extra && typeof extra === "object" && !Array.isArray(extra) ? extra : {}),
    content_card: contentCard,
  });
}

function directStringField(value, names) {
  if (!value || typeof value !== "object") return null;
  for (const name of names) {
    const text = valueAsString(value[name]);
    if (text?.trim()) return text;
  }
  return null;
}

function nestedStringField(value, names, depth = 0) {
  if (depth > 6 || !value || typeof value !== "object") return null;
  const direct = directStringField(value, names);
  if (direct) return direct;
  for (const key of ["arguments", "args"]) {
    const child = value[key];
    if (child == null) continue;
    const parsed = parseJsonValue(child);
    const nested = nestedStringField(parsed ?? child, names, depth + 1);
    if (nested) return nested;
  }
  for (const key of ["action", "input", "tool_input", "toolInput", "state", "request", "params", "parameters"]) {
    const nested = nestedStringField(value[key], names, depth + 1);
    if (nested) return nested;
  }
  return null;
}

function nestedIntegerField(value, names, depth = 0) {
  if (depth > 6 || !value || typeof value !== "object") return null;
  for (const name of names) {
    const child = value[name];
    if (Number.isInteger(child)) return child;
    if (typeof child === "string" && /^-?\d+$/.test(child.trim())) return Number(child);
  }
  for (const key of ["arguments", "args"]) {
    const child = value[key];
    if (child == null) continue;
    const parsed = parseJsonValue(child);
    const nested = nestedIntegerField(parsed ?? child, names, depth + 1);
    if (nested != null) return nested;
  }
  for (const key of ["action", "input", "tool_input", "toolInput", "state", "request", "params", "parameters"]) {
    const nested = nestedIntegerField(value[key], names, depth + 1);
    if (nested != null) return nested;
  }
  return null;
}

function commandFromPayload(payload) {
  return (
    nestedStringField(payload, ["command", "cmd", "shell_command"]) ??
    directStringField(payload, ["name", "tool_name", "toolName", "tool"])
  );
}

function cwdFromPayload(payload) {
  return nestedStringField(payload, ["cwd", "workdir", "working_directory", "workingDirectory"]);
}

function statusFromPayload(payload) {
  return nestedStringField(payload, ["status", "state"]);
}

function exitCodeFromPayload(payload) {
  return nestedIntegerField(payload, ["exit_code", "exitCode", "code"]);
}

function isToolEvent(payload) {
  const type = String(payload?.type ?? "");
  return (
    type.includes("tool") ||
    type.includes("function") ||
    type.includes("exec") ||
    type.includes("shell") ||
    type === "patch" ||
    payload?.tool_use_id != null ||
    payload?.toolUseID != null ||
    payload?.call_id != null ||
    payload?.callID != null ||
    payload?.tool_name != null ||
    payload?.toolName != null
  );
}

function inferProjectPath(turns) {
  for (const turn of turns) {
    for (const part of turn.parts) {
      if (part.cwd?.trim()) return part.cwd;
    }
  }
  return null;
}

function normalizeTurns(text) {
  const turns = [];
  let current = null;
  let projectPath = null;
  for (const line of text.split(/\r?\n/)) {
    if (!line.trim()) continue;
    let parsed;
    try {
      parsed = JSON.parse(line);
    } catch {
      continue;
    }
    const payload = parsed.payload ?? parsed;
    projectPath ??= cwdFromPayload(parsed) ?? cwdFromPayload(payload);
    const role = payload.role;
    const type = payload.type;
    if (type === "message" && role === "user") {
      if (current) turns.push(current);
      const userText = contentText(payload.content);
      if (!userText.trim()) {
        current = null;
        continue;
      }
      current = {
        external_id: payload.id ?? `turn-${turns.length + 1}`,
        turn_index: turns.length,
        user_text: userText,
        title: null,
        started_at: parsed.timestamp ?? payload.timestamp ?? null,
        ended_at: null,
        parts: [],
      };
    } else if (current && type === "message" && role === "assistant") {
      const text = contentText(payload.content);
      if (text.trim()) {
        current.parts.push({
          role: "assistant",
          kind: "text",
          text,
          language: null,
          command: null,
          cwd: null,
          status: null,
          exit_code: null,
          metadata_json: metadata({ type: "answer", format: "markdown" }),
        });
      }
      current.ended_at = parsed.timestamp ?? payload.timestamp ?? current.ended_at;
    } else if (current && isToolEvent(payload)) {
      const command = commandFromPayload(payload);
      const cwd = cwdFromPayload(payload);
      const text = contentText(payload.content);
      const card = command
        ? { type: "command" }
        : { type: "result", format: "plain" };
      current.parts.push({
        role: "tool",
        kind: "command",
        text: text.trim() ? text : null,
        language: null,
        command,
        cwd,
        status: statusFromPayload(payload),
        exit_code: exitCodeFromPayload(payload),
        metadata_json: metadata(card, payload),
      });
      current.ended_at = parsed.timestamp ?? payload.timestamp ?? current.ended_at;
    }
  }
  if (current) turns.push(current);
  return { turns, projectPath };
}

function readSession() {
  let location = expandPath(input.source?.location);
  if (!location) return [];
  let dbPath = location;
  try {
    if (existsSync(location) && statSync(location).isDirectory()) {
      dbPath = path.join(location, "state_5.sqlite");
    }
  } catch {
    return [];
  }
  if (!existsSync(dbPath)) return [];
  const columns = sqliteJson(dbPath, "PRAGMA table_info(threads)").map((row) => row.name);
  const idCol = pick(columns, ["id", "thread_id", "session_id"]);
  const rolloutCol = pick(columns, ["rollout_path", "path", "file_path", "jsonl_path"]);
  if (!idCol || !rolloutCol) return [];
  const titleCol = pick(columns, ["title", "name"]);
  const updatedCol = pick(columns, ["updated_at", "last_updated_at", "mtime", "created_at"]);
  const sql = `SELECT ${quoteIdent(idCol)} AS id, ${quoteIdent(rolloutCol)} AS rollout_path, ${titleCol ? quoteIdent(titleCol) : "NULL"} AS title, ${updatedCol ? quoteIdent(updatedCol) : "NULL"} AS updated_at FROM threads ORDER BY rowid DESC LIMIT 500`;
  return sqliteJson(dbPath, sql).flatMap((row) => {
    const rolloutPath = expandPath(row.rollout_path);
    if (!rolloutPath || !existsSync(rolloutPath)) return [];
    const text = readFileSync(rolloutPath, "utf8");
    const parsed = normalizeTurns(text);
    const turns = parsed.turns;
    if (!turns.length) return [];
    return [{
      external_id: String(row.id),
      title: row.title == null ? null : String(row.title),
      project_path: parsed.projectPath ?? inferProjectPath(turns),
      started_at: turns[0]?.started_at ?? null,
      updated_at: row.updated_at == null ? null : String(row.updated_at),
      source_locator: rolloutPath,
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
