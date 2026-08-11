#!/usr/bin/env node
import { createHash } from "node:crypto";
import { existsSync, readFileSync, statSync } from "node:fs";
import { homedir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const input = JSON.parse(readFileSync(0, "utf8") || "{}");
const CONTENT_CARD_SCHEMA_VERSION = "codex-content-cards-v12";
const MAX_PART_TEXT_CHARS = 96 * 1024;
const MAX_SESSION_TEXT_CHARS = 384 * 1024;
const MAX_COMPACTED_TOOL_TEXT_CHARS = 24 * 1024;
const MIN_STANDARD_SESSION_TEXT_CHARS = 96 * 1024;
const BROWSE_OUTPUT_EDGE_LINES = 24;
const BROWSE_OUTPUT_CONTEXT_LINES = 2;
const MAX_BROWSE_OUTPUT_LINE_CHARS = 1200;
const SIGNAL_LINE_PATTERN =
  /\b(error|failed|failure|panic|exception|traceback|warning|warn|denied|not found|cannot|could not|timeout|timed out|exit code|failures?|caused by|compilation|syntaxerror|typeerror|referenceerror|assertionerror)\b|error\[[A-Za-z0-9_-]+\]|\b[A-Za-z0-9_./-]+:\d+:\d+\b/i;

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

function fileVersionToken(filePath, updatedAt = null) {
  const stat = statSync(filePath);
  return sha256(`${CONTENT_CARD_SCHEMA_VERSION}\0${updatedAt ?? ""}\0${stat.size}\0${stat.mtimeMs}`);
}

function readStableFile(filePath, updatedAt = null) {
  for (let attempt = 0; attempt < 2; attempt++) {
    const before = fileVersionToken(filePath, updatedAt);
    const text = readFileSync(filePath, "utf8");
    const after = fileVersionToken(filePath, updatedAt);
    if (before === after) return { text, versionToken: after };
  }
  throw new Error(`session changed while being read: ${filePath}`);
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

function valueAsDisplayString(value) {
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  if (value && typeof value === "object") return JSON.stringify(value);
  return null;
}

function javascriptStringField(value, names) {
  const source = String(value ?? "");
  for (const name of names) {
    const escapedName = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    const propertyName = `(?:${escapedName}|\"${escapedName}\")`;
    const match = source.match(
      new RegExp(`(?:^|[,{\\s])${propertyName}\\s*:\\s*(\"(?:\\\\.|[^\"\\\\])*\")`),
    );
    if (!match) continue;
    try {
      const parsed = JSON.parse(match[1]);
      if (typeof parsed === "string" && parsed.trim()) return parsed;
    } catch {
      // Treat custom tool input as data; malformed string literals are ignored.
    }
  }
  return null;
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
    call_id: value.call_id ?? value.callID,
  });
}

function sourceExecutionId(value) {
  if (!value || typeof value !== "object") return null;
  const candidate = value.call_id ?? value.callID ?? value.tool_use_id ?? value.toolUseID;
  return typeof candidate === "string" && candidate.trim() ? candidate.trim() : null;
}

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

function displayTurns(turns) {
  return turns
    .filter((turn) => Array.isArray(turn.parts) && turn.parts.length > 0)
    .map((turn, index) => ({
      ...turn,
      turn_index: index,
    }));
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

function parseUserMessage(content) {
  const original = String(content ?? "");
  const skills = [];
  const withoutSkillBlocks = original.replace(/<skill>([\s\S]*?)<\/skill>/gi, (_match, body) => {
    const name = xmlTagValue(body, "name");
    const skillPath = xmlTagValue(body, "path");
    if (skillPath) skills.push({ name, path: skillPath });
    return "";
  });
  const withoutInlineReferences = withoutSkillBlocks.replace(
    /\[\$([^\]\r\n]+)\]\(([^)\r\n]*SKILL\.md)\)/gi,
    (_match, name, skillPath) => {
      skills.push({ name: String(name).trim(), path: normalizeSkillPath(skillPath) });
      return "";
    },
  );
  return {
    skills: uniqueSkills(skills),
    text: withoutInlineReferences.replace(/[ \t]+\n/g, "\n").trim(),
    skillOnly: skills.length > 0 && !withoutSkillBlocks.trim(),
  };
}

