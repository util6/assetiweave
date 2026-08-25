#!/usr/bin/env node
/**
 * ZCode 会话解析适配器 (ZCode Conversation Adapter).
 *
 * 负责读取、解析及转换 ZCode 本地 SQLite 数据库与 JSON 会话记录为 AssetIWeave 标准会话和内容卡片。
 */
import { createHash } from "node:crypto";
import { existsSync, readFileSync, statSync } from "node:fs";
import { homedir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const SQLITE_MAX_BUFFER_BYTES = 64 * 1024 * 1024;
const CONTENT_CARD_SCHEMA_VERSION = "zcode-content-cards-v7";
const SHELL_EXECUTION_PROJECTION_SCHEMA_VERSION = 1;
const IGNORED_PART_TYPES = new Set([
  "compaction",
  "reasoning",
  "retry",
  "snapshot",
  "step-finish",
  "step-start",
]);

function emit(payload) {
  process.stdout.write(`${JSON.stringify(payload)}\n`);
}

function fail(error) {
  emit({
    type: "error",
    error: {
      message: error instanceof Error ? error.message : String(error),
      kind: error?.name || "Error",
    },
  });
}

function compactJson(value) {
  return JSON.stringify(value);
}

function expandPath(value) {
  if (!value) return value;
  if (value === "~") return homedir();
  if (value.startsWith("~/")) return path.join(homedir(), value.slice(2));
  return value;
}

function quoteIdent(name) {
  return `"${String(name).replaceAll("\"", "\"\"")}"`;
}

function sqlString(value) {
  return `'${String(value).replaceAll("'", "''")}'`;
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

function parseJson(text) {
  if (typeof text !== "string") return {};
  try {
    const value = JSON.parse(text);
    return value && typeof value === "object" && !Array.isArray(value) ? value : {};
  } catch {
    return {};
  }
}

function compactObject(value) {
  if (!value || typeof value !== "object") return {};
  return Object.fromEntries(
    Object.entries(value).filter(([, entry]) => entry !== null && entry !== undefined && entry !== ""),
  );
}

function contentCardMetadata(contentCard, extra = {}) {
  const metadata = { ...(extra && typeof extra === "object" ? extra : {}) };
  metadata.content_card = compactObject(contentCard);
  return compactJson(metadata);
}

function smallMetadata(data) {
  if (!data || typeof data !== "object") return {};
  return compactObject({
    source_type: data.type,
    tool: data.tool || data.tool_name || data.toolName,
    title: data.title,
  });
}

function sourceDatabase(location) {
  const expanded = expandPath(location);
  if (!expanded) return expanded;
  const resolved = path.resolve(expanded);
  if (existsSync(resolved) && statSync(resolved).isDirectory()) {
    for (const candidate of [
      path.join(resolved, "db", "db.sqlite"),
      path.join(resolved, "cli", "db", "db.sqlite"),
      path.join(resolved, "db.sqlite"),
    ]) {
      if (existsSync(candidate) && statSync(candidate).isFile()) {
        return candidate;
      }
    }
  }
  return resolved;
}

function tableColumns(dbPath, table) {
  const rows = sqliteJson(dbPath, `PRAGMA table_info(${quoteIdent(table)})`);
  return new Set(rows.map((row) => String(row.name)));
}

function validateSchema(dbPath) {
  if (!existsSync(dbPath) || !statSync(dbPath).isFile()) {
    throw new Error(`ZCode SQLite database not found: ${dbPath}`);
  }
  const required = {
    session: ["id", "title", "time_updated"],
    message: ["id", "session_id", "time_created", "data"],
    part: ["id", "message_id", "session_id", "time_created", "data"],
  };
  for (const [table, columns] of Object.entries(required)) {
    const existing = tableColumns(dbPath, table);
    const missing = columns.filter((col) => !existing.has(col));
    if (missing.length > 0) {
      throw new Error(`ZCode table ${table} is missing required columns: ${missing.sort().join(", ")}`);
    }
  }
}

function sessionVersionToken(row) {
  const hash = createHash("sha256");
  hash.update(CONTENT_CARD_SCHEMA_VERSION);
  hash.update("\0");
  for (const key of ["id", "time_updated", "message_marker", "part_marker"]) {
    hash.update(String(row[key] ?? ""));
    hash.update("\0");
  }
  return hash.digest("hex");
}

function timestamp(value) {
  if (value == null || typeof value === "boolean") return null;
  if (typeof value === "number") return String(Math.floor(value));
  const text = String(value).trim();
  return text || null;
}

function messageTimestamp(row, data) {
  const direct = timestamp(row.time_created);
  if (direct) return direct;
  const timeValue = data?.time;
  if (timeValue && typeof timeValue === "object") {
    return timestamp(timeValue.created);
  }
  return timestamp(timeValue);
}

function collectStrings(value, output) {
  if (typeof value === "string") {
    if (value.trim()) output.push(value);
    return;
  }
  if (Array.isArray(value)) {
    for (const item of value) collectStrings(item, output);
    return;
  }
  if (!value || typeof value !== "object") return;
  if (value.ignored === true || value.synthetic === true || value.isSynthetic === true || value.isMeta === true) {
    return;
  }
  if (IGNORED_PART_TYPES.has(value.type)) return;
  for (const key of [
    "text",
    "content",
    "output",
    "result",
    "summary",
    "stdout",
    "stderr",
    "preview",
    "message",
    "title",
    "patch",
    "diff",
    "error",
  ]) {
    if (key in value) collectStrings(value[key], output);
  }
}

function toolText(data) {
  const values = [];
  for (const key of ["output", "result", "content", "error"]) {
    if (key in data) collectStrings(data[key], values);
  }
  const state = data?.state;
  if (state && typeof state === "object") {
    for (const key of ["output", "error", "message"]) {
      if (key in state) collectStrings(state[key], values);
    }
  }
  if (values.length === 0) {
    for (const source of [data, state && typeof state === "object" ? state : {}]) {
      for (const key of ["summary", "message", "title"]) {
        if (key in source) collectStrings(source[key], values);
      }
    }
  }
  const unique = [...new Set(values)];
  const text = unique.join("\n").trim();
  return text || null;
}

function nestedString(value, names, depth = 0) {
  if (depth > 8 || !value || typeof value !== "object") return null;
  for (const name of names) {
    const candidate = value[name];
    if (typeof candidate === "string" && candidate.trim()) return candidate;
  }
  for (const key of [
    "state",
    "input",
    "tool_input",
    "toolInput",
    "action",
    "request",
    "params",
    "parameters",
  ]) {
    const candidate = nestedString(value[key], names, depth + 1);
    if (candidate) return candidate;
  }
  for (const key of ["arguments", "args"]) {
    let child = value[key];
    if (typeof child === "string" && (child.trimStart().startsWith("{") || child.trimStart().startsWith("["))) {
      try {
        child = JSON.parse(child);
      } catch {
        continue;
      }
    }
    const candidate = nestedString(child, names, depth + 1);
    if (candidate) return candidate;
  }
  return null;
}

function nestedInt(value, names, depth = 0) {
  if (depth > 8 || !value || typeof value !== "object") return null;
  for (const name of names) {
    const candidate = value[name];
    if (Number.isInteger(candidate) && typeof candidate !== "boolean") return candidate;
  }
  for (const key of ["state", "input", "result", "metadata"]) {
    const candidate = nestedInt(value[key], names, depth + 1);
    if (candidate !== null) return candidate;
  }
  return null;
}

function normalizedPart({
  role,
  kind,
  text = null,
  language = null,
  command = null,
  cwd = null,
  status = null,
  exit_code = null,
  source_execution_id = null,
  metadata_json = null,
}) {
  return {
    role,
    kind,
    text,
    language,
    command,
    cwd,
    status,
    exit_code,
    source_execution_id,
    metadata_json,
  };
}

function splitMarkdown(role, text) {
  const parts = [];
  let remaining = String(text ?? "");
  while (remaining.includes("```")) {
    const start = remaining.indexOf("```");
    const before = remaining.slice(0, start);
    if (before && before.trim()) {
      parts.push(
        normalizedPart({
          role,
          kind: "text",
          text: before.trim(),
          metadata_json: role === "assistant"
            ? contentCardMetadata({ type: "answer", format: "markdown" })
            : null,
        }),
      );
    }
    const fenceBody = remaining.slice(start + 3);
    const end = fenceBody.indexOf("```");
    if (end < 0) {
      const tail = `\`\`\`${fenceBody}`.trim();
      if (tail) {
        parts.push(
          normalizedPart({
            role,
            kind: "text",
            text: tail,
            metadata_json: role === "assistant"
              ? contentCardMetadata({ type: "answer", format: "markdown" })
              : null,
          }),
        );
      }
      return parts;
    }
    const fenced = fenceBody.slice(0, end);
    remaining = fenceBody.slice(end + 3);
    let language = null;
    let code = fenced;
    if (fenced.includes("\n")) {
      const firstNewline = fenced.indexOf("\n");
      language = fenced.slice(0, firstNewline).trim() || null;
      code = fenced.slice(firstNewline + 1);
    }
    code = code.replace(/^\n+|\n+$/g, "");
    if (code.trim()) {
      parts.push(
        normalizedPart({
          role,
          kind: "code_block",
          text: code,
          language,
          metadata_json: contentCardMetadata({ type: "code", language }),
        }),
      );
    }
  }
  if (remaining && remaining.trim()) {
    parts.push(
      normalizedPart({
        role,
        kind: "text",
        text: remaining.trim(),
        metadata_json: role === "assistant"
          ? contentCardMetadata({ type: "answer", format: "markdown" })
          : null,
      }),
    );
  }
  return parts;
}

function normalizeAssistantPart(data, sourceExecutionId = null) {
  const kind = String(data?.type || "text");
  if (IGNORED_PART_TYPES.has(kind)) return [];
  if (kind === "text") {
    const text = data?.text;
    return typeof text === "string" ? splitMarkdown("assistant", text) : [];
  }
  if (kind === "tool" || kind === "tool-call" || kind === "tool-result") {
    const command = nestedString(data, ["command", "cmd", "shell_command"]);
    const text = toolText(data);
    const cwd = nestedString(data, ["cwd", "workdir", "working_directory", "workingDirectory"]);
    const status = nestedString(data, ["status"]);
    const exitCode = nestedInt(data, ["exit_code", "exitCode", "code"]);
    const parts = [];
    if (command) {
      parts.push(
        normalizedPart({
          role: "tool",
          kind: "command",
          command,
          cwd,
          source_execution_id: sourceExecutionId,
          metadata_json: contentCardMetadata(
            { type: "command", cwd },
            smallMetadata(data),
          ),
        }),
      );
      if (text) {
        parts.push(
          normalizedPart({
            role: "tool",
            kind: "tool",
            text,
            status,
            exit_code: exitCode,
            source_execution_id: sourceExecutionId,
            metadata_json: contentCardMetadata(
              {
                type: "result",
                format: "plain",
                status,
                exit_code: exitCode,
              },
              smallMetadata(data),
            ),
          }),
        );
      }
      return parts;
    }
    if (text) {
      return [
        normalizedPart({
          role: "tool",
          kind: "tool",
          text,
          status,
          exit_code: exitCode,
          source_execution_id: sourceExecutionId,
          metadata_json: contentCardMetadata(
            {
              type: "result",
              format: "plain",
              status,
              exit_code: exitCode,
            },
            smallMetadata(data),
          ),
        }),
      ];
    }
    return [
      normalizedPart({
        role: "tool",
        kind: "tool",
        text: String(data?.tool || data?.tool_name || data?.type || "tool"),
        metadata_json: contentCardMetadata(
          { type: "tool", format: "plain" },
          smallMetadata(data),
        ),
      }),
    ];
  }
  if (kind === "file" || kind === "patch") {
    const textFields = kind === "patch"
      ? ["patch", "diff", "text", "summary", "path", "filename", "name", "url"]
      : ["path", "filename", "name", "text", "summary", "url"];
    const text = nestedString(data, textFields);
    return [
      normalizedPart({
        role: "assistant",
        kind: "file_change",
        text,
        metadata_json: contentCardMetadata(
          { type: "file-change", format: "diff" },
          smallMetadata(data),
        ),
      }),
    ];
  }
  const textValues = [];
  collectStrings(data, textValues);
  const text = textValues.join("\n").trim();
  return text
    ? [
        normalizedPart({
          role: "assistant",
          kind: "text",
          text,
          metadata_json: contentCardMetadata({ type: "answer", format: "markdown" }),
        }),
      ]
    : [];
}

function loadPartsByMessage(dbPath, sessionId) {
  const grouped = new Map();
  const sql = `
    SELECT id, message_id, data
    FROM part
    WHERE session_id = ${sqlString(sessionId)}
    ORDER BY time_created ASC, id ASC
  `;
  for (const row of sqliteJson(dbPath, sql)) {
    const msgId = String(row.message_id);
    const list = grouped.get(msgId) ?? [];
    const data = parseJson(row.data);
    const sourceExecutionId = nestedString(data, [
      "source_execution_id",
      "sourceExecutionId",
      "call_id",
      "callId",
      "callID",
      "execution_id",
      "executionId",
    ]) || (nestedString(data, ["command", "cmd", "shell_command"]) ? String(row.id) : null);
    list.push({ data, sourceExecutionId });
    grouped.set(msgId, list);
  }
  return grouped;
}

function userText(parts) {
  const texts = parts
    .map((part) => part.data)
    .filter((part) => part.type === "text" && typeof part.text === "string" && part.text.trim())
    .map((part) => String(part.text).trim());
  return texts.join("\n\n");
}

function loadTurns(dbPath, sessionId) {
  const partsByMessage = loadPartsByMessage(dbPath, sessionId);
  const turns = [];
  let current = null;
  const sql = `
    SELECT id, time_created, data
    FROM message
    WHERE session_id = ${sqlString(sessionId)}
    ORDER BY time_created ASC, id ASC
  `;
  for (const row of sqliteJson(dbPath, sql)) {
    const messageId = String(row.id);
    const data = parseJson(row.data);
    const role = String(data.role || "");
    const createdAt = messageTimestamp(row, data);
    const messageParts = partsByMessage.get(messageId) || [];
    if (role === "user") {
      const prompt = userText(messageParts);
      if (!prompt) continue;
      if (current !== null) {
        turns.push(current);
      }
      current = {
        external_id: messageId,
        turn_index: turns.length,
        user_text: prompt,
        title: null,
        started_at: createdAt,
        ended_at: null,
        parts: [],
      };
    } else if (current !== null) {
      for (const part of messageParts) {
        current.parts.push(...normalizeAssistantPart(part.data, part.sourceExecutionId));
      }
      current.ended_at = createdAt;
    }
  }
  if (current !== null) {
    turns.push(current);
  }
  return displayTurns(turns).map((turn) => ({
    ...turn,
    parts: annotateShellExecutionProjectionParts(turn.parts),
  }));
}

function annotateShellExecutionProjectionParts(parts) {
  for (const part of parts) {
    if (part?.kind !== "command" || typeof part.command !== "string") continue;
    const metadata = parseJson(part.metadata_json);
    const sourceType = String(metadata.tool ?? metadata.source_type ?? "").toLowerCase();
    if (sourceType && sourceType !== "tool" && !/(shell|command|exec|terminal|bash|zsh|run)/.test(sourceType)) continue;
    const nodes = [];
    let pendingLabel = null;
    for (const command of splitTopLevelShellCommands(part.command)) {
      const separator = parseSeparatorPrintCommand(command);
      if (separator) {
        pendingLabel = separator.label;
        continue;
      }
      nodes.push(compactObject({ command, command_label: pendingLabel }));
      pendingLabel = null;
    }
    metadata.shell_execution_projection = {
      schema_version: SHELL_EXECUTION_PROJECTION_SCHEMA_VERSION,
      nodes,
    };
    part.metadata_json = compactJson(metadata);
  }
  return parts;
}

function splitTopLevelShellCommands(value) {
  const source = String(value ?? "").replace(/\r\n?/g, "\n").trim();
  if (!source || isComplexShellScript(source)) return source ? [source] : [];
  const commands = [];
  let start = 0;
  let quote = null;
  let escaped = false;
  let parenDepth = 0;
  let braceDepth = 0;
  let bracketDepth = 0;
  let previousNonWhitespace = null;
  const pushCommand = (end) => {
    const command = source.slice(start, end).trim();
    if (command) commands.push(command);
  };
  for (let index = 0; index < source.length; index += 1) {
    const char = source[index];
    const next = source[index + 1];
    const precedingNonWhitespace = previousNonWhitespace;
    if (!/\s/.test(char)) previousNonWhitespace = char;
    if (escaped) {
      escaped = false;
      continue;
    }
    if (char === "\\" && quote !== "'") {
      escaped = true;
      continue;
    }
    if (quote) {
      if (char === quote) quote = null;
      continue;
    }
    if (char === "'" || char === '"' || char === "`") {
      quote = char;
      continue;
    }
    if (char === "(") parenDepth += 1;
    else if (char === ")") parenDepth = Math.max(0, parenDepth - 1);
    else if (char === "{") braceDepth += 1;
    else if (char === "}") braceDepth = Math.max(0, braceDepth - 1);
    else if (char === "[") bracketDepth += 1;
    else if (char === "]") bracketDepth = Math.max(0, bracketDepth - 1);
    if (parenDepth || braceDepth || bracketDepth) continue;
    let nextNonWhitespace = next;
    if (char === "\n") {
      let cursor = index + 1;
      while (source[cursor] === " " || source[cursor] === "\t") cursor += 1;
      nextNonWhitespace = source[cursor];
    }
    const continuedPipeline = char === "\n"
      && (precedingNonWhitespace === "|" || nextNonWhitespace === "|");
    let separatorLength = 0;
    if (!continuedPipeline) {
      if ((char === "&" && next === "&") || (char === "|" && next === "|")) separatorLength = 2;
      else if (char === ";" || char === "\n") separatorLength = 1;
    }
    if (!separatorLength) continue;
    pushCommand(index);
    index += separatorLength - 1;
    start = index + 1;
  }
  pushCommand(source.length);
  return commands.length > 0 ? commands : [source];
}

function isComplexShellScript(source) {
  return /<<-?\s*['"]?[A-Za-z_][A-Za-z0-9_]*['"]?/.test(source)
    || /(?:^|[;&|\n]\s*)(?:for|select|while|until|if|case|function)\b/.test(source)
    || /^\s*\{(?:\s|\n)/.test(source);
}

function parseSeparatorPrintCommand(command) {
  const trimmed = String(command ?? "").trim();
  let body = null;
  const printfWithArgument = trimmed.match(/^printf\s+(['"])([\s\S]*?)\1\s+(['"])([\s\S]*?)\3\s*$/);
  if (printfWithArgument) {
    const format = printfWithArgument[2];
    const substitutions = format.match(/%s/g) ?? [];
    if (substitutions.length === 1 && /^(?:%s|\\[nrt]|\s)+$/.test(format)) body = printfWithArgument[4];
  } else {
    const printfLiteral = trimmed.match(/^printf\s+(['"])([\s\S]*?)\1\s*$/);
    const echoLiteral = trimmed.match(/^echo\s+(?:-[A-Za-z]+\s+)*(['"])([\s\S]*?)\1\s*$/);
    body = printfLiteral?.[2] ?? echoLiteral?.[2] ?? null;
  }
  if (body == null) return null;
  const printedText = body
    .replace(/^(?:(?:\\[nrt])+|\s)+|(?:(?:\\[nrt])+|\s)+$/g, "")
    .trim();
  if (!printedText) return null;
  const divider = "[-=*~_─━—–]";
  if (new RegExp(`^${divider}+$`, "u").test(printedText)) return { label: null };
  const wrappedLabel = printedText.match(new RegExp(`^${divider}{2,}\\s*(.{1,80}?)\\s*${divider}{2,}$`, "u"));
  if (!wrappedLabel) return null;
  return { label: wrappedLabel[1].replace(/\\s+/g, " ").trim() || null };
}

function displayTurns(turns) {
  const visible = turns.filter((turn) => Array.isArray(turn.parts) && turn.parts.length > 0);
  return visible.map((turn, index) => ({
    ...turn,
    turn_index: index,
  }));
}

function sessionRows(dbPath, sessionId, maxSessions) {
  const columns = tableColumns(dbPath, "session");
  const projectExpression = columns.has("path") && columns.has("directory")
    ? "COALESCE(NULLIF(path, ''), NULLIF(directory, ''))"
    : columns.has("path")
      ? "path"
      : columns.has("directory")
        ? "directory"
        : "NULL";
  let query = `
    SELECT id, title, ${projectExpression} AS project_path, time_updated,
           COALESCE((SELECT MAX(CAST(time_created AS TEXT) || ':' || id)
                     FROM message WHERE session_id = session.id), '') AS message_marker,
           COALESCE((SELECT MAX(CAST(time_created AS TEXT) || ':' || id)
                     FROM part WHERE session_id = session.id), '') AS part_marker
    FROM session
  `;
  if (sessionId) {
    query += ` WHERE id = ${sqlString(sessionId)}`;
  }
  query += ` ORDER BY time_updated DESC, id DESC LIMIT ${maxSessions}`;
  return sqliteJson(dbPath, query);
}

function configuredLimit(config) {
  if (!config || typeof config !== "object") return 500;
  const value = config.max_sessions ?? 500;
  if (!Number.isInteger(value) || typeof value === "boolean") {
    throw new Error("source.config.max_sessions must be an integer");
  }
  return Math.min(Math.max(value, 1), 5000);
}

function executionTextValue(value, depth = 0) {
  if (value == null || depth > 6) return "";
  if (typeof value === "string") {
    const source = value.trim();
    if (source.startsWith("[") || source.startsWith("{") || source.startsWith('"')) {
      try {
        return executionTextValue(JSON.parse(source), depth + 1);
      } catch {
        // Not valid JSON, keep as is
      }
    }
    return value;
  }
  if (typeof value === "boolean") return value ? "true" : "false";
  if (typeof value === "number") return String(value);
  if (Array.isArray(value)) {
    const fragmentKeys = new Set(["stdout", "stderr", "output", "result", "content", "text", "message"]);
    const structuredFragments = value.length > 0 && value.every(
      (entry) => entry && typeof entry === "object" && Object.keys(entry).some((k) => fragmentKeys.has(k)),
    );
    if (!structuredFragments) return JSON.stringify(value, null, 2);
    let result = "";
    for (const entry of value) {
      const fragment = executionTextValue(entry, depth + 1);
      if (!fragment) continue;
      const separator = !result || result.endsWith("\n") || fragment.startsWith("\n") ? "" : "\n";
      result += separator + fragment;
    }
    return result;
  }
  if (typeof value === "object") {
    const streams = ["stdout", "stderr"]
      .map((k) => executionTextValue(value[k], depth + 1))
      .filter((entry) => entry.trim());
    if (streams.length > 0) return streams.join("\n");
    for (const key of ["output", "result", "content", "text", "message"]) {
      if (value[key] != null) return executionTextValue(value[key], depth + 1);
    }
    return JSON.stringify(value, null, 2);
  }
  return String(value);
}

function normalizeTerminalText(value) {
  const text = String(value ?? "")
    .replace(/\r\n/g, "\n")
    .replace(/\x1b\][^\x07]*(?:\x07|\x1b\\)/g, "")
    .replace(/\x1b\[[0-?]*[ -/]*[@-~]/g, "")
    .replace(/[\x00\x08\x0b\x0c\x0e-\x1f\x7f]/g, "");
  const lines = text.split("\n").map((line) => {
    if (line.includes("\r")) {
      line = line.slice(line.lastIndexOf("\r") + 1);
    }
    return line.trimEnd();
  });
  return lines.join("\n").trim().replace(/\n{3,}/g, "\n\n");
}

function stripExecutionEnvelope(value) {
  const header = /^(?:Created At:.*|Completed At:.*|Script completed(?: successfully)?(?: in .*)?|Wall time\b.*|Chunk ID:.*|Process exited with code\b.*|Original token count:.*|The command completed successfully\.?|(?:Final |Original |Command )?Output:)$/i;
  const lines = String(value ?? "").split("\n");
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
  if (source.startsWith("{") || source.startsWith('"')) {
    try {
      const parsed = JSON.parse(source);
      if (typeof parsed === "string") return normalizeTerminalText(parsed);
      if (parsed && typeof parsed === "object") {
        const command = parsed.command || parsed.cmd || parsed.shell_command;
        if (typeof command === "string") return normalizeTerminalText(command);
      }
    } catch {
      // Keep source
    }
  }
  return normalizeTerminalText(value);
}

function decodeDiffPath(value) {
  let candidate = String(value ?? "").split("\t", 1)[0].trim();
  if (!candidate) return null;
  if (candidate.startsWith('"') && candidate.endsWith('"')) {
    try {
      const decoded = JSON.parse(candidate);
      if (typeof decoded === "string") candidate = decoded;
    } catch {
      candidate = candidate.slice(1, -1);
    }
  }
  return candidate.replace(/^[ab]\//, "") || null;
}

function diffFilePath(value) {
  const lines = String(value ?? "").split("\n");
  const renamed = lines.find((line) => line.startsWith("rename to "))?.slice("rename to ".length);
  const newPath = lines.find((line) => line.startsWith("+++ "))?.slice(4);
  const oldPath = lines.find((line) => line.startsWith("--- "))?.slice(4);
  for (const candidate of [renamed, newPath, oldPath]) {
    const decoded = decodeDiffPath(candidate);
    if (decoded && decoded !== "/dev/null") return decoded;
  }
  const gitHeader = lines.find((line) => line.startsWith("diff --git "));
  const match = gitHeader?.match(/^diff --git a\/(\S+) b\/(\S+)$/);
  return decodeDiffPath(match?.[2]);
}

function isUnifiedFileHeaderPair(oldLine, newLine) {
  if (!oldLine?.startsWith("--- ") || !newLine?.startsWith("+++ ")) return false;
  const oldPath = oldLine.slice(4).split("\t", 1)[0].trim();
  const newPath = newLine.slice(4).split("\t", 1)[0].trim();
  if (oldPath === "/dev/null" || newPath === "/dev/null") return true;
  const decodedOld = decodeDiffPath(oldPath);
  const decodedNew = decodeDiffPath(newPath);
  return decodedOld === decodedNew || (oldPath.startsWith("a/") && newPath.startsWith("b/"));
}

function splitUnifiedDiffFiles(value) {
  const text = String(value ?? "").trimEnd();
  if (!text) return [];
  const lines = text.split("\n");
  let starts = lines.flatMap((line, index) => line.startsWith("diff --git ") ? [index] : []);
  if (starts.length < 2) {
    starts = lines.flatMap((line, index) => (
      index < lines.length - 1 && isUnifiedFileHeaderPair(line, lines[index + 1]) ? [index] : []
    ));
  }
  if (starts.length < 2) return [[diffFilePath(text), text]];
  if (starts[0] > 0) starts[0] = 0;

  const files = [];
  for (let index = 0; index < starts.length; index += 1) {
    const start = starts[index];
    const end = index + 1 < starts.length ? starts[index + 1] : lines.length;
    const diffText = lines.slice(start, end).join("\n").trimEnd();
    files.push([diffFilePath(diffText), diffText]);
  }
  return files;
}

function splitFileChangeParts(parts) {
  const expanded = [];
  for (const part of parts) {
    if (part?.kind !== "file_change" || typeof part.text !== "string") {
      expanded.push(part);
      continue;
    }
    const files = splitUnifiedDiffFiles(part.text);
    if (files.length < 2) {
      expanded.push(part);
      continue;
    }
    let metadata = {};
    if (typeof part.metadata_json === "string") {
      try {
        const parsed = JSON.parse(part.metadata_json);
        if (parsed && typeof parsed === "object") metadata = parsed;
      } catch {
        // Keep empty metadata
      }
    }
    for (let index = 0; index < files.length; index += 1) {
      const [filePath, diffText] = files[index];
      const filePart = { ...part, text: diffText };
      if (part.content_card && typeof part.content_card === "object") {
        filePart.content_card = { ...part.content_card };
      }
      filePart.metadata_json = compactJson({
        ...metadata,
        ...(filePath ? { file_path: filePath } : {}),
        file_change_index: index + 1,
        file_change_count: files.length,
      });
      expanded.push(filePart);
    }
  }
  return expanded;
}

function structuredCardRenderer(card) {
  const cardType = card?.type;
  if (cardType === "file-change") return "diff";
  if (cardType === "code") return "code";
  if (cardType === "command") return "command";
  if (cardType === "result") {
    return ["markdown", "json"].includes(card?.format) ? card.format : "terminal_output";
  }
  if (["markdown", "plain", "json"].includes(card?.format)) return card.format;
  if (cardType === "answer") return "markdown";
  return "plain";
}

function finalizeStructuredContentCards(session) {
  for (const turn of Array.isArray(session?.turns) ? session.turns : []) {
    const parts = Array.isArray(turn?.parts) ? turn.parts : [];
    for (const part of parts) {
      let metadata = {};
      const rawMetadata = part?.metadata_json;
      if (typeof rawMetadata === "string" && rawMetadata.trim()) {
        try {
          const parsed = JSON.parse(rawMetadata);
          if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
            metadata = { ...parsed };
          }
        } catch {
          metadata = {};
        }
      }
      const legacyCard = metadata.content_card || metadata.contentCard;
      delete metadata.content_card;
      delete metadata.contentCard;
      let cardType = legacyCard && typeof legacyCard === "object" ? legacyCard.type : null;
      if (typeof cardType !== "string") {
        const kind = part?.content_card?.kind;
        cardType = typeof kind === "string" ? kind.slice(kind.lastIndexOf(".") + 1) : null;
      }
      const isFileChange = (
        part?.kind === "file_change"
        || cardType === "file-change"
        || (legacyCard && typeof legacyCard === "object" && legacyCard.format === "diff")
        || part?.content_card?.renderer === "diff"
      );
      if (isFileChange) {
        part.kind = "file_change";
        cardType = "file-change";
      }
      if (cardType === "result" && typeof part.text === "string") {
        part.text = normalizeExecutionResultText(part.text) || null;
      } else if (cardType === "command") {
        const field = typeof part.command === "string" ? "command" : "text";
        if (typeof part[field] === "string") {
          part[field] = normalizeExecutionCommandText(part[field]) || null;
        }
      }
      if (isFileChange) {
        part.content_card = {
          schema_version: 1,
          kind: "zcode.file-change",
          renderer: "diff",
        };
      } else if (!part.content_card && legacyCard && typeof legacyCard === "object" && typeof legacyCard.type === "string") {
        part.content_card = {
          schema_version: 1,
          kind: `zcode.${legacyCard.type}`,
          renderer: structuredCardRenderer(legacyCard),
        };
      }
      part.metadata_json = Object.keys(metadata).length > 0 ? compactJson(metadata) : null;
    }
    turn.parts = splitFileChangeParts(parts);
  }
  return session;
}

function run(request) {
  if (request.protocol_version !== 1) {
    throw new Error("unsupported protocol_version");
  }
  const method = String(request.method || "");
  if (!["probe", "list_sessions", "read_session"].includes(method)) {
    throw new Error(`unsupported method: ${method}`);
  }
  if (method === "probe") {
    emit({ type: "complete", item: { session_count: 0, turn_count: 0 } });
    return;
  }
  const source = request.source;
  if (!source || typeof source !== "object" || typeof source.location !== "string") {
    throw new Error("source.location is required");
  }
  const params = request.params && typeof request.params === "object" ? request.params : {};
  const sessionId = params.session_id ?? null;
  if (sessionId !== null && typeof sessionId !== "string") {
    throw new Error("params.session_id must be a string or null");
  }
  const dbPath = sourceDatabase(source.location);
  const maxSessions = configuredLimit(source.config);

  validateSchema(dbPath);
  const rows = sessionRows(dbPath, sessionId, maxSessions);

  if (method === "list_sessions") {
    for (const row of rows) {
      emit({
        type: "item",
        item: {
          kind: "session_descriptor",
          external_id: String(row.id),
          updated_at: timestamp(row.time_updated),
          source_locator: dbPath,
          version_token: sessionVersionToken(row),
        },
      });
    }
    emit({
      type: "complete",
      item: {
        session_count: rows.length,
        turn_count: 0,
        snapshot_complete: true,
      },
    });
    return;
  }

  // read_session
  const sessions = [];
  for (const row of rows) {
    const turns = loadTurns(dbPath, String(row.id));
    if (!turns.length) continue;
    sessions.push({
      external_id: String(row.id),
      title: row.title != null ? String(row.title) : null,
      project_path: row.project_path != null ? String(row.project_path) : null,
      started_at: turns[0]?.started_at ?? null,
      updated_at: timestamp(row.time_updated),
      source_locator: dbPath,
      source_fingerprint: sessionVersionToken(row),
      turns,
    });
  }

  if (sessionId && rows.length > 0) {
    const beforeToken = sessionVersionToken(rows[0]);
    const returnedToken = sessions.length > 0 ? sessions[0].source_fingerprint : null;
    if (returnedToken && beforeToken !== returnedToken) {
      throw new Error(`ZCode session changed while it was being read: ${sessionId}`);
    }
  }

  let turnCount = 0;
  for (const session of sessions) {
    turnCount += session.turns.length;
    emit({
      type: "item",
      item: {
        kind: "session",
        session: finalizeStructuredContentCards(session),
      },
    });
  }

  emit({
    type: "complete",
    item: {
      session_count: sessions.length,
      turn_count: turnCount,
      snapshot_complete: null,
    },
  });
}

function main() {
  try {
    const raw = readFileSync(0, "utf8").trim();
    if (!raw) {
      throw new Error("adapter request must be a JSON object");
    }
    const request = JSON.parse(raw);
    if (!request || typeof request !== "object" || Array.isArray(request)) {
      throw new Error("adapter request must be a JSON object");
    }
    run(request);
  } catch (error) {
    fail(error);
  }
}

main();
