#!/usr/bin/env node
import { normalizeSessionPayload } from "./payload-policy.mjs";
import shellProjector from "./shell-projector.cjs";
const { projectCommandParts, SHELL_PROJECTOR_VERSION } = shellProjector;
import { createHash } from "node:crypto";
import { existsSync, readFileSync, statSync } from "node:fs";
import { homedir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const input = JSON.parse(readFileSync(0, "utf8") || "{}");
const SQLITE_MAX_BUFFER_BYTES = 64 * 1024 * 1024;
const CONTENT_CARD_SCHEMA_VERSION = "opencode-content-cards-v7";

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

function sleepSync(ms) {
  try {
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
  } catch {
    const end = Date.now() + ms;
    while (Date.now() < end) {}
  }
}

function sqliteJson(dbPath, sql, maxRetries = 3) {
  const resolved = path.resolve(expandPath(dbPath));
  const uri = `file:${resolved}?mode=ro`;
  let lastError = null;

  for (let attempt = 0; attempt < maxRetries; attempt++) {
    let result = spawnSync("sqlite3", ["-json", "-cmd", ".timeout 10000", uri, sql], {
      encoding: "utf8",
      maxBuffer: SQLITE_MAX_BUFFER_BYTES,
    });

    if (result.status !== 0 && result.stderr && (result.stderr.includes("unable to open database") || result.stderr.includes("URI"))) {
      result = spawnSync("sqlite3", ["-json", "-cmd", ".timeout 10000", resolved, sql], {
        encoding: "utf8",
        maxBuffer: SQLITE_MAX_BUFFER_BYTES,
      });
    }

    if (!result.error && result.status === 0) {
      const text = result.stdout.trim();
      return text ? JSON.parse(text) : [];
    }

    const errMsg = (result.error ? result.error.message : result.stderr || `sqlite3 exited with ${result.status}`).trim();
    lastError = new Error(errMsg);

    if (errMsg.includes("database is locked") || errMsg.includes("busy")) {
      sleepSync(200 * (attempt + 1));
      continue;
    }
    break;
  }

  throw lastError;
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
  const toolName = String(valueText(data, ["tool", "tool_name"]) ?? "").trim().toLowerCase();
  const declaredType = String(valueText(data, ["type", "kind"]) ?? kind).trim().toLowerCase();
  // OpenCode 的 `task` 工具调用就是子代理派发步：这一步原位产出轻量锚点，
  // 不落普通 tool Part，也不带 content_card（后端映射成 opencode.subagent）。
  if (kind !== "text" && (toolName === "task" || ["subtask", "agent"].includes(declaredType))) {
    const prompt = row.subagent_prompt?.trim()
      || valueText(data, ["prompt"])?.trim()
      || null;
    const title = row.description?.trim()
      || row.title?.trim()
      || valueText(data, ["description", "title"])?.trim()
      || sanitizeSessionShortTitle(prompt)
      || null;
    const agentRole = row.subagent_type?.trim()
      || valueText(data, ["subagent_type", "subagentType", "agentType", "role"])?.trim()
      || null;
    const childExternalId = row.child_session_id?.trim()
      || valueText(data, ["sessionId", "session_id", "sessionID", "childSessionId", "child_session_id"])?.trim()
      || null;
    const rawStatus = String(
      valueText(data, ["status"]) ?? nestedText(data, [["state", "status"]]) ?? "",
    ).trim().toLowerCase();
    const status = /^(error|failed|failure|cancelled|canceled|interrupted)$/.test(rawStatus)
      ? "failed"
      : rawStatus === "running" || rawStatus === "pending"
        ? rawStatus
        : "completed";
    const spawnAnchor = typeof row.source_execution_id === "string" && row.source_execution_id.trim()
      ? row.source_execution_id.trim()
      : typeof row.id === "string" && row.id.trim()
        ? row.id.trim()
        : null;
    return [subagentAnchorPart({
      childExternalId,
      spawnAnchor,
      agentRole,
      title,
      status,
      summary: summarizeReport(text),
    })];
  }
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
    const part = textPart(normalizedRole, text);
    return part ? [part] : [];
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
  const sourceExecutionId = typeof row.source_execution_id === "string" && row.source_execution_id.trim()
    ? row.source_execution_id.trim()
    : null;
  const resultText = String(text ?? "").trim();
  const rawLabel = valueText(data, ["title", "description", "tool"]);
  const commandLabel = rawLabel?.trim() || null;
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
        source_execution_id: sourceExecutionId,
        command_label: commandLabel,
        metadata_json: metadata(compactObject({ type: "command", cwd, command_label: commandLabel }), smallMetadata(data)),
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
        source_execution_id: sourceExecutionId,
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

function resolvePrompt(...candidates) {
  for (const c of candidates) {
    if (typeof c === "string" && c.trim().length > 0) return c.trim();
  }
  return "Subagent execution";
}

const SUBAGENT_TITLE_SUFFIX = /\(@([^\s()]+)\s+subagent\)\s*$/i;
/** 汇报摘要上限：只用于父轮次锚点，完整汇报留在子会话里 */
const SUBAGENT_REPORT_SUMMARY_MAX = 240;

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

/** 锚点 metadata 的稳定字段顺序，便于跨宿主对账与快照 diff。 */
function orderedAnchorMetadata(metadata) {
  const ordered = {};
  if (metadata?.child_external_id) ordered.child_external_id = metadata.child_external_id;
  if (metadata?.agent_role) ordered.agent_role = metadata.agent_role;
  if (metadata?.title) ordered.title = metadata.title;
  ordered.status = metadata?.status ?? "completed";
  if (metadata?.model) ordered.model = metadata.model;
  if (Number.isInteger(metadata?.step_count)) ordered.step_count = metadata.step_count;
  if (metadata?.report_summary) ordered.report_summary = metadata.report_summary;
  if (metadata?.spawn_anchor) ordered.spawn_anchor = metadata.spawn_anchor;
  return ordered;
}

/**
 * 构造父轮次里的轻量委派锚点 Part（ADR-0022）。
 *
 * 锚点不复制子代理执行过程，只保留任务标题与结论摘要；子会话的确定性
 * `external_id` 写进 metadata，后端据此折算子会话主键。锚点不带 content_card，
 * 后端会把 `kind = "subagent"` 自动映射成 `opencode.subagent` 卡片。
 */
function subagentAnchorPart({ childExternalId, spawnAnchor, agentRole, title, status, summary }) {
  const normalizedTitle = collapseWhitespace(title) || null;
  const metadata = orderedAnchorMetadata({
    child_external_id: childExternalId ?? null,
    agent_role: agentRole ?? null,
    title: normalizedTitle,
    status: status ?? "completed",
    report_summary: summary ?? null,
    spawn_anchor: spawnAnchor ?? null,
  });
  return {
    role: "assistant",
    kind: "subagent",
    text: anchorText(normalizedTitle, metadata.report_summary),
    language: null,
    command: null,
    cwd: null,
    status: metadata.status,
    exit_code: null,
    source_execution_id: spawnAnchor ?? null,
    metadata_json: JSON.stringify(metadata),
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

/** 就地更新锚点：补角色、标题、步骤数、模型与汇报摘要，并同步刷新正文。 */
function updateSubagentAnchor(part, patch) {
  if (!part) return;
  const merged = { ...parseMetadataObject(part), ...patch };
  for (const key of Object.keys(merged)) {
    if (merged[key] === null || merged[key] === undefined) delete merged[key];
  }
  const metadata = orderedAnchorMetadata(merged);
  part.metadata_json = JSON.stringify(metadata);
  part.text = anchorText(metadata.title, metadata.report_summary);
  part.status = metadata.status;
  if (metadata.spawn_anchor) part.source_execution_id = metadata.spawn_anchor;
}

// OpenCode titles child sessions as "<task description> (@<type> subagent)".
// The type is the only stable role name; the description belongs to the turn title.
function parseSubagentTitle(title) {
  const raw = typeof title === "string" ? title.trim() : "";
  if (!raw) return { agentRole: null, description: null };
  const suffix = raw.match(SUBAGENT_TITLE_SUFFIX);
  if (!suffix) return { agentRole: null, description: raw };
  return {
    agentRole: suffix[1],
    description: raw.slice(0, suffix.index).trim() || null,
  };
}

// ADR-0022：父会话里 1 次人类提问 = 1 个真实轮次，子代理只留锚点 Part，
// 因此这里不再按 turn_kind / parent_turn_id 保留伪轮次。
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

const DEFAULT_PATCH_BUDGET_BYTES = 16 * 1024 * 1024;

function getPatchBudget() {
  const envVal = process.env.ASSETIWEAVE_PATCH_BUDGET_BYTES;
  if (envVal) {
    const parsed = parseInt(envVal, 10);
    if (!isNaN(parsed) && parsed > 0) return parsed;
  }
  return DEFAULT_PATCH_BUDGET_BYTES;
}

function canonicalSummaryDiff(entries) {
  if (!Array.isArray(entries)) return null;
  const validEntries = [];
  let totalBytes = 0;

  for (const entry of entries) {
    if (!entry || typeof entry !== "object" || typeof entry.patch !== "string" || !entry.patch.trim()) continue;
    const filePath = String(entry.file ?? entry.path ?? "").replace(/^[./\\]+/, "").replaceAll("\\", "/");
    if (!filePath) continue;
    const patch = entry.patch.replace(/\r\n?/g, "\n").trim();
    totalBytes += Buffer.byteLength(patch, "utf8");
    validEntries.push({ filePath, patch, status: entry.status });
  }

  if (validEntries.length === 0) return null;

  const budget = getPatchBudget();
  if (totalBytes > budget) {
    return {
      reduced: true,
      fileCount: validEntries.length,
      originalBytes: totalBytes,
      filePaths: validEntries.map((e) => e.filePath),
    };
  }

  const files = [];
  for (const { filePath, patch, status: rawStatus } of validEntries) {
    if (patch.startsWith("diff --git ")) {
      files.push(patch);
      continue;
    }
    const status = String(rawStatus ?? "modified").toLowerCase();
    const oldPath = status === "added" || status === "new" ? "/dev/null" : `a/${filePath}`;
    const newPath = status === "deleted" || status === "removed" ? "/dev/null" : `b/${filePath}`;
    files.push([
      `diff --git a/${filePath} b/${filePath}`,
      `--- ${oldPath}`,
      `+++ ${newPath}`,
      patch,
    ].join("\n"));
  }
  return {
    reduced: false,
    text: files.join("\n"),
  };
}

function attachSummaryDiff(parts, summaryDiffs) {
  const result = canonicalSummaryDiff(summaryDiffs);
  if (!result) return false;
  const target = parts.find((part) => part?.kind === "file_change" && !part.text);
  if (!target) return false;

  if (result.reduced) {
    const textLines = [
      "# File changes reduced: patch payload exceeded safety budget",
      `# Total files changed: ${result.fileCount}`,
      `# Total original patch size: ${result.originalBytes} bytes`,
      "# Retained representation: available file paths and summary statistics",
      "",
      "Files:",
      ...result.filePaths.map((p) => `- ${p}`),
      "",
      "[Content truncated]",
    ];
    target.text = textLines.join("\n");
    const meta = parseStructuredMetadata(target.metadata_json);
    meta.truncated = true;
    meta.content_reduced = true;
    meta.original_file_count = result.fileCount;
    meta.original_bytes = result.originalBytes;
    meta.retained_representation = "file_paths_and_statistics";
    meta.truncation_reason = "patch_payload_exceeded_safety_budget";
    target.metadata_json = JSON.stringify(meta);

    emit("warning", {
      message: `OpenCode summary diff content reduced (${result.fileCount} files, ${result.originalBytes} bytes): exceeded safety budget`,
    });
    return true;
  }

  target.text = result.text;
  return true;
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
  const partId = pick(partColumns, ["id", "part_id", "partID"]);
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
  // 子代理派发步的稳定标识：child session id / 角色 / 委派 Prompt。
  const partChildSessionExpr = partData
    ? sqlCoalesce([
      jsonExtract(partData, "$.state.metadata.sessionId"),
      jsonExtract(partData, "$.state.metadata.sessionID"),
      jsonExtract(partData, "$.metadata.sessionId"),
      jsonExtract(partData, "$.sessionId"),
      jsonExtract(partData, "$.session_id"),
      jsonExtract(partData, "$.childSessionId"),
    ])
    : "NULL";
  const partSubagentTypeExpr = partData
    ? sqlCoalesce([
      jsonExtract(partData, "$.state.input.subagent_type"),
      jsonExtract(partData, "$.input.subagent_type"),
      jsonExtract(partData, "$.subagent_type"),
      jsonExtract(partData, "$.subagentType"),
    ])
    : "NULL";
  const partSubagentPromptExpr = partData
    ? sqlCoalesce([
      jsonExtract(partData, "$.state.input.prompt"),
      jsonExtract(partData, "$.input.prompt"),
      jsonExtract(partData, "$.prompt"),
    ])
    : "NULL";
  const sourceExecutionExpressions = partData
    ? [
      jsonExtract(partData, "$.callID"),
      jsonExtract(partData, "$.call_id"),
      jsonExtract(partData, "$.tool_call_id"),
      jsonExtract(partData, "$.state.callID"),
      jsonExtract(partData, "$.state.call_id"),
    ]
    : [];
  if (partId) sourceExecutionExpressions.push(quoteIdent(partId));
  const sourceExecutionExpr = sourceExecutionExpressions.length
    ? sqlCoalesce(sourceExecutionExpressions)
    : "NULL";
  const partSql = partMessage
    ? `SELECT ${partSession ? quoteIdent(partSession) : "NULL"} AS session_id, ${quoteIdent(partMessage)} AS message_id, ${partKindExpr} AS kind, ${partTextExpr} AS text, NULL AS data, ${partCommandExpr} AS command, ${partCwdExpr} AS cwd, ${partStatusExpr} AS status, ${partExitExpr} AS exit_code, ${partToolExpr} AS tool, ${partTitleExpr} AS title, ${partDescriptionExpr} AS description, ${partChildSessionExpr} AS child_session_id, ${partSubagentTypeExpr} AS subagent_type, ${partSubagentPromptExpr} AS subagent_prompt, ${sourceExecutionExpr} AS source_execution_id FROM part ${partWhere} ORDER BY rowid ASC`
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
  const summaryDiffsExpr = dataCol
    ? `CASE
         WHEN ${quoteIdent(dataCol)} IS NOT NULL
          AND json_valid(${quoteIdent(dataCol)})
          AND json_type(${quoteIdent(dataCol)}, '$.summary.diffs') = 'array'
         THEN (
           SELECT json_group_array(
             json_object(
               'file', json_extract(value, '$.file'),
               'path', json_extract(value, '$.path'),
               'status', json_extract(value, '$.status'),
               'patch', json_extract(value, '$.patch')
             )
           )
           FROM json_each(${quoteIdent(dataCol)}, '$.summary.diffs')
           WHERE json_extract(value, '$.patch') IS NOT NULL
             AND trim(json_extract(value, '$.patch')) != ''
         )
         ELSE NULL
       END`
    : "NULL";
  const msgSql = `SELECT ${quoteIdent(msgId)} AS id, ${quoteIdent(msgSession)} AS session_id, ${roleExpr} AS role, ${timestampExpr} AS timestamp, ${dataCol ? quoteIdent(dataCol) : "NULL"} AS data, ${summaryDiffsExpr} AS summary_diffs FROM message WHERE ${quoteIdent(msgSession)} IN (${sessionList}) ORDER BY rowid ASC`;
  const currentBySession = new Map();
  const summaryDiffsBySession = new Map();
  for (const message of sqliteJson(dbPath, msgSql)) {
    const sessionId = String(message.session_id);
    const data = parseJson(message.data);
    const role = message.role ?? valueText(data, ["role", "author"]) ?? "";
    const timestamp = message.timestamp ?? valueText(data, ["time", "created_at"]);
    const model = valueText(data, ["modelID", "model", "model_id"]);
    const partKey = partSession
      ? messageKey(sessionId, String(message.id))
      : String(message.id);
    const parts = (partsByMessage.get(partKey) ?? [])
      .flatMap((row) => normalizePart(row, role) ?? [])
      .filter(Boolean);
    if (String(role).toLowerCase() === "user") {
      const summaryDiffs = parseJson(message.summary_diffs);
      if (Array.isArray(summaryDiffs) && summaryDiffs.length > 0) {
        summaryDiffsBySession.set(sessionId, summaryDiffs);
      }
      const userText = parts
        .filter((part) => part.role === "user" && part.kind === "text")
        .map((part) => part.text)
        .filter(Boolean)
        .join("\n\n");
      const effectiveUserText = resolvePrompt(userText, "User message");
      const turns = turnsForSession(turnsBySession, sessionId);
      const current = currentBySession.get(sessionId);
      if (current) turns.push(current);
      currentBySession.set(sessionId, {
        external_id: String(message.id),
        turn_index: turns.length,
        user_text: effectiveUserText,
        title: null,
        started_at: timestamp == null ? null : String(timestamp),
        ended_at: null,
        model: null,
        parts: parts.filter((part) => part.role !== "user"),
      });
    } else if (currentBySession.has(sessionId)) {
      const current = currentBySession.get(sessionId);
      if (model && !current.model) {
        current.model = model;
      }
      if (attachSummaryDiff(parts, summaryDiffsBySession.get(sessionId))) {
        summaryDiffsBySession.delete(sessionId);
      }
      current.parts.push(...parts);
      current.ended_at = timestamp == null ? current.ended_at : String(timestamp);
    }
  }
  for (const [sessionId, current] of currentBySession) {
    turnsForSession(turnsBySession, sessionId).push(current);
  }
  return turnsBySession;
}

function sessionQueryContext(dbPath) {
  const sessionColumns = sqliteJson(dbPath, "PRAGMA table_info(session)").map((row) => row.name);
  const messageColumns = sqliteJson(dbPath, "PRAGMA table_info(message)").map((row) => row.name);
  const partColumns = sqliteJson(dbPath, "PRAGMA table_info(part)").map((row) => row.name);
  const idCol = pick(sessionColumns, ["id", "session_id"]);
  const messageSessionCol = pick(messageColumns, ["session_id", "sessionID", "session"]);
  if (!idCol || !messageColumns.length || !partColumns.length || !messageSessionCol) return null;
  const messageIdCol = pick(messageColumns, ["id", "message_id"]);
  const partSessionCol = pick(partColumns, ["session_id", "sessionID", "session"]);
  const partMessageCol = pick(partColumns, ["message_id", "messageID", "message"]);
  return {
    messageColumns,
    partColumns,
    idCol,
    titleCol: pick(sessionColumns, ["title", "name"]),
    projectCol: pick(sessionColumns, ["project", "project_path", "cwd", "directory", "path"]),
    updatedCol: pick(sessionColumns, ["updated_at", "time_updated", "timeUpdated", "created_at"]),
    parentCol: pick(sessionColumns, ["parent_id", "parentID", "parent_session_id", "parent"]),
    messageVersionExpr: `(SELECT COALESCE(MAX(m.rowid), 0) FROM message m WHERE m.${quoteIdent(messageSessionCol)} = s.${quoteIdent(idCol)})`,
    partVersionExpr: partSessionCol
      ? `(SELECT COALESCE(MAX(p.rowid), 0) FROM part p WHERE p.${quoteIdent(partSessionCol)} = s.${quoteIdent(idCol)})`
      : messageIdCol && partMessageCol
        ? `(SELECT COALESCE(MAX(p.rowid), 0) FROM part p JOIN message m ON p.${quoteIdent(partMessageCol)} = m.${quoteIdent(messageIdCol)} WHERE m.${quoteIdent(messageSessionCol)} = s.${quoteIdent(idCol)})`
        : "0",
  };
}

function sessionSelectSql(context) {
  const { idCol, titleCol, projectCol, updatedCol, parentCol, messageVersionExpr, partVersionExpr } = context;
  const parentExpr = parentCol ? `s.${quoteIdent(parentCol)}` : "NULL";
  return `SELECT s.${quoteIdent(idCol)} AS id, ${titleCol ? `s.${quoteIdent(titleCol)}` : "NULL"} AS title, ${projectCol ? `s.${quoteIdent(projectCol)}` : "NULL"} AS project_path, ${updatedCol ? `s.${quoteIdent(updatedCol)}` : "NULL"} AS updated_at, ${parentExpr} AS parent_id, ${messageVersionExpr} AS message_version, ${partVersionExpr} AS part_version FROM session s`;
}

function allSessionRows(context, dbPath) {
  return sqliteJson(dbPath, `${sessionSelectSql(context)} ORDER BY s.rowid DESC`);
}

function sessionRowsById(context, dbPath, sessionId) {
  const sql = `${sessionSelectSql(context)} WHERE s.${quoteIdent(context.idCol)} = ${sqlString(sessionId)} ORDER BY s.rowid DESC`;
  return sqliteJson(dbPath, sql);
}

function topLevelSessionRows(context, dbPath) {
  const where = context.parentCol
    ? ` WHERE (s.${quoteIdent(context.parentCol)} IS NULL OR s.${quoteIdent(context.parentCol)} = '')`
    : "";
  return sqliteJson(dbPath, `${sessionSelectSql(context)}${where} ORDER BY s.rowid DESC`);
}

function childRowsForParents(context, dbPath, parentIds) {
  if (!context.parentCol || parentIds.length === 0) return [];
  const ids = parentIds.map(sqlString).join(", ");
  const sql = `${sessionSelectSql(context)} WHERE s.${quoteIdent(context.parentCol)} IN (${ids}) ORDER BY s.rowid ASC`;
  try {
    return sqliteJson(dbPath, sql);
  } catch {
    return [];
  }
}

function summarizeChildTurns(turns) {
  let stepCount = 0;
  let reportSummary = null;
  for (const turn of turns) {
    for (const part of Array.isArray(turn?.parts) ? turn.parts : []) {
      if (part.kind === "command" || part.kind === "tool" || part.kind === "file_change") stepCount += 1;
      if (part.role === "assistant" && (part.kind === "text" || part.kind === "code") && part.text?.trim()) {
        reportSummary = part.text.trim();
      }
    }
  }
  return {
    stepCount,
    reportSummary: summarizeReport(reportSummary),
    model: turns.map((turn) => turn.model).find(Boolean) ?? null,
  };
}

/**
 * 把一个子会话行组装成独立的 `Subagent Session`（ADR-0022）。
 *
 * `external_id` 直接用 OpenCode 原生 session id；父子溯源写进
 * `execution_purpose.parent_external_id`，后端入库时再折算成父会话主键。
 */
function childSessionEntry(childRow, childTurns, dbPath, parentExternalId) {
  const parsed = parseSubagentTitle(childRow.title);
  const rawTitle = childRow.title == null ? null : String(childRow.title).trim();
  const title = parsed.description
    || sanitizeSessionShortTitle(childTurns[0]?.user_text)
    || rawTitle
    || null;
  return {
    parsed,
    summary: summarizeChildTurns(childTurns),
    agentRole: parsed.agentRole ?? null,
    spawnAnchor: null,
    parentTurnIndex: null,
    parentExternalId,
    session: {
      external_id: String(childRow.id),
      title,
      project_path: childRow.project_path == null ? null : String(childRow.project_path),
      started_at: childTurns[0]?.started_at ?? null,
      updated_at: childTurns.at(-1)?.ended_at ?? childTurns.at(-1)?.started_at ?? (childRow.updated_at == null ? null : String(childRow.updated_at)),
      source_locator: dbPath,
      source_fingerprint: sessionVersionToken(childRow),
      user_visible: false,
      execution_origin: "subagent",
      execution_purpose: null,
      turns: childTurns,
    },
  };
}

/** 把子会话的权威信息回填进父轮次锚点，并交换双方的互相引用。 */
function applyAnchorLink(anchor, entry) {
  const meta = anchor.metadata ?? {};
  const spawnAnchor = typeof meta.spawn_anchor === "string" && meta.spawn_anchor.trim()
    ? meta.spawn_anchor.trim()
    : null;
  const agentRole = entry.agentRole ?? meta.agent_role ?? null;
  const title = meta.title ?? entry.session.title ?? null;
  const reportSummary = entry.summary.reportSummary ?? meta.report_summary ?? null;
  const patch = {
    child_external_id: entry.session.external_id,
    agent_role: agentRole,
    title,
    status: "completed",
    step_count: entry.summary.stepCount,
    report_summary: reportSummary,
  };
  if (entry.summary.model) patch.model = entry.summary.model;
  if (spawnAnchor) patch.spawn_anchor = spawnAnchor;
  updateSubagentAnchor(anchor.part, patch);
  entry.agentRole = agentRole ?? entry.agentRole;
  entry.spawnAnchor = spawnAnchor;
  entry.parentTurnIndex = anchor.turn.turn_index;
  anchor.metadata = parseMetadataObject(anchor.part);
}

function anchorHostTurn(parentTurns, entry) {
  const startedAt = entry.session.started_at;
  if (!startedAt) return parentTurns.at(-1) ?? null;
  let host = null;
  for (const turn of parentTurns) {
    if (turn.started_at && turn.started_at <= startedAt) host = turn;
  }
  return host ?? parentTurns[0] ?? null;
}

/**
 * 父轮次锚点与子会话的配对：先按 `child_external_id` 精确匹配（宿主写入
 * `state.metadata.sessionId` 的情形），再按派发顺序兜底（历史数据缺 sessionId）。
 */
function linkChildrenToAnchors(parentTurns, childEntries) {
  if (childEntries.length === 0) return;
  const anchors = [];
  for (const turn of parentTurns) {
    for (const part of Array.isArray(turn.parts) ? turn.parts : []) {
      if (part?.kind !== "subagent") continue;
      anchors.push({ turn, part, metadata: parseMetadataObject(part), used: false });
    }
  }

  const unmatched = [];
  for (const entry of childEntries) {
    const childId = entry.session.external_id;
    const match = anchors.find((anchor) => !anchor.used
      && anchor.metadata.child_external_id
      && String(anchor.metadata.child_external_id) === childId);
    if (match) {
      match.used = true;
      applyAnchorLink(match, entry);
    } else {
      unmatched.push(entry);
    }
  }

  const freeAnchors = anchors.filter((anchor) => !anchor.used);
  while (unmatched.length > 0 && freeAnchors.length > 0) {
    const anchor = freeAnchors.shift();
    const entry = unmatched.shift();
    anchor.used = true;
    applyAnchorLink(anchor, entry);
  }

  // 宿主没有留下 task 工具调用步（旧库/异常中断）时，仍在包含该子会话的
  // 父轮次末尾补一个锚点，保证“派发必有锚点”的契约成立。
  for (const entry of unmatched) {
    const host = anchorHostTurn(parentTurns, entry);
    if (!host) continue;
    const anchorPart = subagentAnchorPart({
      childExternalId: entry.session.external_id,
      spawnAnchor: entry.session.external_id,
      agentRole: entry.agentRole,
      title: entry.session.title,
      status: "completed",
      summary: entry.summary.reportSummary,
    });
    host.parts.push(anchorPart);
    applyAnchorLink({ turn: host, part: anchorPart, metadata: parseMetadataObject(anchorPart) }, entry);
  }

  for (const entry of childEntries) {
    entry.session.execution_purpose = JSON.stringify(compactObject({
      parent_external_id: entry.parentExternalId,
      parent_turn_index: entry.parentTurnIndex,
      agent_role: entry.agentRole,
      spawn_anchor: entry.spawnAnchor,
    }));
  }
}

/**
 * 解析一批顶层会话：每个父会话产出 `[父会话, ...直接子会话]`。
 *
 * 父会话的轮次只按真实人类提问切分；子代理派发步在 parts 序列的原位置
 * 保留一个 `kind = "subagent"` 锚点，不再产生 subagent / continuation 伪轮次。
 */
function buildSessionBundles(rootRows, context, dbPath) {
  const roots = rootRows.filter((row) => !isInternalExecutionPath(row.project_path));
  if (roots.length === 0) return [];
  const rootIds = roots.map((row) => String(row.id));
  const childRows = childRowsForParents(context, dbPath, rootIds)
    .filter((row) => !isInternalExecutionPath(row.project_path));
  const childrenByParent = new Map();
  for (const childRow of childRows) {
    const parentId = String(childRow.parent_id);
    const list = childrenByParent.get(parentId) ?? [];
    list.push(childRow);
    childrenByParent.set(parentId, list);
  }
  const turnsBySession = readTurnsBySession(
    dbPath,
    context.messageColumns,
    context.partColumns,
    [...rootIds, ...childRows.map((row) => String(row.id))],
  );

  const bundles = [];
  for (const row of roots) {
    const sessionId = String(row.id);
    const parentTurns = displayTurns(turnsBySession.get(sessionId) ?? []);
    if (parentTurns.length === 0) continue;
    const childEntries = (childrenByParent.get(sessionId) ?? []).map((childRow) => childSessionEntry(
      childRow,
      displayTurns(turnsBySession.get(String(childRow.id)) ?? []),
      dbPath,
      sessionId,
    ));
    linkChildrenToAnchors(parentTurns, childEntries);
    const parentSession = {
      external_id: sessionId,
      title: (row.title == null || !String(row.title).trim())
        ? (sanitizeSessionShortTitle(parentTurns[0]?.user_text) || null)
        : String(row.title).trim(),
      project_path: row.project_path == null ? null : String(row.project_path),
      started_at: parentTurns[0]?.started_at ?? null,
      updated_at: parentTurns.at(-1)?.ended_at ?? parentTurns.at(-1)?.started_at ?? (row.updated_at == null ? null : String(row.updated_at)),
      source_locator: dbPath,
      source_fingerprint: sessionVersionToken(row),
      user_visible: true,
      execution_origin: "opencode",
      execution_purpose: null,
      turns: parentTurns,
    };
    bundles.push([parentSession, ...childEntries.map((entry) => entry.session)]);
  }
  return bundles;
}

function buildOrphanChildSession(childRow, childTurns, dbPath, parentExternalId) {
  const entry = childSessionEntry(childRow, childTurns, dbPath, parentExternalId);
  entry.session.execution_purpose = JSON.stringify(compactObject({
    parent_external_id: parentExternalId,
    agent_role: entry.agentRole,
  }));
  return entry.session;
}

function readSession() {
  const requestedSessionId = input.params?.session_id ?? null;
  const dbPath = expandPath(input.source?.location);
  if (!dbPath || !existsSync(dbPath)) return [];
  const context = sessionQueryContext(dbPath);
  if (!context) return [];

  if (!requestedSessionId) {
    return buildSessionBundles(topLevelSessionRows(context, dbPath), context, dbPath).flat();
  }

  const targetRows = sessionRowsById(context, dbPath, String(requestedSessionId));
  if (targetRows.length === 0) return [];
  const target = targetRows[0];
  if (!context.parentCol || !target.parent_id) {
    return buildSessionBundles(targetRows, context, dbPath).flat();
  }

  // 直接读取子会话：连同父会话一起解析，才能得到与父会话打包时一致的
  // parent_turn_index / spawn_anchor；返回时只保留该子会话本身。
  const parentId = String(target.parent_id);
  const parentRows = sessionRowsById(context, dbPath, parentId);
  if (parentRows.length > 0) {
    for (const bundle of buildSessionBundles(parentRows, context, dbPath)) {
      const child = bundle.find((session) => session.external_id === String(target.id));
      if (child) return [child];
    }
  }
  const childTurns = displayTurns(
    readTurnsBySession(dbPath, context.messageColumns, context.partColumns, [String(target.id)])
      .get(String(target.id)) ?? [],
  );
  return [buildOrphanChildSession(target, childTurns, dbPath, parentId)];
}

function listSessions() {
  const dbPath = expandPath(input.source?.location);
  if (!dbPath || !existsSync(dbPath)) return [];
  const context = sessionQueryContext(dbPath);
  if (!context) return [];
  // 子代理会话也必须出现在描述符里：增量同步靠这份集合给缺失会话打 missing，
  // 顶层列表由后端 user_visible = 0 过滤，不能在这里隐藏。
  return allSessionRows(context, dbPath)
    .filter((row) => !isInternalExecutionPath(row.project_path))
    .map((row) => ({
      external_id: String(row.id),
      updated_at: row.updated_at == null ? null : String(row.updated_at),
      source_locator: dbPath,
      version_token: sessionVersionToken(row),
    }));
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
          kind: `opencode.${legacyCard.type}`,
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
 * 查询 OpenCode 本地 SQLite (opencode.db) 中的 message 表，提取 message.data 内记录的 Token 用量与模型消耗。
 */
function readUsage() {
  const location = expandPath(input.source?.location);
  if (!location) {
    emit("complete", { item: { snapshot_complete: true, usage_event_count: 0, next_cursor: null, decoder_profile: "opencode-sqlite-v1", diagnostics: [] } });
    return;
  }
  let dbPath = location;
  if (existsSync(location) && statSync(location).isDirectory()) {
    dbPath = path.join(location, "opencode.db");
  }
  if (!existsSync(dbPath)) {
    emit("complete", { item: { snapshot_complete: true, usage_event_count: 0, next_cursor: null, decoder_profile: "opencode-sqlite-v1", diagnostics: [] } });
    return;
  }

  const cursorRaw = input.params?.cursor;
  let cursor = null;
  if (cursorRaw) {
    try {
      cursor = typeof cursorRaw === "string" ? JSON.parse(cursorRaw) : cursorRaw;
    } catch {}
  }
  const lastTime = cursor?.last_time_created ? Number(cursor.last_time_created) : 0;

  const messageColumns = sqliteJson(dbPath, "PRAGMA table_info(message)").map((row) => row.name);
  const hasTimeCreated = messageColumns.includes("time_created");
  const timeExpr = hasTimeCreated ? "time_created" : "NULL AS time_created";

  const whereClause = (hasTimeCreated && lastTime > 0)
    ? `WHERE time_created > ${lastTime} AND data LIKE '%"tokens"%'`
    : `WHERE data LIKE '%"tokens"%'`;
  const orderClause = hasTimeCreated ? "ORDER BY time_created ASC, id ASC" : "ORDER BY rowid ASC";

  const sql = `SELECT id, session_id, ${timeExpr}, data FROM message ${whereClause} ${orderClause} LIMIT 10000;`;
  let rows = [];
  try {
    rows = sqliteJson(dbPath, sql);
  } catch (err) {
    fail(`failed to query opencode sqlite: ${err.message}`);
    return;
  }

  let maxTime = lastTime;
  let eventCount = 0;

  for (const row of rows) {
    let data;
    try {
      data = typeof row.data === "string" ? JSON.parse(row.data) : row.data;
    } catch {
      continue;
    }
    if (!data || typeof data !== "object") continue;

    let timeCreated = Number(row.time_created || 0);
    if (!timeCreated || isNaN(timeCreated) || timeCreated <= 0) {
      const candidate = data.time?.created ?? data.time?.created_at ?? data.created_at;
      if (typeof candidate === "number" && !isNaN(candidate) && candidate > 0) {
        timeCreated = candidate;
      } else if (typeof candidate === "string") {
        const parsed = Date.parse(candidate);
        if (!isNaN(parsed) && parsed > 0) {
          timeCreated = parsed;
        }
      }
    }

    if (timeCreated > maxTime) {
      maxTime = timeCreated;
    }

    if (!hasTimeCreated && lastTime > 0 && timeCreated > 0 && timeCreated <= lastTime) {
      continue;
    }

    const tokens = data.tokens;
    if (!tokens || typeof tokens !== "object") continue;

    const inputTokens = Number(tokens.input || 0);
    const outputTokens = Number(tokens.output || 0);
    const reasoningTokens = Number(tokens.reasoning || 0);
    const cacheReadTokens = Number(tokens.cache?.read || 0);
    const cacheWriteTokens = Number(tokens.cache?.write || 0);
    const totalTokens = Number(tokens.total || (inputTokens + outputTokens + reasoningTokens + cacheReadTokens + cacheWriteTokens));

    if (totalTokens <= 0) continue;

    const sessionId = String(row.session_id);
    const rawId = String(row.id);
    const turnId = rawId.startsWith("msg-") ? rawId : `msg-${rawId}`;
    const eventId = `${sessionId}-${turnId}`;
    let timestamp;
    if (timeCreated > 0) {
      timestamp = new Date(timeCreated).toISOString();
    } else {
      const candidate = data.time?.created ?? data.time?.created_at ?? data.created_at;
      if (typeof candidate === "string" && !isNaN(Date.parse(candidate))) {
        timestamp = new Date(candidate).toISOString();
      } else {
        timestamp = new Date().toISOString();
      }
    }
    const modelStr = String(data.modelID || data.model || "unknown");
    const providerStr = String(data.providerID || data.provider || "opencode");
    const cost = typeof data.cost === "number" ? data.cost : null;

    const event = {
      external_event_id: eventId,
      session_id: sessionId,
      turn_id: turnId,
      logical_request_id: eventId,
      attempt_index: 0,
      timestamp,
      provider: providerStr,
      model: modelStr,
      input_tokens: inputTokens,
      output_tokens: outputTokens,
      cache_read_tokens: cacheReadTokens,
      cache_write_tokens: cacheWriteTokens,
      reasoning_tokens: reasoningTokens,
      total_tokens: totalTokens,
      status: "success",
      currency: null,
      cost,
      metadata: {
        mode: data.mode,
        agent: data.agent,
      },
    };
    emit("usage_event", { usage_event: event });
    eventCount++;
  }

  emit("complete", {
    item: {
      snapshot_complete: rows.length < 10000,
      usage_event_count: eventCount,
      next_cursor: JSON.stringify({
        last_time_created: maxTime,
      }),
      decoder_profile: "opencode-sqlite-v1",
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
