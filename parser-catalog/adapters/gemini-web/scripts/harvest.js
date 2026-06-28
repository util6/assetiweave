#!/usr/bin/env node
const fs = require("fs");
const path = require("path");
const crypto = require("crypto");
const { parseDetailBody } = require("./gemini-normalize.cjs");

const root = process.env.ASSETIWEAVE_HARVESTER_DIR || process.cwd();
const runID = new Date().toISOString().replace(/[-:]/g, "").replace(/\.\d{3}Z$/, "Z");
const rawDir = path.join(root, "output", "raw", runID);
const detailDir = path.join(rawDir, "details");
const normalizedDir = path.join(root, "output", "normalized");
const normalizedFile = path.join(normalizedDir, "sessions.json");

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

function match1(text, pattern) {
  const match = text.match(pattern);
  return match ? match[1] : null;
}

function nested(value, path, fallback = undefined) {
  let current = value;
  for (const key of path) {
    if (Array.isArray(current) && Number.isInteger(key) && key >= 0 && key < current.length) {
      current = current[key];
    } else {
      return fallback;
    }
  }
  return current == null ? fallback : current;
}

function text(value) {
  return typeof value === "string" ? value.trim() : "";
}

function safeName(value) {
  return String(value).replace(/[^A-Za-z0-9._-]+/g, "_").slice(0, 160) || "item";
}

function timestamp(parts) {
  if (!Array.isArray(parts) || typeof parts[0] !== "number") return null;
  const nanos = typeof parts[1] === "number" ? parts[1] : 0;
  const date = new Date(parts[0] * 1000 + Math.floor(nanos / 1e6));
  return Number.isNaN(date.getTime()) ? null : date.toISOString();
}

function parseFrames(body) {
  let content = body.startsWith(")]}'") ? body.slice(4) : body;
  const frames = [];
  for (const line of content.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed || /^\d+$/.test(trimmed) || !trimmed.startsWith("[")) continue;
    try {
      const parsed = JSON.parse(trimmed);
      if (Array.isArray(parsed)) frames.push(...parsed);
      else frames.push(parsed);
    } catch {}
  }
  return frames;
}

function parseListSnapshot(snapshot) {
  for (const frame of snapshot.frames) {
    const bodyString = nested(frame, [2], null);
    if (typeof bodyString !== "string") continue;
    try {
      const body = JSON.parse(bodyString);
      const items = nested(body, [2], []);
      if (Array.isArray(items)) {
        return {
          cursor: typeof body[1] === "string" ? body[1] : null,
          items
        };
      }
    } catch {}
  }
  return { cursor: null, items: [] };
}

function parseDetailSnapshot(cid, snapshot) {
  for (const frame of snapshot.frames) {
    const bodyString = nested(frame, [2], null);
    if (typeof bodyString !== "string") continue;
    try {
      const body = JSON.parse(bodyString);
      return parseDetailBody(cid, body);
    } catch {}
  }
  return [];
}

async function fetchAppContext(baseHeaders) {
  const response = await fetch("https://gemini.google.com/app", { headers: baseHeaders });
  const html = await response.text();
  const token = match1(html, /"SNlM0e"\s*:\s*"(.*?)"/);
  if (!token) throw new Error(`Gemini SNlM0e access token not found; app status ${response.status}`);
  return {
    token,
    sid: match1(html, /"FdrFJe"\s*:\s*"(.*?)"/),
    bl: match1(html, /"cfb2h"\s*:\s*"(.*?)"/),
    appStatus: response.status,
    htmlBytes: html.length
  };
}

function requestHeaders(authProbe) {
  const headers = authProbe.headers || {};
  const cookie = headers.Cookie || headers.cookie;
  if (!cookie) {
    throw new Error("Gemini cookie login state is missing. Run: assetiweave-cli conversation web auth-detect " + root + " --domain google.com --credential cookie --probe-url https://gemini.google.com/app");
  }
  return {
    "User-Agent": headers["User-Agent"] || headers["user-agent"] || "Mozilla/5.0",
    "Cookie": cookie,
    "Origin": "https://gemini.google.com",
    "Referer": "https://gemini.google.com/",
    "Content-Type": "application/x-www-form-urlencoded;charset=utf-8",
    "X-Same-Domain": "1",
    "x-goog-ext-525001261-jspb": "[1,null,null,null,null,null,null,null,[4]]",
    "x-goog-ext-73010989-jspb": "[0]"
  };
}

