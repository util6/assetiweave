#!/usr/bin/env node
const fs = require("fs");
const path = require("path");
const crypto = require("crypto");
const { normalizeRound } = require("./qwen-normalize.cjs");

const root = process.env.ASSETIWEAVE_HARVESTER_DIR || process.cwd();
const nowID = new Date().toISOString().replace(/[-:]/g, "").replace(/\.\d{3}Z$/, "Z");
const rawDir = path.join(root, "output", "raw", nowID);
const detailDir = path.join(rawDir, "details");
const normalizedDir = path.join(root, "output", "normalized");
const normalizedFile = path.join(normalizedDir, "sessions.json");

const existingSessions = new Map();
try {
  if (fs.existsSync(normalizedFile)) {
    const data = JSON.parse(fs.readFileSync(normalizedFile, "utf8"));
    if (Array.isArray(data.sessions)) {
      for (const session of data.sessions) {
        existingSessions.set(session.external_id, session);
      }
    }
  }
} catch {}

function mkdirp(dir) {
  fs.mkdirSync(dir, { recursive: true, mode: 0o700 });
}

function readJSON(file) {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

function writeJSON(file, value) {
  mkdirp(path.dirname(file));
  fs.writeFileSync(file, JSON.stringify(value, null, 2) + "\n", { mode: 0o600 });
}

function cookieValue(cookieHeader, name) {
  for (const part of String(cookieHeader || "").split(";")) {
    const index = part.indexOf("=");
    if (index <= 0) continue;
    if (part.slice(0, index).trim() === name) return part.slice(index + 1).trim();
  }
  return "";
}

function text(value) {
  return typeof value === "string" ? value.trim() : "";
}

function commonParams(extra, ut) {
  const params = new URLSearchParams({
    biz_id: "ai_qwen",
    fe_version: "1.0.0",
    chat_client: "h5",
    device: "pc",
    fr: "pc",
    pr: "qwen",
    ut,
    la: "zh-CN",
    tz: Intl.DateTimeFormat().resolvedOptions().timeZone || "Asia/Shanghai",
    wv: "2.11.6",
    sign_type: "2"
  });
  for (const [key, value] of Object.entries(extra)) params.set(key, String(value));
  return params;
}

async function requestJSON(url, headers) {
  const response = await fetch(url, { headers });
  const body = await response.text();
  let parsed = null;
  try {
    parsed = JSON.parse(body);
  } catch {}
  return { status_code: response.status, body, json: parsed };
}

(async () => {
  mkdirp(detailDir);
  mkdirp(normalizedDir);

  const authProbe = readJSON(path.join(root, "requests", "auth-probe.json"));
  const cookie = authProbe.headers && authProbe.headers.Cookie;
  if (!cookie) {
    throw new Error("Qwen cookie login state is missing. Run: assetiweave-cli conversation web auth-detect " + root + " --domain qianwen.com --credential cookie");
  }
  const ut = cookieValue(cookie, "cna");
  if (!ut) {
    throw new Error("Qwen cna cookie is missing; cannot build web request ut parameter.");
  }

  const headers = {
    "Accept": "application/json, text/plain, */*",
    "Cookie": cookie,
    "Origin": "https://www.qianwen.com",
    "Referer": "https://www.qianwen.com/",
    "User-Agent": (authProbe.headers && authProbe.headers["User-Agent"]) || "Mozilla/5.0"
  };

  const listItems = [];
  const seenSessions = new Set();
  for (let page = 1; page <= 100; page++) {
    const params = commonParams({
      page,
      page_size: 100,
      return_response_messages: "false"
    }, ut);
    const url = "https://chat2-api.qianwen.com/api/v1/session/list?" + params.toString();
    const snapshot = await requestJSON(url, headers);
    writeJSON(path.join(rawDir, `list-page-${page}.json`), {
      status_code: snapshot.status_code,
      body: snapshot.body
    });
    if (snapshot.status_code !== 200) throw new Error(`Qwen list page ${page} failed with status ${snapshot.status_code}`);
    const items = snapshot.json && snapshot.json.data && Array.isArray(snapshot.json.data.list) ? snapshot.json.data.list : [];
    for (const item of items) {
      const sessionID = text(item.session_id);
      if (!sessionID || seenSessions.has(sessionID)) continue;
      seenSessions.add(sessionID);
      listItems.push(item);
    }
    if (items.length < 100) break;
  }

  const sessions = [];
  for (let index = 0; index < listItems.length; index++) {
    const item = listItems[index];
    const sessionID = text(item.session_id);
    const updatedAt = text(item.update_time) || null;
    const existing = existingSessions.get(sessionID);
    if (existing && existing.updated_at === updatedAt) {
      sessions.push(existing);
      continue;
    }
    const rounds = [];
    const seenRounds = new Set();
    for (let page = 1; page <= 100; page++) {
      const params = commonParams({
        session_id: sessionID,
        page,
        page_size: 100,
        return_response_messages: "true",
        event_filter: "all"
      }, ut);
      const url = "https://chat2-api.qianwen.com/api/v1/session/msg/list?" + params.toString();
      const snapshot = await requestJSON(url, headers);
      writeJSON(path.join(detailDir, `${String(index + 1).padStart(4, "0")}-${sessionID}-page-${page}.json`), {
        status_code: snapshot.status_code,
        body: snapshot.body
      });
      if (snapshot.status_code !== 200) throw new Error(`Qwen detail ${sessionID} page ${page} failed with status ${snapshot.status_code}`);
      const items = snapshot.json && snapshot.json.data && Array.isArray(snapshot.json.data.list) ? snapshot.json.data.list : [];
      for (const round of items) {
        const rid = text(round.req_id) || crypto.createHash("sha256").update(JSON.stringify(round)).digest("hex");
        if (seenRounds.has(rid)) continue;
        seenRounds.add(rid);
        rounds.push(round);
      }
      if (items.length < 100) break;
    }
    const turns = [];
    for (const round of rounds.slice().reverse()) {
      const turn = normalizeRound(round, turns.length);
      if (turn) turns.push(turn);
    }
    if (!turns.length) continue;
    sessions.push({
      external_id: sessionID,
      title: text(item.title) || null,
      project_path: null,
      started_at: text(item.create_time) || null,
      updated_at: updatedAt,
      source_locator: "https://www.qianwen.com/",
      source_fingerprint: sessionID,
      turns
    });
  }

  writeJSON(normalizedFile, { sessions });
  const turnCount = sessions.reduce((sum, session) => sum + session.turns.length, 0);
  console.log(JSON.stringify({
    ok: true,
    site_id: "qwen-web",
    raw_run_dir: rawDir,
    normalized_file: normalizedFile,
    session_count: sessions.length,
    turn_count: turnCount
  }));
})().catch((error) => {
  console.error(error && error.message ? error.message : String(error));
  process.exit(1);
});
