#!/usr/bin/env node
const fs = require("fs");
const path = require("path");
const crypto = require("crypto");
const { parseConversation, timestamp } = require("./chatgpt-normalize.cjs");

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

function text(value) {
  return typeof value === "string" ? value.trim() : "";
}

function safeName(value) {
  return String(value).replace(/[^A-Za-z0-9._-]+/g, "_").slice(0, 160) || "item";
}

async function requestJSON(url, headers) {
  const response = await fetch(url, { headers });
  const body = await response.text();
  let parsed = null;
  try {
    parsed = JSON.parse(body);
  } catch {}
  return {
    status_code: response.status,
    body,
    json: parsed
  };
}

function readAuthHeaders(authProbe) {
  const headers = authProbe.headers || {};
  return {
    userAgent: headers["User-Agent"] || headers["user-agent"] || "Mozilla/5.0",
    cookie: headers.Cookie || headers.cookie || "",
    authorization: headers.Authorization || headers.authorization || ""
  };
}

async function resolveAccessToken(authProbe, authHeaders) {
  if (/^Bearer\s+\S+/i.test(authHeaders.authorization)) {
    return {
      token: authHeaders.authorization.replace(/^Bearer\s+/i, ""),
      sessionStatus: 0,
      sessionBytes: 0
    };
  }
  if (!authHeaders.cookie) {
    throw new Error("ChatGPT cookie login state is missing. Run: assetiweave-cli conversation web auth-detect " + root + " --domain chatgpt.com --credential cookie");
  }
  const sessionURL = authProbe.url || "https://chatgpt.com/api/auth/session";
  const snapshot = await requestJSON(sessionURL, {
    "Accept": "application/json",
    "Cookie": authHeaders.cookie,
    "Referer": "https://chatgpt.com/",
    "User-Agent": authHeaders.userAgent
  });
  writeJSON(path.join(rawDir, "session.json"), {
    status_code: snapshot.status_code,
    body: snapshot.body
  });
  if (snapshot.status_code !== 200) {
    throw new Error(`ChatGPT session probe failed with status ${snapshot.status_code}`);
  }
  const token = snapshot.json && typeof snapshot.json.accessToken === "string"
    ? snapshot.json.accessToken.trim()
    : "";
  if (!token) {
    throw new Error("ChatGPT session probe did not return accessToken; refresh auth with `assetiweave-cli conversation web auth-detect " + root + " --domain chatgpt.com --credential cookie`.");
  }
  return {
    token,
    sessionStatus: snapshot.status_code,
    sessionBytes: snapshot.body.length
  };
}

function apiHeaders(authHeaders, accessToken) {
  const headers = {
    "Accept": "application/json",
    "Authorization": "Bearer " + accessToken,
    "Referer": "https://chatgpt.com/",
    "User-Agent": authHeaders.userAgent,
    "oai-language": "en-US"
  };
  if (authHeaders.cookie) headers.Cookie = authHeaders.cookie;
  return headers;
}

function listItemsFromSnapshot(snapshot) {
  const body = snapshot.json || {};
  if (Array.isArray(body.items)) return body.items;
  if (body.data && Array.isArray(body.data.items)) return body.data.items;
  if (Array.isArray(body.conversations)) return body.conversations;
  return [];
}

function totalFromSnapshot(snapshot) {
  const body = snapshot.json || {};
  if (typeof body.total === "number") return body.total;
  if (body.data && typeof body.data.total === "number") return body.data.total;
  return null;
}

async function readRemoteDebugJSON(url) {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`DevTools endpoint ${url} returned ${response.status}`);
  return response.json();
}

function createCDPClient(webSocketDebuggerURL) {
  let nextID = 1;
  const pending = new Map();
  const ws = new WebSocket(webSocketDebuggerURL);
  const opened = new Promise((resolve, reject) => {
    ws.onopen = resolve;
    ws.onerror = () => reject(new Error("failed to connect to DevTools websocket"));
  });
  ws.onmessage = (event) => {
    const message = JSON.parse(event.data);
    if (!message.id || !pending.has(message.id)) return;
    const { resolve, reject } = pending.get(message.id);
    pending.delete(message.id);
    if (message.error) reject(new Error(message.error.message || JSON.stringify(message.error)));
    else resolve(message.result);
  };
  return {
    async send(method, params = {}) {
      await opened;
      const id = nextID++;
      const promise = new Promise((resolve, reject) => pending.set(id, { resolve, reject }));
      ws.send(JSON.stringify({ id, method, params }));
      return promise;
    },
    close() {
      ws.close();
    }
  };
}