let reqid = 100000;
async function batch(ctx, headers, rpcid, payload) {
  reqid += 100000;
  const params = new URLSearchParams({
    rpcids: rpcid,
    hl: "en",
    _reqid: String(reqid),
    rt: "c",
    "source-path": "/app"
  });
  if (ctx.bl) params.set("bl", ctx.bl);
  if (ctx.sid) params.set("f.sid", ctx.sid);
  const body = new URLSearchParams({
    at: ctx.token,
    "f.req": JSON.stringify([[[rpcid, JSON.stringify(payload), null, "generic"]]])
  });
  const url = "https://gemini.google.com/_/BardChatUi/data/batchexecute?" + params.toString();
  const response = await fetch(url, { method: "POST", headers, body });
  const responseBody = await response.text();
  return {
    status_code: response.status,
    body: responseBody,
    frames: parseFrames(responseBody)
  };
}

(async () => {
  mkdirp(detailDir);
  mkdirp(normalizedDir);

  const authProbe = readJSON(path.join(root, "requests", "auth-probe.json"));
  const headers = requestHeaders(authProbe);
  const ctx = await fetchAppContext({
    "User-Agent": headers["User-Agent"],
    "Cookie": headers.Cookie,
    "Accept": "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"
  });
  writeJSON(path.join(rawDir, "context.json"), {
    app_status: ctx.appStatus,
    html_bytes: ctx.htmlBytes,
    token_found: true,
    sid_found: !!ctx.sid,
    bl_found: !!ctx.bl
  });

  const listItems = [];
  const seenIDs = new Set();
  for (const flag of [1, 0]) {
    let cursor = null;
    for (let page = 1; page <= 20; page++) {
      const snapshot = await batch(ctx, headers, "MaZiqc", [100, cursor, [flag, null, 1]]);
      writeJSON(path.join(rawDir, `list-flag-${flag}-page-${page}.json`), {
        status_code: snapshot.status_code,
        body: snapshot.body
      });
      if (snapshot.status_code !== 200) throw new Error(`Gemini list flag ${flag} page ${page} failed with status ${snapshot.status_code}`);
      const parsed = parseListSnapshot(snapshot);
      for (const item of parsed.items) {
        const cid = text(nested(item, [0], ""));
        if (!cid || seenIDs.has(cid)) continue;
        seenIDs.add(cid);
        listItems.push({
          cid,
          title: text(nested(item, [1], "")) || null,
          updatedAt: timestamp(nested(item, [5], null))
        });
      }
      if (!parsed.cursor || parsed.items.length === 0) break;
      cursor = parsed.cursor;
    }
  }

  const sessions = [];
  let detailFailures = 0;
  for (let index = 0; index < listItems.length; index++) {
    const item = listItems[index];
    const snapshot = await batch(ctx, headers, "hNvQHb", [item.cid, 1000, null, 1, [1], [4], null, 1]);
    writeJSON(path.join(detailDir, `${String(index + 1).padStart(4, "0")}-${safeName(item.cid)}.json`), {
      status_code: snapshot.status_code,
      body: snapshot.body
    });
    if (snapshot.status_code !== 200) {
      detailFailures++;
      continue;
    }
    const turns = parseDetailSnapshot(item.cid, snapshot);
    if (!turns.length) continue;
    sessions.push({
      external_id: item.cid,
      title: item.title,
      project_path: null,
      started_at: null,
      updated_at: item.updatedAt,
      source_locator: `https://gemini.google.com/app/${encodeURIComponent(item.cid)}`,
      source_fingerprint: crypto.createHash("sha256").update(JSON.stringify({
        cid: item.cid,
        updatedAt: item.updatedAt,
        turns
      })).digest("hex"),
      turns
    });
  }

  writeJSON(normalizedFile, { sessions });
  const turnCount = sessions.reduce((sum, session) => sum + session.turns.length, 0);
  console.log(JSON.stringify({
    ok: true,
    site_id: "gemini-web",
    raw_run_dir: rawDir,
    normalized_file: normalizedFile,
    listed_sessions: listItems.length,
    session_count: sessions.length,
    turn_count: turnCount,
    detail_failures: detailFailures
  }));
})().catch((error) => {
  console.error(error && error.message ? error.message : String(error));
  process.exit(1);
});
