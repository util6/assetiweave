#!/usr/bin/env node
const fs = require("fs");
const path = require("path");
const crypto = require("crypto");
const { parseDetailBody } = require("./gemini-normalize.cjs");
const { acquireCDPTarget, tryRefreshAuth } = require("./cdp-browser.cjs");

const root = process.env.ASSETIWEAVE_HARVESTER_DIR || process.cwd();
const runID = new Date().toISOString().replace(/[-:]/g, "").replace(/\.\\d{3}Z$/, "Z");
const rawDir = path.join(root, "output", "raw", runID);
const detailDir = path.join(rawDir, "details");
const normalizedDir = path.join(root, "output", "normalized");
const normalizedFile = path.join(normalizedDir, "sessions.json");

const existingSessions = new Map();
try {
  if (fs.existsSync(normalizedFile)) {
    const data = JSON.parse(fs.readFileSync(normalizedFile, "utf8"));
    if (Array.isArray(data.sessions)) {
      for (const s of data.sessions) {
        existingSessions.set(s.external_id, s);
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

// ---------------------------------------------------------------------------
// Direct cookie-based collection helpers
// ---------------------------------------------------------------------------

async function fetchAppContext(baseHeaders) {
  const response = await fetch("https://gemini.google.com/app", { headers: baseHeaders });
  const html = await response.text();
  const token = match1(html, /"SNlM0e"\s*:\s*"(.*?)"/);
  if (!token) {
    throw new Error(`Gemini web CSRF token SNlM0e was not found in the app HTML; app status ${response.status}`);
  }
  return {
    token,
    sid: match1(html, /"FdrFJe"\s*:\s*"(.*?)"/),
    bl: match1(html, /"cfb2h"\s*:\s*"(.*?)"/),
    endpointBase: match1(html, /"eptZe"\s*:\s*"(.*?)"/),
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
  const endpointBase = ctx.endpointBase || "/_/BardChatUi";
  const endpointPath = endpointBase.endsWith("/")
    ? `${endpointBase}data/batchexecute`
    : `${endpointBase}/data/batchexecute`;
  const url = new URL(endpointPath, "https://gemini.google.com").toString() + "?" + params.toString();
  const response = await fetch(url, { method: "POST", headers, body });
  const responseBody = await response.text();
  return {
    status_code: response.status,
    body: responseBody,
    frames: parseFrames(responseBody)
  };
}

// ---------------------------------------------------------------------------
// Direct cookie-based collection
// ---------------------------------------------------------------------------

async function collectDirect() {
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
    bl_found: !!ctx.bl,
    endpoint_found: !!ctx.endpointBase
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
    
    // Check local cache
    const existing = existingSessions.get(item.cid);
    if (existing && existing.updated_at === item.updatedAt) {
      sessions.push(existing);
      continue;
    }

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
  return { listItems, sessions, detailFailures, usedBrowserContext: false };
}

// ---------------------------------------------------------------------------
// Direct collection with auto auth-detect retry
// ---------------------------------------------------------------------------

async function collectDirectWithRetry() {
  try {
    return await collectDirect();
  } catch (firstError) {
    process.stderr.write(`[gemini-web] direct collection failed: ${firstError.message}; attempting auth-detect refresh...\n`);
    const refreshed = tryRefreshAuth(root, "google.com", {
      probeURL: "https://gemini.google.com/app"
    });
    if (!refreshed) throw firstError;
    return await collectDirect();
  }
}

// ---------------------------------------------------------------------------
// CDP browser collection
// ---------------------------------------------------------------------------

async function collectViaBrowserContext() {
  const { client, target, launched } = await acquireCDPTarget({
    urlPattern: /^https:\/\/gemini\.google\.com\/app(?:\/|$|\?)/,
    siteURL: "https://gemini.google.com/app",
    endpointEnv: "ASSETIWEAVE_GEMINI_CDP_ENDPOINT",
  });

  try {
    await client.send("Runtime.enable");
    const expression = String.raw`(async () => {
      const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
      for (let i = 0; i < 40; i++) {
        if (window.location.hostname.includes("gemini.google.com")) break;
        await sleep(250);
      }
      const text = (value) => typeof value === "string" ? value.trim() : "";
      const nested = (value, path, fallback = undefined) => {
        let current = value;
        for (const key of path) {
          if (Array.isArray(current) && Number.isInteger(key) && key >= 0 && key < current.length) current = current[key];
          else return fallback;
        }
        return current == null ? fallback : current;
      };
      const timestamp = (parts) => {
        if (!Array.isArray(parts) || typeof parts[0] !== "number") return null;
        const nanos = typeof parts[1] === "number" ? parts[1] : 0;
        const date = new Date(parts[0] * 1000 + Math.floor(nanos / 1e6));
        return Number.isNaN(date.getTime()) ? null : date.toISOString();
      };
      const parseFrames = (body) => {
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
      };
      const parseListSnapshot = (snapshot) => {
        for (const frame of snapshot.frames) {
          const bodyString = nested(frame, [2], null);
          if (typeof bodyString !== "string") continue;
          try {
            const body = JSON.parse(bodyString);
            const items = nested(body, [2], []);
            if (Array.isArray(items)) return { cursor: typeof body[1] === "string" ? body[1] : null, items };
          } catch {}
        }
        return { cursor: null, items: [] };
      };
      const parseDetailSnapshot = (snapshot) => {
        for (const frame of snapshot.frames) {
          const bodyString = nested(frame, [2], null);
          if (typeof bodyString !== "string") continue;
          try {
            return JSON.parse(bodyString);
          } catch {}
        }
        return null;
      };
      const rpcConfig = async () => {
        for (let attempt = 0; attempt < 60; attempt++) {
          const data = window.WIZ_global_data || {};
          const at = typeof data.SNlM0e === "string" ? data.SNlM0e.trim() : "";
          const buildLabel = typeof data.cfb2h === "string" ? data.cfb2h.trim() : "";
          const sessionId = typeof data.FdrFJe === "string" ? data.FdrFJe.trim() : "";
          const endpointBase = typeof data.eptZe === "string" && data.eptZe.trim() ? data.eptZe.trim() : "/_/BardChatUi";
          if (at && buildLabel && sessionId) return { at, buildLabel, sessionId, endpointBase };
          await sleep(250);
        }
        throw new Error("Gemini browser page did not expose WIZ_global_data.SNlM0e");
      };
      const config = await rpcConfig();
      let reqid = 100000;
      const batch = async (rpcid, payload) => {
        reqid += 100000;
        const endpointPath = config.endpointBase.endsWith("/")
          ? config.endpointBase + "data/batchexecute"
          : config.endpointBase + "/data/batchexecute";
        const url = new URL(endpointPath, location.origin);
        url.searchParams.set("rpcids", rpcid);
        url.searchParams.set("hl", document.documentElement.lang || navigator.language || "en");
        url.searchParams.set("_reqid", String(reqid));
        url.searchParams.set("rt", "c");
        url.searchParams.set("source-path", location.pathname || "/app");
        url.searchParams.set("bl", config.buildLabel);
        url.searchParams.set("f.sid", config.sessionId);
        const body = new URLSearchParams({
          at: config.at,
          "f.req": JSON.stringify([[[rpcid, JSON.stringify(payload), null, "generic"]]])
        });
        const response = await fetch(url.toString(), {
          method: "POST",
          credentials: "same-origin",
          headers: {
            "Content-Type": "application/x-www-form-urlencoded;charset=UTF-8",
            "X-Same-Domain": "1"
          },
          body
        });
        const responseBody = await response.text();
        return { status_code: response.status, body: responseBody, frames: parseFrames(responseBody) };
      };
      const listItems = [];
      const listSnapshots = [];
      const seenIDs = new Set();
      for (const flag of [1, 0]) {
        let cursor = null;
        for (let page = 1; page <= 20; page++) {
          const snapshot = await batch("MaZiqc", [100, cursor, [flag, null, 1]]);
          listSnapshots.push({ flag, page, status_code: snapshot.status_code, body: snapshot.body });
          if (snapshot.status_code !== 200) throw new Error("Gemini browser list flag " + flag + " page " + page + " failed with status " + snapshot.status_code);
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
      const cache = ${JSON.stringify(Object.fromEntries(Array.from(existingSessions.entries()).map(([k, v]) => [k, v.updated_at])))};
      const details = [];
      for (let index = 0; index < listItems.length; index++) {
        const item = listItems[index];
        if (cache[item.cid] === item.updatedAt) {
          continue; // Skip fetch as local cache is up to date
        }
        const snapshot = await batch("hNvQHb", [item.cid, 1000, null, 1, [1], [4], null, 1]);
        details.push({
          index: index + 1,
          cid: item.cid,
          status_code: snapshot.status_code,
          body: snapshot.body,
          detailBody: parseDetailSnapshot(snapshot)
        });
      }
      return {
        target_url: location.href,
        rpc_config: {
          token_found: true,
          sid_found: Boolean(config.sessionId),
          bl_found: Boolean(config.buildLabel),
          endpoint_found: Boolean(config.endpointBase)
        },
        listItems,
        listSnapshots,
        details
      };
    })()`;
    const evaluated = await client.send("Runtime.evaluate", {
      expression,
      awaitPromise: true,
      returnByValue: true,
      timeout: 120000
    });
    if (evaluated.exceptionDetails) {
      throw new Error(evaluated.exceptionDetails.text || "Gemini browser collection failed");
    }
    const value = evaluated.result && evaluated.result.value;
    if (!value || !Array.isArray(value.listItems) || !Array.isArray(value.details)) {
      throw new Error("Gemini browser collection returned an invalid payload");
    }
    writeJSON(path.join(rawDir, "context.json"), {
      browser_context: true,
      browser_target_url: value.target_url,
      browser_launched: launched,
      token_found: Boolean(value.rpc_config && value.rpc_config.token_found),
      sid_found: Boolean(value.rpc_config && value.rpc_config.sid_found),
      bl_found: Boolean(value.rpc_config && value.rpc_config.bl_found),
      endpoint_found: Boolean(value.rpc_config && value.rpc_config.endpoint_found)
    });
    for (const snapshot of value.listSnapshots || []) {
      writeJSON(path.join(rawDir, `list-flag-${snapshot.flag}-page-${snapshot.page}.json`), {
        status_code: snapshot.status_code,
        body: snapshot.body
      });
    }
    const sessions = [];
    let detailFailures = 0;
    
    for (const item of value.listItems) {
      const detail = value.details.find((d) => d.cid === item.cid);
      if (detail) {
        writeJSON(path.join(detailDir, `${String(detail.index).padStart(4, "0")}-${safeName(detail.cid)}.json`), {
          status_code: detail.status_code,
          body: detail.body
        });
        if (detail.status_code !== 200 || !detail.detailBody) {
          detailFailures++;
          continue;
        }
        const turns = parseDetailBody(detail.cid, detail.detailBody);
        if (!turns.length) continue;
        sessions.push({
          external_id: detail.cid,
          title: item.title || null,
          project_path: null,
          started_at: null,
          updated_at: item.updatedAt || null,
          source_locator: `https://gemini.google.com/app/${encodeURIComponent(detail.cid)}`,
          source_fingerprint: crypto.createHash("sha256").update(JSON.stringify({
            cid: detail.cid,
            updatedAt: item.updatedAt || null,
            turns
          })).digest("hex"),
          turns
        });
      } else {
        const existing = existingSessions.get(item.cid);
        if (existing) {
          sessions.push(existing);
        }
      }
    }
    return { listItems: value.listItems, sessions, detailFailures, usedBrowserContext: true };
  } finally {
    client.close();
  }
}

// ---------------------------------------------------------------------------
// Main — CDP-first for consistency, direct cookie as fast fallback
// ---------------------------------------------------------------------------

(async () => {
  mkdirp(detailDir);
  mkdirp(normalizedDir);

  let collection;
  let directFailed = false;

  // Strategy: try direct cookie collection first (still works reliably for Gemini
  // when cookies are fresh), then CDP browser as fallback with auto-discovery.
  // This differs from chatgpt-web which must use CDP-first due to Cloudflare.
  try {
    collection = await collectDirectWithRetry();
  } catch (directError) {
    directFailed = true;
    process.stderr.write(`[gemini-web] direct collection failed: ${directError.message}; falling back to CDP browser...\n`);
    try {
      collection = await collectViaBrowserContext();
    } catch (browserError) {
      const directMessage = directError && directError.message ? directError.message : String(directError);
      const browserMessage = browserError && browserError.message ? browserError.message : String(browserError);
      throw new Error(
        `Gemini direct collection failed: ${directMessage}; ` +
        `CDP browser collection also failed: ${browserMessage}. ` +
        `Start Edge/Chrome with --remote-debugging-port=9222 and keep https://gemini.google.com/app open, ` +
        `or run: assetiweave-cli conversation web auth-detect ${root} --domain google.com --credential cookie --probe-url https://gemini.google.com/app`
      );
    }
  }

  writeJSON(normalizedFile, { sessions: collection.sessions });
  const turnCount = collection.sessions.reduce((sum, session) => sum + session.turns.length, 0);
  console.log(JSON.stringify({
    ok: true,
    site_id: "gemini-web",
    raw_run_dir: rawDir,
    normalized_file: normalizedFile,
    listed_sessions: collection.listItems.length,
    session_count: collection.sessions.length,
    turn_count: turnCount,
    detail_failures: collection.detailFailures,
    used_browser_context: collection.usedBrowserContext,
    direct_collection_failed: directFailed
  }));
})().catch((error) => {
  console.error(error && error.message ? error.message : String(error));
  process.exit(1);
});