async function collectViaBrowserContext() {
  const endpoint = process.env.ASSETIWEAVE_CHATGPT_CDP_ENDPOINT || "http://127.0.0.1:9222";
  const targets = await readRemoteDebugJSON(endpoint.replace(/\/$/, "") + "/json/list");
  const target = targets.find((item) =>
    item.type === "page" &&
    typeof item.url === "string" &&
    /^https:\/\/chatgpt\.com\/?/.test(item.url) &&
    item.webSocketDebuggerUrl
  );
  if (!target) {
    throw new Error("ChatGPT page was not found on DevTools endpoint " + endpoint);
  }

  const client = createCDPClient(target.webSocketDebuggerUrl);
  try {
    await client.send("Runtime.enable");
    const limit = Number(process.env.ASSETIWEAVE_CHATGPT_LIMIT || 100);
    const expression = String.raw`(async (limit) => {
      const readJSON = async (url, init) => {
        const response = await fetch(url, init);
        const body = await response.text();
        let json = null;
        try { json = JSON.parse(body); } catch {}
        return { status_code: response.status, body, json };
      };
      const session = await readJSON("/api/auth/session", {
        credentials: "include",
        headers: { "Accept": "application/json" }
      });
      if (session.status_code !== 200 || !session.json || typeof session.json.accessToken !== "string") {
        throw new Error("ChatGPT browser session did not return an access token; status=" + session.status_code);
      }
      const headers = {
        "Accept": "application/json",
        "Authorization": "Bearer " + session.json.accessToken,
        "oai-language": "en-US"
      };
      const params = new URLSearchParams({
        offset: "0",
        limit: String(limit),
        order: "updated"
      });
      const list = await readJSON("/backend-api/conversations?" + params.toString(), {
        credentials: "include",
        headers
      });
      if (list.status_code !== 200) {
        throw new Error("ChatGPT browser list request failed with status " + list.status_code);
      }
      const body = list.json || {};
      const items = Array.isArray(body.items)
        ? body.items
        : body.data && Array.isArray(body.data.items)
          ? body.data.items
          : Array.isArray(body.conversations)
            ? body.conversations
            : [];
      const details = [];
      for (let index = 0; index < items.length; index++) {
        const id = typeof items[index].id === "string" ? items[index].id : "";
        if (!id) continue;
        const detail = await readJSON("/backend-api/conversation/" + encodeURIComponent(id), {
          credentials: "include",
          headers
        });
        details.push({ index: index + 1, id, snapshot: detail });
      }
      return { target_url: location.href, list, items, details };
    })(${JSON.stringify(Number.isFinite(limit) && limit > 0 ? Math.min(limit, 100) : 100)})`;
    const evaluated = await client.send("Runtime.evaluate", {
      expression,
      awaitPromise: true,
      returnByValue: true,
      timeout: 120000
    });
    if (evaluated.exceptionDetails) {
      throw new Error(evaluated.exceptionDetails.text || "ChatGPT browser collection failed");
    }
    const value = evaluated.result && evaluated.result.value;
    if (!value || !Array.isArray(value.items) || !Array.isArray(value.details)) {
      throw new Error("ChatGPT browser collection returned an invalid payload");
    }
    writeJSON(path.join(rawDir, "context.json"), {
      browser_context: true,
      browser_target_url: value.target_url,
      access_token_found: true
    });
    writeJSON(path.join(rawDir, "list-page-1.json"), {
      status_code: value.list.status_code,
      body: value.list.body
    });
    return {
      listItems: value.items,
      details: value.details
    };
  } finally {
    client.close();
  }
}

