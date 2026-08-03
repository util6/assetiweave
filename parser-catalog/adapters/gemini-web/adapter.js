#!/usr/bin/env node
const fs = require("fs");
const path = require("path");

const CONTENT_CARD_SCHEMA = "web-content-cards-v4";
const ADAPTER_ID = "gemini-web";

function emit(value) {
  process.stdout.write(JSON.stringify(value) + "\n");
}

let request = {};
try {
  const input = fs.readFileSync(0, "utf8").trim();
  request = input ? JSON.parse(input) : {};
} catch (error) {
  emit({ type: "error", message: "failed to read adapter request: " + error.message });
  process.exit(0);
}

if (request.method === "probe") {
  emit({ type: "complete", item: { ok: true } });
  process.exit(0);
}

const location = request.source && request.source.location ? request.source.location : ".";
const sessionsPath = resolveSessionsPath(location);
let payload;
try {
  payload = JSON.parse(fs.readFileSync(sessionsPath, "utf8"));
} catch (error) {
  emit({ type: "error", message: "failed to read normalized sessions: " + sessionsPath + ": " + error.message });
  process.exit(0);
}

const sessions = Array.isArray(payload.sessions)
  ? payload.sessions.map(normalizeSessionCards).filter(Boolean)
  : [];
for (const session of sessions) {
  session.source_fingerprint = sessionVersionToken(session);
}
if (request.method === "list_sessions") {
  for (const session of sessions) {
    emit({ type: "item", item: { kind: "session_descriptor", external_id: session.external_id, updated_at: session.updated_at || null, source_locator: session.source_locator || null, version_token: session.source_fingerprint } });
  }
  emit({ type: "complete", item: { session_count: sessions.length, snapshot_complete: true } });
  process.exit(0);
}
if (request.method !== "read_session") {
  emit({ type: "error", message: "unsupported adapter method: " + request.method });
  process.exit(0);
}
const requestedSessionID = request.params && request.params.session_id;
const selectedSessions = requestedSessionID
  ? sessions.filter((session) => String(session.external_id) === String(requestedSessionID))
  : sessions;
for (const session of selectedSessions) {
  emit({ type: "item", item: { kind: "session", session } });
}
emit({ type: "complete", item: { session_count: selectedSessions.length } });

function sessionVersionToken(session) {
  return [CONTENT_CARD_SCHEMA, session.external_id || "", session.updated_at || "", session.source_fingerprint || ""].join(":");
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

function normalizeSessionCards(session) {
  if (!session || typeof session !== "object") return session;
  let changed = false;
  const turns = Array.isArray(session.turns) ? session.turns : [];
  const visibleTurns = turns
    .filter((turn) => Array.isArray(turn && turn.parts) && turn.parts.length > 0)
    .map((turn, index) => ({ ...turn, turn_index: index }));
  changed = visibleTurns.length !== turns.length;
  if (visibleTurns.length === 0) return null;
  session.turns = visibleTurns;
  for (const turn of visibleTurns) {
    const parts = Array.isArray(turn && turn.parts) ? turn.parts : [];
    for (const part of parts) {
      if (ensurePartContentCard(part)) {
        changed = true;
      }
    }
  }
  if (changed && typeof session.source_fingerprint === "string" && session.source_fingerprint.trim()) {
    if (!session.source_fingerprint.includes(CONTENT_CARD_SCHEMA)) {
      session.source_fingerprint = session.source_fingerprint + ":" + CONTENT_CARD_SCHEMA;
    }
  }
  return session;
}

function ensurePartContentCard(part) {
  if (!part || typeof part !== "object") return false;
  const metadata = metadataObject(part.metadata_json);
  const legacyCard = metadata.content_card || metadata.contentCard;
  const contentCard = legacyCard && typeof legacyCard === "object" && typeof legacyCard.type === "string"
    ? legacyCard
    : inferContentCard(part);
  let changed = false;
  if (contentCard?.type === "result" && typeof part.text === "string") {
    const normalized = normalizeExecutionResultText(part.text) || null;
    if (part.text !== normalized) changed = true;
    part.text = normalized;
  }
  if (contentCard?.type === "command") {
    const field = typeof part.command === "string" ? "command" : typeof part.text === "string" ? "text" : null;
    if (field) {
      const normalized = normalizeExecutionCommandText(part[field]) || null;
      if (part[field] !== normalized) changed = true;
      part[field] = normalized;
    }
  }
  if (!part.content_card && contentCard) {
    part.content_card = {
      schema_version: 1,
      kind: `${ADAPTER_ID}.${contentCard.type}`,
      renderer: structuredCardRenderer(contentCard),
    };
    changed = true;
  }
  if ("content_card" in metadata || "contentCard" in metadata) changed = true;
  delete metadata.content_card;
  delete metadata.contentCard;
  const nextMetadata = Object.keys(metadata).length > 0 ? JSON.stringify(metadata) : null;
  if (part.metadata_json !== nextMetadata) changed = true;
  part.metadata_json = nextMetadata;
  return changed;
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

function metadataObject(value) {
  if (!value || typeof value !== "string" || !value.trim()) return {};
  try {
    const parsed = JSON.parse(value);
    return parsed && typeof parsed === "object" && !Array.isArray(parsed) ? parsed : {};
  } catch {
    return {};
  }
}

function inferContentCard(part) {
  const kind = text(part.kind || "text");
  const role = text(part.role || "assistant");
  const language = text(part.language);
  if (kind === "code_block") {
    return compactObject({ type: "code", language });
  }
  if (kind === "command") {
    return { type: "command" };
  }
  if (kind === "tool" || kind === "file_change" || kind === "subagent") {
    return { type: "result", format: "markdown" };
  }
  if (kind === "metadata") {
    return { type: "tool", format: "markdown" };
  }
  if (role === "tool") {
    return { type: "result", format: "markdown" };
  }
  if (role === "assistant") {
    return { type: "answer", format: "markdown" };
  }
  return null;
}

function compactObject(value) {
  return Object.fromEntries(
    Object.entries(value).filter(([, entry]) => entry !== null && entry !== undefined && entry !== "")
  );
}

function text(value) {
  return typeof value === "string" ? value.trim() : "";
}

function resolveSessionsPath(location) {
  const candidates = [
    path.join(location, "sessions.json"),
    path.join(location, "normalized", "sessions.json"),
    path.join(location, "output", "normalized", "sessions.json"),
  ];
  if (/[\\/]normalized$/i.test(location) && !/[\\/]output[\\/]normalized$/i.test(location)) {
    candidates.push(path.join(path.dirname(location), "output", "normalized", "sessions.json"));
  }
  if (/[\\/]output[\\/]normalized$/i.test(location)) {
    candidates.push(path.join(path.dirname(path.dirname(location)), "normalized", "sessions.json"));
  }
  for (const candidate of candidates) {
    if (fs.existsSync(candidate)) {
      return candidate;
    }
  }
  return candidates[0];
}
