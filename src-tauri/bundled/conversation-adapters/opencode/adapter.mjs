#!/usr/bin/env node
import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const input = JSON.parse(readFileSync(0, "utf8") || "{}");
const SQLITE_MAX_BUFFER_BYTES = 64 * 1024 * 1024;
const CONTENT_CARD_SCHEMA_VERSION = "opencode-content-cards-v3";

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

function sessionVersionToken(row) {
  return createHash("sha256")
    .update(CONTENT_CARD_SCHEMA_VERSION)
    .update("\0")
    .update(String(row.id))
    .update("\0")
    .update(String(row.updated_at ?? ""))
    .update("\0")
    .update(String(row.message_version ?? 0))
    .update("\0")
    .update(String(row.part_version ?? 0))
    .digest("hex");
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

function jsonExtract(column, jsonPath) {
  const ident = quoteIdent(column);
  return `CASE WHEN ${ident} IS NOT NULL AND json_valid(${ident}) THEN json_extract(${ident}, ${sqlString(jsonPath)}) ELSE NULL END`;
}

function sqlCoalesce(expressions) {
  return `COALESCE(${expressions.join(", ")})`;
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

function compactObject(value) {
  return Object.fromEntries(
    Object.entries(value).filter(([, entry]) => entry !== null && entry !== undefined && entry !== ""),
  );
}

function integerValue(value) {
  if (Number.isInteger(value)) return value;
  if (typeof value === "string" && /^-?\d+$/.test(value.trim())) return Number(value);
  return null;
}

function smallMetadata(value) {
  if (!value || typeof value !== "object") return {};
  return compactObject({
    source_type: value.type ?? value.kind,
    tool: value.tool,
    title: value.title,
    description: value.description,
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
  const rawData = parseJson(row.data);
  const data = rawData ?? compactObject({
    type: row.kind,
    kind: row.kind,
    command: row.command,
    cmd: row.command,
    cwd: row.cwd,
    directory: row.cwd,
    status: row.status,
    exit_code: integerValue(row.exit_code),
    tool: row.tool,
    title: row.title,
    description: row.description,
  });
  const kind = row.kind ?? valueText(data, ["type", "kind"]) ?? "text";
  const text = partText(kind, data, row.text);
  if (!text && (!data || Object.keys(data).length === 0)) return null;
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
  if (normalizedKind === "text") {
    return splitMarkdownParts(normalizedRole, text);
  }
  const cwd = valueText(data, ["cwd", "directory"])
    ?? nestedText(data, [["state", "input", "cwd"], ["input", "cwd"]])
    ?? null;
  const status = valueText(data, ["status"])
    ?? nestedText(data, [["state", "status"]])
    ?? null;
  const exitCode = Number.isInteger(data?.exit_code)
    ? data.exit_code
    : nestedInteger(data, [["state", "metadata", "exit"], ["state", "exit"]]);
  const resultText = String(text ?? "").trim();
  if (normalizedKind === "command" || command) {
    const parts = [];
    if (command?.trim()) {
      parts.push({
        role: normalizedRole,
        kind: "command",
        text: null,
        language: null,
        command,
        cwd,
        status: null,
        exit_code: null,
        metadata_json: metadata(compactObject({ type: "command", cwd }), smallMetadata(data)),
      });
    }
    if (resultText) {
      parts.push({
        role: "tool",
        kind: "tool",
        text: resultText,
        language: null,
        command: null,
        cwd: null,
        status,
        exit_code: exitCode,
        metadata_json: metadata(
          compactObject({ type: "result", format: "plain", status, exit_code: exitCode }),
          smallMetadata(data),
        ),
      });
    }
    return parts;
  }
  const contentCard = normalizedKind === "tool" || normalizedKind === "file_change" || normalizedKind === "subagent"
    ? { type: "result", format: "plain" }
    : { type: "answer", format: "markdown" };
  return {
    role: normalizedRole,
    kind: normalizedKind,
    text: text || null,
    language: null,
    command: null,
    cwd,
    status,
    exit_code: exitCode,
    metadata_json: metadata(contentCard, smallMetadata(data)),
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

function displayTurns(turns) {
  return turns
    .filter((turn) => Array.isArray(turn.parts) && turn.parts.length > 0)
    .map((turn, index) => ({
      ...turn,
      turn_index: index,
    }));
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
  const partTypeExpr = partKind ? quoteIdent(partKind) : partData ? jsonExtract(partData, "$.type") : null;
  if (partTypeExpr) partFilters.push(`COALESCE(${partTypeExpr}, '') NOT IN (${ignoredPartTypes.map(sqlString).join(", ")})`);
  const partWhere = partFilters.length ? `WHERE ${partFilters.join(" AND ")}` : "";
  const partKindExpr = partKind ? quoteIdent(partKind) : partData ? jsonExtract(partData, "$.type") : "NULL";
  const partTextExpr = partTextCol
    ? quoteIdent(partTextCol)
    : partData
      ? sqlCoalesce([
        jsonExtract(partData, "$.text"),
        jsonExtract(partData, "$.content"),
        jsonExtract(partData, "$.output"),
        jsonExtract(partData, "$.message"),
        jsonExtract(partData, "$.state.output"),
        jsonExtract(partData, "$.state.metadata.output"),
        jsonExtract(partData, "$.state.title"),
      ])
      : "NULL";
  const partCommandExpr = partData
    ? sqlCoalesce([
      jsonExtract(partData, "$.command"),
      jsonExtract(partData, "$.cmd"),
      jsonExtract(partData, "$.state.input.command"),
      jsonExtract(partData, "$.input.command"),
    ])
    : "NULL";
  const partCwdExpr = partData
    ? sqlCoalesce([
      jsonExtract(partData, "$.cwd"),
      jsonExtract(partData, "$.directory"),
      jsonExtract(partData, "$.state.input.cwd"),
      jsonExtract(partData, "$.input.cwd"),
    ])
    : "NULL";
  const partStatusExpr = partData ? sqlCoalesce([jsonExtract(partData, "$.status"), jsonExtract(partData, "$.state.status")]) : "NULL";
  const partExitExpr = partData
    ? sqlCoalesce([
      jsonExtract(partData, "$.exit_code"),
      jsonExtract(partData, "$.state.metadata.exit"),
      jsonExtract(partData, "$.state.exit"),
    ])
    : "NULL";
  const partToolExpr = partData ? sqlCoalesce([jsonExtract(partData, "$.tool"), jsonExtract(partData, "$.tool_name")]) : "NULL";
  const partTitleExpr = partData ? sqlCoalesce([jsonExtract(partData, "$.title"), jsonExtract(partData, "$.state.title")]) : "NULL";
  const partDescriptionExpr = partData ? sqlCoalesce([jsonExtract(partData, "$.description"), jsonExtract(partData, "$.state.input.description")]) : "NULL";
  const partSql = partMessage
    ? `SELECT ${partSession ? quoteIdent(partSession) : "NULL"} AS session_id, ${quoteIdent(partMessage)} AS message_id, ${partKindExpr} AS kind, ${partTextExpr} AS text, NULL AS data, ${partCommandExpr} AS command, ${partCwdExpr} AS cwd, ${partStatusExpr} AS status, ${partExitExpr} AS exit_code, ${partToolExpr} AS tool, ${partTitleExpr} AS title, ${partDescriptionExpr} AS description FROM part ${partWhere} ORDER BY rowid ASC`
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
  const roleExpr = roleCol
    ? quoteIdent(roleCol)
    : dataCol
      ? sqlCoalesce([jsonExtract(dataCol, "$.role"), jsonExtract(dataCol, "$.author")])
      : "NULL";
  const timestampExpr = timeCol
    ? quoteIdent(timeCol)
    : dataCol
      ? sqlCoalesce([jsonExtract(dataCol, "$.time.created"), jsonExtract(dataCol, "$.created_at"), jsonExtract(dataCol, "$.time")])
      : "NULL";
  const msgSql = `SELECT ${quoteIdent(msgId)} AS id, ${quoteIdent(msgSession)} AS session_id, ${roleExpr} AS role, ${timestampExpr} AS timestamp, NULL AS data FROM message WHERE ${quoteIdent(msgSession)} IN (${sessionList}) ORDER BY rowid ASC`;
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
      .flatMap((row) => normalizePart(row, role) ?? [])
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

function readSessions(includeTurns) {
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
  const messageSessionCol = pick(messageColumns, ["session_id", "sessionID", "session"]);
  const messageIdCol = pick(messageColumns, ["id", "message_id"]);
  const partSessionCol = pick(partColumns, ["session_id", "sessionID", "session"]);
  const partMessageCol = pick(partColumns, ["message_id", "messageID", "message"]);
  if (!messageSessionCol) return [];
  const requestedSessionId = input.params?.session_id ?? null;
  const sessionWhere = requestedSessionId ? `WHERE s.${quoteIdent(idCol)} = ${sqlString(requestedSessionId)}` : "";
  const messageVersionExpr = `(SELECT COALESCE(MAX(m.rowid), 0) FROM message m WHERE m.${quoteIdent(messageSessionCol)} = s.${quoteIdent(idCol)})`;
  const partVersionExpr = partSessionCol
    ? `(SELECT COALESCE(MAX(p.rowid), 0) FROM part p WHERE p.${quoteIdent(partSessionCol)} = s.${quoteIdent(idCol)})`
    : messageIdCol && partMessageCol
      ? `(SELECT COALESCE(MAX(p.rowid), 0) FROM part p JOIN message m ON p.${quoteIdent(partMessageCol)} = m.${quoteIdent(messageIdCol)} WHERE m.${quoteIdent(messageSessionCol)} = s.${quoteIdent(idCol)})`
      : "0";
  const sessionSql = `SELECT s.${quoteIdent(idCol)} AS id, ${titleCol ? `s.${quoteIdent(titleCol)}` : "NULL"} AS title, ${projectCol ? `s.${quoteIdent(projectCol)}` : "NULL"} AS project_path, ${updatedCol ? `s.${quoteIdent(updatedCol)}` : "NULL"} AS updated_at, ${messageVersionExpr} AS message_version, ${partVersionExpr} AS part_version FROM session s ${sessionWhere} ORDER BY s.rowid DESC`;
  const rows = sqliteJson(dbPath, sessionSql);
  const turnsBySession = includeTurns
    ? readTurnsBySession(
      dbPath,
      messageColumns,
      partColumns,
      rows.map((row) => String(row.id)),
    )
    : new Map();
  return rows.flatMap((row) => {
    const turns = displayTurns(turnsBySession.get(String(row.id)) ?? []);
    if (includeTurns && !turns.length) return [];
    return [{
      external_id: String(row.id),
      title: row.title == null ? null : String(row.title),
      project_path: row.project_path == null ? null : String(row.project_path),
      started_at: turns[0]?.started_at ?? null,
      updated_at: row.updated_at == null ? null : String(row.updated_at),
      source_locator: dbPath,
      source_fingerprint: sessionVersionToken(row),
      turns,
    }];
  });
}

function readSession() {
  return readSessions(true);
}

function listSessions() {
  return readSessions(false).map((session) => ({
    external_id: session.external_id,
    updated_at: session.updated_at,
    source_locator: session.source_locator,
    version_token: session.source_fingerprint,
  }));
}

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
  const metadata = typeof value === "string" ? parseJson(value) : value;
  const card = metadata?.content_card ?? metadata?.contentCard;
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

try {
  if (input.method === "probe") {
    emit("complete", { item: { session_count: 0 } });
  } else if (input.method === "list_sessions") {
    const descriptors = listSessions();
    for (const descriptor of descriptors) emit("item", { item: { kind: "session_descriptor", ...descriptor } });
    emit("complete", { item: { session_count: descriptors.length, snapshot_complete: true } });
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