async function collectDirect() {
  const authProbe = readJSON(path.join(root, "requests", "auth-probe.json"));
  const authHeaders = readAuthHeaders(authProbe);
  const tokenResult = await resolveAccessToken(authProbe, authHeaders);
  writeJSON(path.join(rawDir, "context.json"), {
    session_status: tokenResult.sessionStatus,
    session_bytes: tokenResult.sessionBytes,
    access_token_found: true
  });
  const headers = apiHeaders(authHeaders, tokenResult.token);

  const listItems = [];
  const seenSessions = new Set();
  const limit = 100;
  for (let offset = 0, page = 1; page <= 200; page++, offset += limit) {
    const params = new URLSearchParams({
      offset: String(offset),
      limit: String(limit),
      order: "updated"
    });
    const url = "https://chatgpt.com/backend-api/conversations?" + params.toString();
    const snapshot = await requestJSON(url, headers);
    writeJSON(path.join(rawDir, `list-page-${page}.json`), {
      status_code: snapshot.status_code,
      body: snapshot.body
    });
    if (snapshot.status_code !== 200) {
      throw new Error(`ChatGPT list page ${page} failed with status ${snapshot.status_code}`);
    }
    const items = listItemsFromSnapshot(snapshot);
    for (const item of items) {
      const id = text(item.id);
      if (!id || seenSessions.has(id)) continue;
      seenSessions.add(id);
      listItems.push(item);
    }
    const total = totalFromSnapshot(snapshot);
    if (items.length < limit || (typeof total === "number" && listItems.length >= total)) break;
  }

  const details = [];
  for (let index = 0; index < listItems.length; index++) {
    const item = listItems[index];
    const sessionID = text(item.id);
    const url = "https://chatgpt.com/backend-api/conversation/" + encodeURIComponent(sessionID);
    const snapshot = await requestJSON(url, headers);
    details.push({ index: index + 1, id: sessionID, snapshot });
  }
  return { listItems, details };
}

function normalizeCollection(listItems, details) {
  const sessions = [];
  let detailFailures = 0;
  const itemByID = new Map(listItems.map((item) => [text(item.id), item]));
  for (const detail of details) {
    const item = itemByID.get(text(detail.id)) || {};
    const sessionID = text(detail.id);
    const snapshot = detail.snapshot;
    writeJSON(path.join(detailDir, `${String(detail.index).padStart(4, "0")}-${safeName(sessionID)}.json`), {
      status_code: snapshot.status_code,
      body: snapshot.body
    });
    if (snapshot.status_code !== 200 || !snapshot.json) {
      detailFailures++;
      continue;
    }
    const turns = parseConversation(snapshot.json);
    if (!turns.length) continue;
    const updatedAt = timestamp(snapshot.json.update_time) || timestamp(item.update_time);
    sessions.push({
      external_id: sessionID,
      title: text(snapshot.json.title) || text(item.title) || null,
      project_path: null,
      started_at: timestamp(snapshot.json.create_time) || timestamp(item.create_time),
      updated_at: updatedAt,
      source_locator: `https://chatgpt.com/c/${encodeURIComponent(sessionID)}`,
      source_fingerprint: crypto.createHash("sha256").update(JSON.stringify({
        id: sessionID,
        updatedAt,
        turns
      })).digest("hex"),
      turns
    });
  }
  return { sessions, detailFailures };
}

(async () => {
  mkdirp(detailDir);
  mkdirp(normalizedDir);

  let collection;
  let fallbackError = null;
  try {
    collection = await collectDirect();
  } catch (error) {
    fallbackError = error;
    collection = await collectViaBrowserContext().catch((browserError) => {
      const directMessage = error && error.message ? error.message : String(error);
      const browserMessage = browserError && browserError.message ? browserError.message : String(browserError);
      throw new Error(`ChatGPT direct collection failed: ${directMessage}; browser-context fallback failed: ${browserMessage}. Start Edge/Chrome with --remote-debugging-port=9222 and keep https://chatgpt.com open.`);
    });
  }

  const { sessions, detailFailures } = normalizeCollection(collection.listItems, collection.details);

  writeJSON(normalizedFile, { sessions });
  const turnCount = sessions.reduce((sum, session) => sum + session.turns.length, 0);
  console.log(JSON.stringify({
    ok: true,
    site_id: "chatgpt-web",
    raw_run_dir: rawDir,
    normalized_file: normalizedFile,
    listed_sessions: collection.listItems.length,
    session_count: sessions.length,
    turn_count: turnCount,
    detail_failures: detailFailures,
    used_browser_context: Boolean(fallbackError)
  }));
})().catch((error) => {
  console.error(error && error.message ? error.message : String(error));
  process.exit(1);
});