function xmlTagValue(value, tag) {
  const match = String(value ?? "").match(new RegExp(`<${tag}>([\\s\\S]*?)<\\/${tag}>`, "i"));
  return match ? decodeXmlText(match[1]).trim() || null : null;
}

function decodeXmlText(value) {
  return String(value ?? "")
    .replaceAll("&lt;", "<")
    .replaceAll("&gt;", ">")
    .replaceAll("&quot;", '"')
    .replaceAll("&#39;", "'")
    .replaceAll("&amp;", "&");
}

function normalizeSkillPath(value) {
  const trimmed = String(value ?? "").trim();
  if (trimmed.startsWith("<") && trimmed.endsWith(">")) return trimmed.slice(1, -1).trim();
  return trimmed;
}

function uniqueSkills(skills) {
  const seen = new Set();
  return skills.filter((skill) => {
    if (!skill.path || seen.has(skill.path)) return false;
    seen.add(skill.path);
    return true;
  });
}

function appendSkillParts(turn, skills) {
  const existingPaths = new Set(
    turn.parts
      .filter((part) => contentCardType(part) === "skill")
      .map((part) => String(part.text ?? "")),
  );
  for (const skill of skills) {
    if (!skill.path || existingPaths.has(skill.path)) continue;
    existingPaths.add(skill.path);
    turn.parts.push({
      role: "system",
      kind: "metadata",
      text: skill.path,
      language: null,
      command: null,
      cwd: null,
      status: null,
      exit_code: null,
      metadata_json: metadata(
        { type: "skill", format: "path" },
        compactObject({ skill_name: skill.name, skill_path: skill.path }),
      ),
    });
  }
}

function appendDetectedSkill(turn, skillDocument, skillPath) {
  if (!skillPath) return;
  turn.parts.push({
    role: "system",
    kind: "metadata",
    text: skillPath,
    language: null,
    command: null,
    cwd: null,
    status: null,
    exit_code: null,
    metadata_json: metadata(
      { type: "skill", format: "path" },
      compactObject({ skill_name: skillDocument.name, skill_path: skillPath, detected_from_tool: true }),
    ),
  });
}

