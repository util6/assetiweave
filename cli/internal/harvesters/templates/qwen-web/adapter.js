#!/usr/bin/env node
const fs = require("fs");
const path = require("path");

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

const sessions = Array.isArray(payload.sessions) ? payload.sessions : [];
for (const session of sessions) {
  emit({ type: "item", item: { kind: "session", session } });
}
emit({ type: "complete", item: { session_count: sessions.length } });

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
