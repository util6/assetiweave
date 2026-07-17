#!/usr/bin/env node
const fs = require("fs");
const path = require("path");

const CONTENT_CARD_SCHEMA = "web-content-cards-v2";

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
  const existing = metadata.content_card || metadata.contentCard;
  if (existing && typeof existing === "object" && typeof existing.type === "string") {
    return false;
  }
  const contentCard = inferContentCard(part);
  if (!contentCard) return false;
  part.metadata_json = JSON.stringify({ ...metadata, content_card: contentCard });
  return true;
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
  const direct = path.join(location, "sessions.json");
  if (fs.existsSync(direct)) {
    return direct;
  }
  const normalized = path.join(location, "output", "normalized", "sessions.json");
  if (fs.existsSync(normalized)) {
    return normalized;
  }
  return /[\\/]output[\\/]normalized$/i.test(location) ? direct : normalized;
}