function skillPathFromCommand(command) {
  const match = String(command ?? "").match(/(?:^|\s)(["']?)([^\s"']*SKILL\.md)\1(?=\s|$)/i);
  return match ? normalizeSkillPath(match[2]) : null;
}

function skillDocumentFromOutput(value) {
  for (const fragment of toolOutputTextFragments(value)) {
    const skillBlock = fragment.match(/<skill>\s*([\s\S]*?)<\/skill>/i);
    const skillPath = skillBlock ? xmlTagValue(skillBlock[1], "path") : null;
    const body = skillBlock
      ? skillBlock[1].replace(/^[\s\S]*?<\/path>\s*/i, "")
      : fragment;
    const document = skillDocumentFromText(body);
    if (document) return { ...document, path: skillPath };
  }
  return null;
}

function toolOutputTextFragments(value, fragments = [], depth = 0) {
  if (depth > 5 || value == null) return fragments;
  if (typeof value === "string") {
    fragments.push(value);
    const parsed = parseJsonValue(value);
    if (parsed) toolOutputTextFragments(parsed, fragments, depth + 1);
    return fragments;
  }
  if (Array.isArray(value)) {
    value.forEach((entry) => toolOutputTextFragments(entry, fragments, depth + 1));
    return fragments;
  }
  if (typeof value === "object") {
    for (const key of ["text", "content", "output", "result"]) {
      if (value[key] != null) toolOutputTextFragments(value[key], fragments, depth + 1);
    }
  }
  return fragments;
}

function skillDocumentFromText(value) {
  const text = String(value ?? "");
  const start = text.search(/(?:^|\r?\n)---\r?\n/);
  if (start < 0) return null;
  const document = text.slice(start + (text[start] === "\n" ? 1 : 0)).trim();
  const frontMatter = document.match(/^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)/);
  if (!frontMatter) return null;
  const name = frontMatter[1].match(/^name:\s*(.+?)\s*$/m)?.[1]?.trim() || null;
  const description = frontMatter[1].match(/^description:\s*(.+?)\s*$/m)?.[1]?.trim() || null;
  if (!name || !description) return null;
  return { name, text: document };
}

function splitMarkdownParts(role, text) {
  const parts = [];
  let remaining = String(text ?? "");
  while (remaining.includes("```")) {
    const start = remaining.indexOf("```");
    const before = remaining.slice(0, start);
    const beforePart = textPart(role, before);
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

function directStringField(value, names) {
  if (!value || typeof value !== "object") return null;
  for (const name of names) {
    const text = valueAsString(value[name]);
    if (text?.trim()) return text;
  }
  return null;
}

function nestedStringField(value, names, depth = 0) {
  if (depth > 6 || !value) return null;
  if (typeof value === "string") {
    const parsed = parseJsonValue(value);
    if (parsed) {
      const nested = nestedStringField(parsed, names, depth + 1);
      if (nested) return nested;
    }
    return javascriptStringField(value, names);
  }
  if (typeof value !== "object") return null;
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
  return nestedStringField(payload, ["command", "cmd", "shell_command"]);
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

function outputTextFromPayload(payload) {
  return (
    contentText(payload?.content) ||
    valueAsDisplayString(payload?.output) ||
    valueAsDisplayString(payload?.result) ||
    ""
  );
}

function toolNameFromPayload(payload) {
  return directStringField(payload, ["name", "tool_name", "toolName", "tool"]);
}

function isToolResultPayload(payload) {
  const type = String(payload?.type ?? "").toLowerCase();
  return (
    type.includes("output") ||
    type.includes("result") ||
    payload?.output != null ||
    payload?.result != null
  );
}

function toolDisplayText(payload, content) {
  const text = String(content ?? "").trim();
  if (text) return text;
  const toolName = toolNameFromPayload(payload);
  const type = String(payload?.type ?? "tool").trim() || "tool";
  if (toolName?.trim()) return `${type}: ${toolName.trim()}`;
  return "";
}

function isLowSignalControlTool(payload) {
  const type = String(payload?.type ?? "").toLowerCase();
  const name = String(toolNameFromPayload(payload) ?? "").trim().toLowerCase();
  return type === "function_call" && name === "wait";
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
  let pendingSkillRead = null;
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
      const userText = contentText(payload.content);
      const userMessage = parseUserMessage(userText);
      if (userMessage.skillOnly) {
        if (current) appendSkillParts(current, userMessage.skills);
        continue;
      }
      if (current) turns.push(current);
      if (!userMessage.text) {
        current = null;
        continue;
      }
      current = {
        external_id: payload.id ?? `turn-${turns.length + 1}`,
        turn_index: turns.length,
        user_text: userMessage.text,
        title: null,
        started_at: parsed.timestamp ?? payload.timestamp ?? null,
        ended_at: null,
        parts: [],
      };
      appendSkillParts(current, userMessage.skills);
    } else if (current && type === "message" && role === "assistant") {
      const text = contentText(payload.content);
      if (text.trim()) {
        current.parts.push(...splitMarkdownParts("assistant", text));
      }
      current.ended_at = parsed.timestamp ?? payload.timestamp ?? current.ended_at;
    } else if (current && isToolEvent(payload)) {
      const command = commandFromPayload(payload);
      const cwd = cwdFromPayload(payload);
      const text = outputTextFromPayload(payload);
      const status = statusFromPayload(payload);
      const exitCode = exitCodeFromPayload(payload);
      const executionId = sourceExecutionId(payload);
      const readSkillPath = command ? skillPathFromCommand(command) : pendingSkillRead?.path ?? null;
      const skillDocument = skillDocumentFromOutput(text);
      const skillPath = readSkillPath ?? skillDocument?.path ?? null;
      const skillRead = Boolean(skillDocument && skillPath);
      if (command?.trim()) {
        current.parts.push({
          role: "tool",
          kind: "command",
          text: null,
          language: null,
          command,
          cwd,
          status: null,
          exit_code: null,
          source_execution_id: executionId,
          metadata_json: metadata(compactObject({ type: "command", cwd }), smallMetadata(payload)),
        });
      }
      if (command?.trim() && skillRead) {
        appendDetectedSkill(current, skillDocument, skillPath);
        pendingSkillRead = null;
      } else if (command?.trim() && text.trim()) {
        current.parts.push({
          role: "tool",
          kind: "tool",
          text: text.trim(),
          language: null,
          command: null,
          cwd: null,
          status,
          exit_code: exitCode,
          source_execution_id: executionId,
          metadata_json: metadata(
            compactObject({ type: "result", format: "plain", status, exit_code: exitCode }),
            smallMetadata(payload),
          ),
        });
      }
      if (!command?.trim()) {
        const displayText = toolDisplayText(payload, text);
        if (skillRead) {
          appendDetectedSkill(current, skillDocument, skillPath);
          pendingSkillRead = null;
        } else if (isLowSignalControlTool(payload)) {
          // Keep an invisible positional Part so later Part IDs remain stable across re-syncs.
          current.parts.push({
            role: "tool",
            kind: "tool",
            text: null,
            language: null,
            command: null,
            cwd: null,
            status: null,
            exit_code: null,
            source_execution_id: null,
            metadata_json: null,
          });
        } else if (displayText) {
          const result = isToolResultPayload(payload);
          current.parts.push({
            role: "tool",
            kind: "tool",
            text: displayText,
            language: null,
            command: null,
            cwd: null,
            status: result ? status : null,
            exit_code: result ? exitCode : null,
            source_execution_id: result ? executionId : null,
            metadata_json: metadata(
              compactObject({
                type: result ? "result" : "tool",
                format: result ? "plain" : undefined,
                status: result ? status : null,
                exit_code: result ? exitCode : null,
              }),
              smallMetadata(payload),
            ),
          });
        }
      }
      if (command?.trim() && !skillRead) {
        pendingSkillRead = readSkillPath ? { path: readSkillPath } : null;
      } else if (!command?.trim() && !skillRead && isToolResultPayload(payload)) {
        pendingSkillRead = null;
      }
      current.ended_at = parsed.timestamp ?? payload.timestamp ?? current.ended_at;
    }
  }
  if (current) turns.push(current);
  return { turns, projectPath };
}

function sessionRows() {
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
  const sql = `SELECT ${quoteIdent(idCol)} AS id, ${quoteIdent(rolloutCol)} AS rollout_path, ${titleCol ? quoteIdent(titleCol) : "NULL"} AS title, ${updatedCol ? quoteIdent(updatedCol) : "NULL"} AS updated_at FROM threads ORDER BY rowid DESC`;
  return sqliteJson(dbPath, sql).map((row) => ({ ...row, rollout_path: expandPath(row.rollout_path) }));
}

function listSessions() {
  return sessionRows().flatMap((row) => {
    if (!row.rollout_path || !existsSync(row.rollout_path)) return [];
    return [{
      external_id: String(row.id),
      updated_at: row.updated_at == null ? null : String(row.updated_at),
      source_locator: row.rollout_path,
      version_token: fileVersionToken(row.rollout_path, row.updated_at),
    }];
  });
}

function readSession() {
  const requestedSessionId = input.params?.session_id ?? null;
  return sessionRows().filter((row) => !requestedSessionId || String(row.id) === String(requestedSessionId)).flatMap((row) => {
    const rolloutPath = expandPath(row.rollout_path);
    if (!rolloutPath || !existsSync(rolloutPath)) return [];
    const { text, versionToken } = readStableFile(rolloutPath, row.updated_at);
    const parsed = normalizeTurns(text);
    const turns = displayTurns(parsed.turns);
    if (!turns.length) return [];
    return [applyTextBudgets(finalizeStructuredContentCards({
      external_id: String(row.id),
      title: row.title == null ? null : String(row.title),
      project_path: parsed.projectPath ?? inferProjectPath(turns),
      started_at: turns[0]?.started_at ?? null,
      updated_at: row.updated_at == null ? null : String(row.updated_at),
      source_locator: rolloutPath,
      source_fingerprint: versionToken,
      turns,
    }))];
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
          kind: `codex.${legacyCard.type}`,
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
  if (card.type === "skill") return "path";
  if (card.type === "skill-content") return "markdown";
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
