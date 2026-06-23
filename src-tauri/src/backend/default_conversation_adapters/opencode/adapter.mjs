#!/usr/bin/env node
import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const input = JSON.parse(readFileSync(0, "utf8") || "{}");
const SQLITE_MAX_BUFFER_BYTES = 64 * 1024 * 1024;

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

function sha256File(filePath) {
  return createHash("sha256").update(readFileSync(filePath)).digest("hex");
}

function sqliteJson(dbPath, sql) {
  const result = spawnSync("sqlite3", ["-json", dbPath, sql], {
    encoding: "utf8",
    maxBuffer: SQLITE_MAX_BUFFER_BYTES,
  });
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

function sqlString(value) {
  return `'${String(value).replaceAll("'", "''")}'`;
}

function pick(columns, candidates) {
  return candidates.find((name) => columns.includes(name));
}

function parseJson(text) {
  if (!text) return null;
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

function valueText(value, keys) {
  for (const key of keys) {
    const candidate = value?.[key];
    if (typeof candidate === "string") return candidate;
    if (typeof candidate === "number") return String(candidate);
  }
  return null;
}

function nestedValue(value, path) {
  let current = value;
  for (const key of path) {
    if (!current || typeof current !== "object") return null;
    current = current[key];
  }
  return current ?? null;
}

function nestedText(value, paths) {
  for (const path of paths) {
    const candidate = nestedValue(value, path);
    if (typeof candidate === "string") return candidate;
    if (typeof candidate === "number") return String(candidate);
  }
  return null;
}

function nestedInteger(value, paths) {
  for (const path of paths) {
    const candidate = nestedValue(value, path);
    if (Number.isInteger(candidate)) return candidate;
  }
  return null;
}

function partText(kind, value, fallback) {
  if (fallback) return fallback;
  if (!value) return "";
  return valueText(value, ["text", "content", "output", "message"])
    ?? nestedText(value, [["state", "output"], ["state", "metadata", "output"]])
    ?? "";
}

function normalizePart(row, role) {
  const data = parseJson(row.data);
  const kind = row.kind ?? valueText(data, ["type", "kind"]) ?? "text";
  const text = partText(kind, data, row.text);
  if (!text && !data) return null;
  if (["reasoning", "step-start", "step-finish", "step_finish", "compaction", "retry", "snapshot"].includes(kind)) return null;
  const lowerRole = String(role || "").toLowerCase();
  const normalizedKind = kind === "tool"
    ? "tool"
    : kind === "command"
      ? "command"
      : kind === "patch" || kind === "file"
        ? "file_change"
        : kind === "subtask" || kind === "agent"
          ? "subagent"
          : "text";
  const command = valueText(data, ["command", "cmd"])
    ?? nestedText(data, [["state", "input", "command"], ["input", "command"]])
    ?? null;
  const normalizedRole = lowerRole === "user"
    ? "user"
    : normalizedKind === "tool" || normalizedKind === "command"
      ? "tool"
      : "assistant";
  const contentCard = normalizedKind === "command" || command
    ? { type: "command" }
    : normalizedKind === "tool" || normalizedKind === "file_change" || normalizedKind === "subagent"
      ? { type: "result", format: "plain" }
      : { type: "answer", format: "markdown" };
  return {
    role: normalizedRole,
    kind: normalizedKind,
    text: text || null,
    language: null,
    command,
    cwd: valueText(data, ["cwd", "directory"])
      ?? nestedText(data, [["state", "input", "cwd"], ["input", "cwd"]])
      ?? null,
    status: valueText(data, ["status"])
      ?? nestedText(data, [["state", "status"]])
      ?? null,
    exit_code: Number.isInteger(data?.exit_code)
      ? data.exit_code
      : nestedInteger(data, [["state", "metadata", "exit"], ["state", "exit"]]),
    metadata_json: metadata(contentCard, data),
  };
}

function messageKey(sessionId, messageId) {
  return `${sessionId}\u0000${messageId}`;
}

function turnsForSession(turnsBySession, sessionId) {
  const existing = turnsBySession.get(sessionId);
  if (existing) return existing;
  const turns = [];
  turnsBySession.set(sessionId, turns);
  return turns;
}

function readTurnsBySession(dbPath, messageColumns, partColumns, sessionIds) {
  const turnsBySession = new Map();
  if (!sessionIds.length) return turnsBySession;
  const msgId = pick(messageColumns, ["id", "message_id"]);
  const msgSession = pick(messageColumns, ["session_id", "sessionID", "session"]);
  if (!msgId || !msgSession) return turnsBySession;
  const roleCol = pick(messageColumns, ["role", "author"]);
  const timeCol = pick(messageColumns, ["created_at", "time_created", "timeCreated", "time"]);
  const dataCol = pick(messageColumns, ["data", "json", "metadata"]);
  const partMessage = pick(partColumns, ["message_id", "messageID", "message"]);
  const partSession = pick(partColumns, ["session_id", "sessionID", "session"]);
  const partKind = pick(partColumns, ["type", "kind"]);
  const partTextCol = pick(partColumns, ["text", "content", "output"]);
  const partData = pick(partColumns, ["data", "json", "metadata"]);
  const sessionList = sessionIds.map(sqlString).join(", ");
  const ignoredPartTypes = ["reasoning", "step-start", "step-finish", "step_finish", "compaction", "retry", "snapshot"];
  const partFilters = [];
  if (partSession) partFilters.push(`${quoteIdent(partSession)} IN (${sessionList})`);
  if (partData) {
    partFilters.push(
      `COALESCE(json_extract(${quoteIdent(partData)}, '$.type'), '') NOT IN (${ignoredPartTypes.map(sqlString).join(", ")})`,
    );
  }
  const partWhere = partFilters.length ? `WHERE ${partFilters.join(" AND ")}` : "";
  const partSql = partMessage
    ? `SELECT ${partSession ? quoteIdent(partSession) : "NULL"} AS session_id, ${quoteIdent(partMessage)} AS message_id, ${partKind ? quoteIdent(partKind) : "NULL"} AS kind, ${partTextCol ? quoteIdent(partTextCol) : "NULL"} AS text, ${partData ? quoteIdent(partData) : "NULL"} AS data FROM part ${partWhere} ORDER BY rowid ASC`
    : "";
  const partsByMessage = new Map();
  if (partSql) {
    for (const row of sqliteJson(dbPath, partSql)) {
      const key = partSession
        ? messageKey(String(row.session_id), String(row.message_id))
        : String(row.message_id);
      const list = partsByMessage.get(key) ?? [];
      list.push(row);
      partsByMessage.set(key, list);
    }
  }
  const msgSql = `SELECT ${quoteIdent(msgId)} AS id, ${quoteIdent(msgSession)} AS session_id, ${roleCol ? quoteIdent(roleCol) : "NULL"} AS role, ${timeCol ? quoteIdent(timeCol) : "NULL"} AS timestamp, ${dataCol ? quoteIdent(dataCol) : "NULL"} AS data FROM message WHERE ${quoteIdent(msgSession)} IN (${sessionList}) ORDER BY rowid ASC`;
  const currentBySession = new Map();
  for (const message of sqliteJson(dbPath, msgSql)) {
    const sessionId = String(message.session_id);
    const data = parseJson(message.data);
    const role = message.role ?? valueText(data, ["role", "author"]) ?? "";
    const timestamp = message.timestamp ?? valueText(data, ["time", "created_at"]);
    const partKey = partSession
      ? messageKey(sessionId, String(message.id))
      : String(message.id);
    const parts = (partsByMessage.get(partKey) ?? [])
      .map((row) => normalizePart(row, role))
      .filter(Boolean);
    if (String(role).toLowerCase() === "user") {
      const userText = parts
        .filter((part) => part.role === "user" && part.kind === "text")
        .map((part) => part.text)
        .filter(Boolean)
        .join("\n\n");
      if (!userText.trim()) continue;
      const turns = turnsForSession(turnsBySession, sessionId);
      const current = currentBySession.get(sessionId);
      if (current) turns.push(current);
      currentBySession.set(sessionId, {
        external_id: String(message.id),
        turn_index: turns.length,
        user_text: userText,
        title: null,
        started_at: timestamp == null ? null : String(timestamp),
        ended_at: null,
        parts: parts.filter((part) => part.role !== "user"),
      });
    } else if (currentBySession.has(sessionId)) {
      const current = currentBySession.get(sessionId);
      current.parts.push(...parts);
      current.ended_at = timestamp == null ? current.ended_at : String(timestamp);
    }
  }
  for (const [sessionId, current] of currentBySession) {
    turnsForSession(turnsBySession, sessionId).push(current);
  }
  return turnsBySession;
}

function readSession() {
  const dbPath = expandPath(input.source?.location);
  if (!dbPath || !existsSync(dbPath)) return [];
  const sessionColumns = sqliteJson(dbPath, "PRAGMA table_info(session)").map((row) => row.name);
  const messageColumns = sqliteJson(dbPath, "PRAGMA table_info(message)").map((row) => row.name);
  const partColumns = sqliteJson(dbPath, "PRAGMA table_info(part)").map((row) => row.name);
  const idCol = pick(sessionColumns, ["id", "session_id"]);
  if (!idCol || !messageColumns.length || !partColumns.length) return [];
  const titleCol = pick(sessionColumns, ["title", "name"]);
  const projectCol = pick(sessionColumns, ["project", "project_path", "cwd", "directory", "path"]);
  const updatedCol = pick(sessionColumns, ["updated_at", "time_updated", "timeUpdated", "created_at"]);
  const requestedSessionId = input.params?.session_id ?? null;
  const sessionWhere = requestedSessionId ? `WHERE ${quoteIdent(idCol)} = ${sqlString(requestedSessionId)}` : "";
  const sessionSql = `SELECT ${quoteIdent(idCol)} AS id, ${titleCol ? quoteIdent(titleCol) : "NULL"} AS title, ${projectCol ? quoteIdent(projectCol) : "NULL"} AS project_path, ${updatedCol ? quoteIdent(updatedCol) : "NULL"} AS updated_at FROM session ${sessionWhere} ORDER BY rowid DESC LIMIT 500`;
  const fingerprint = sha256File(dbPath);
  const rows = sqliteJson(dbPath, sessionSql);
  const turnsBySession = readTurnsBySession(
    dbPath,
    messageColumns,
    partColumns,
    rows.map((row) => String(row.id)),
  );
  return rows.flatMap((row) => {
    const turns = turnsBySession.get(String(row.id)) ?? [];
    if (!turns.length) return [];
    return [{
      external_id: String(row.id),
      title: row.title == null ? null : String(row.title),
      project_path: row.project_path == null ? null : String(row.project_path),
      started_at: turns[0]?.started_at ?? null,
      updated_at: row.updated_at == null ? null : String(row.updated_at),
      source_locator: dbPath,
      source_fingerprint: fingerprint,
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
