#!/usr/bin/env node
/**
 * @file harvest.js — ChatGPT Web 会话增量抓取与归一化导出脚本
 *
 * 架构设计与抓取流水线：
 * 1. 【优先路径 - CDP Browser Mode】：优先使用 CDP 连接已有的/无头 Chrome 浏览器上下文，在浏览器环境中安全执行 API 抓取
 * 2. 【退化路径 - Direct Cookie Mode】：若 CDP 不可用，使用已保存的 Cookie/AccessToken 进行直接 HTTP API 请求
 * 3. 【自动恢复 - Auth Retry】：当 401/403 失败时，触发 `assetiweave-cli auth-detect` 尝试重刷新 Cookie 凭据
 * 4. 【增量缓存与导流】：对比本地 sessions.json 的 updated_at 时间戳，未变动会话跳过详情下载
 */
const fs = require("fs");
const path = require("path");
const crypto = require("crypto");
const { parseConversation, timestamp } = require("./chatgpt-normalize.cjs");
const { acquireCDPTarget, tryRefreshAuth } = require("./cdp-browser.cjs");

const root = process.env.ASSETIWEAVE_HARVESTER_DIR || process.cwd();
const runID = new Date().toISOString().replace(/[-:]/g, "").replace(/\.\d{3}Z$/, "Z");
const rawDir = path.join(root, "output", "raw", runID);
const detailDir = path.join(rawDir, "details");
const normalizedDir = path.join(root, "output", "normalized");
const normalizedFile = path.join(normalizedDir, "sessions.json");
const forceFullReparse = process.env.ASSETIWEAVE_FULL_REPARSE === "1";

/** 本地已有会话缓存映射表 external_id -> session */
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

/** 递归创建包含文件系统权限 (0700) 的目录 */
function mkdirp(dir) {
  fs.mkdirSync(dir, { recursive: true, mode: 0o700 });
}

/** 读取并解析 JSON 文件 */
function readJSON(file) {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

/** 写入具有安全权限 (0600) 的格式化 JSON 文件 */
function writeJSON(file, value) {
  mkdirp(path.dirname(file));
  fs.writeFileSync(file, JSON.stringify(value, null, 2) + "\n", { mode: 0o600 });
}

/** 文本格式化与清理 */
function text(value) {
  return typeof value === "string" ? value.trim() : "";
}

/** 过滤生成安全的文件名字符串 */
function safeName(value) {
  return String(value).replace(/[^A-Za-z0-9._-]+/g, "_").slice(0, 160) || "item";
}

/**
 * 封装安全的 HTTP JSON 请求
 * @param {string} url - 目标 API URL
 * @param {object} headers - HTTP Headers
 * @returns {Promise<{ status_code: number, body: string, json: any }>}
 */
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

/**
 * 从凭据探测结果中解析抽取 User-Agent, Cookie 与 Authorization Header
 * @param {object} authProbe - auth-probe.json 解析对象
 */
function readAuthHeaders(authProbe) {
  const headers = authProbe.headers || {};
  return {
    userAgent: headers["User-Agent"] || headers["user-agent"] || "Mozilla/5.0",
    cookie: headers.Cookie || headers.cookie || "",
    authorization: headers.Authorization || headers.authorization || ""
  };
}

/**
 * 解析并验证 ChatGPT 交互用的 Bearer AccessToken
 *
 * @param {object} authProbe - 凭据对象
 * @param {object} authHeaders - 头部信息
 * @returns {Promise<{ token: string, sessionStatus: number, sessionBytes: number }>}
 */
async function resolveAccessToken(authProbe, authHeaders) {
  if (/^Bearer\s+\S+/i.test(authHeaders.authorization)) {
    return {
      token: authHeaders.authorization.replace(/^Bearer\s+/i, ""),
      sessionStatus: 0,
      sessionBytes: 0
    };
  }
  if (!authHeaders.cookie) {
    throw new Error("ChatGPT Cookie 登录状态丢失。请运行: assetiweave-cli conversation web auth-detect " + root + " --domain chatgpt.com --credential cookie");
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
    throw new Error(`ChatGPT session 校验响应异常，状态码 ${snapshot.status_code}`);
  }
  const token = snapshot.json && typeof snapshot.json.accessToken === "string"
    ? snapshot.json.accessToken.trim()
    : "";
  if (!token) {
    throw new Error("ChatGPT session 未能返回 accessToken，请使用 assetiweave-cli 重新探测 Cookie。");
  }
  return {
    token,
    sessionStatus: snapshot.status_code,
    sessionBytes: snapshot.body.length
  };
}

/** 构造请求 ChatGPT API 所需的标准 HTTP 头部 */
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

/** 从响应 Snapshot 中提炼会话列表项 */
function listItemsFromSnapshot(snapshot) {
  const body = snapshot.json || {};
  if (Array.isArray(body.items)) return body.items;
  if (body.data && Array.isArray(body.data.items)) return body.data.items;
  if (Array.isArray(body.conversations)) return body.conversations;
  return [];
}

/** 从响应 Snapshot 中获取会话总条数 */
function totalFromSnapshot(snapshot) {
  const body = snapshot.json || {};
  if (typeof body.total === "number") return body.total;
  if (body.data && typeof body.data.total === "number") return body.data.total;
  return null;
}

// ---------------------------------------------------------------------------
// CDP 浏览器环境抓取 (优先路径)
// ---------------------------------------------------------------------------

/**
 * 【优先路径】在 CDP 调试浏览器环境中直接注入与执行抓取逻辑
 *
 * 优点：直接继承已打开网页的域名 Client Session、Cookie 与 Cloudflare 人机验证解封状态。
 *
 * @returns {Promise<{ listItems: object[], details: object[] }>}
 */
async function collectViaBrowserContext() {
  const { client, target, launched } = await acquireCDPTarget({
    urlPattern: /^https:\/\/chatgpt\.com\/?/,
    siteURL: "https://chatgpt.com",
    endpointEnv: "ASSETIWEAVE_CHATGPT_CDP_ENDPOINT",
  });

  try {
    await client.send("Runtime.enable");
    const limit = Number(process.env.ASSETIWEAVE_CHATGPT_LIMIT || 100);
    // 注入页面上执行的异步提取表达式
    const expression = String.raw`(async (limit) => {
      const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
      for (let i = 0; i < 40; i++) {
        if (window.location.hostname.includes("chatgpt.com")) break;
        await sleep(250);
      }
      const readJSON = async (url, init) => {
        const response = await fetch(url, init);
        const body = await response.text();
        let json = null;
        try { json = JSON.parse(body); } catch {}
        return { status_code: response.status, body, json };
      };
      const timestamp = (sec) => {
        if (typeof sec !== "number") return null;
        return new Date(sec * 1000).toISOString();
      };
      const session = await readJSON("/api/auth/session", {
        credentials: "include",
        headers: { "Accept": "application/json" }
      });
      if (session.status_code !== 200 || !session.json || typeof session.json.accessToken !== "string") {
        throw new Error("ChatGPT 浏览器 session 未能获取 access token，状态码=" + session.status_code);
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
        throw new Error("ChatGPT 浏览器请求会话列表失败，状态码 " + list.status_code);
      }
      const body = list.json || {};
      const items = Array.isArray(body.items)
        ? body.items
        : body.data && Array.isArray(body.data.items)
          ? body.data.items
          : Array.isArray(body.conversations)
            ? body.conversations
            : [];
      const cache = ${JSON.stringify(Object.fromEntries(Array.from(existingSessions.entries()).map(([k, v]) => [k, v.updated_at])))};
      const details = [];
      for (let index = 0; index < items.length; index++) {
        const id = typeof items[index].id === "string" ? items[index].id : "";
        if (!id) continue;
        const item = items[index];
        const updatedAt = timestamp(item.update_time);
        if (!${JSON.stringify(forceFullReparse)} && cache[id] === updatedAt) {
          continue; // 本地缓存已是最新，跳过该会话的详情 Fetch
        }
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
      throw new Error(evaluated.exceptionDetails.text || "ChatGPT 浏览器环境抓取失败");
    }
    const value = evaluated.result && evaluated.result.value;
    if (!value || !Array.isArray(value.items) || !Array.isArray(value.details)) {
      throw new Error("ChatGPT 浏览器抓取返回了无效的载荷格式");
    }
    writeJSON(path.join(rawDir, "context.json"), {
      browser_context: true,
      browser_target_url: value.target_url,
      browser_launched: launched,
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

// ---------------------------------------------------------------------------
// 直接基于 Cookie 的 HTTP API 抓取 (退化路径)
// ---------------------------------------------------------------------------

/**
 * 【退化路径】基于本地探测保存的 Cookie 字符串进行直接 HTTP API 翻页请求
 *
 * @returns {Promise<{ listItems: object[], details: object[] }>}
 */
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
      throw new Error(`ChatGPT 会话列表第 ${page} 页请求失败，状态码 ${snapshot.status_code}`);
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
    
    // 对比本地缓存
    const existing = existingSessions.get(sessionID);
    const updatedAt = timestamp(item.update_time);
    if (!forceFullReparse && existing && existing.updated_at === updatedAt) {
      continue;
    }

    const url = "https://chatgpt.com/backend-api/conversation/" + encodeURIComponent(sessionID);
    const snapshot = await requestJSON(url, headers);
    details.push({ index: index + 1, id: sessionID, snapshot });
  }
  return { listItems, details };
}

// ---------------------------------------------------------------------------
// 带有自动重新探测 refresh 机制的 Direct 抓取
// ---------------------------------------------------------------------------

/**
 * 带有失败自动刷新凭据重试能力的 Direct 抓取封装
 */
async function collectDirectWithRetry() {
  try {
    return await collectDirect();
  } catch (firstError) {
    // 当遇到 401/403 等报错时，自动拉起 CLI 探测重刷新
    process.stderr.write(`[chatgpt-web] 直接抓取失败: ${firstError.message}; 尝试触发 auth-detect 重新刷新...\n`);
    const refreshed = tryRefreshAuth(root, "chatgpt.com");
    if (!refreshed) throw firstError;
    // 重试一次
    return await collectDirect();
  }
}

// ---------------------------------------------------------------------------
// 会话数据归一化导出 (Normalization)
// ---------------------------------------------------------------------------

/**
 * 将抓取到的列表与详情 JSON 转译合并为标准的 sessions.json 数据
 *
 * @param {object[]} listItems - 列表节点
 * @param {object[]} details - 详情节点
 * @returns {{ sessions: object[], detailFailures: number }}
 */
function normalizeCollection(listItems, details) {
  const sessions = [];
  let detailFailures = 0;
  for (const item of listItems) {
    const sessionID = text(item.id);
    const detail = details.find((d) => text(d.id) === sessionID);
    if (detail) {
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
    } else {
      const existing = existingSessions.get(sessionID);
      if (existing) {
        sessions.push(existing);
      }
    }
  }
  return { sessions, detailFailures };
}

// ---------------------------------------------------------------------------
// 主入口逻辑 — 优先 CDP，退化直接 Cookie 模式
// ---------------------------------------------------------------------------

(async () => {
  mkdirp(detailDir);
  mkdirp(normalizedDir);

  let collection;
  let usedBrowserContext = false;
  let directFailed = false;

  // 执行策略：优先 CDP 浏览器模式 (ChatGPT 抓取成功率最高)，失败后回退到直连 Cookie 模式
  try {
    collection = await collectViaBrowserContext();
    usedBrowserContext = true;
  } catch (browserError) {
    process.stderr.write(`[chatgpt-web] CDP 浏览器抓取失败: ${browserError.message}; 降级退化到直接 Cookie 抓取...\n`);
    directFailed = true;
    try {
      collection = await collectDirectWithRetry();
    } catch (directError) {
      const browserMessage = browserError && browserError.message ? browserError.message : String(browserError);
      const directMessage = directError && directError.message ? directError.message : String(directError);
      throw new Error(
        `ChatGPT CDP 浏览器抓取失败: ${browserMessage}; ` +
        `直接 Cookie 抓取亦失败: ${directMessage}。` +
        `请启动 Chrome/Edge 带有 --remote-debugging-port=9222 端口并打开 https://chatgpt.com，` +
        `或运行: assetiweave-cli conversation web auth-detect ${root} --domain chatgpt.com --credential cookie`
      );
    }
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
    used_browser_context: usedBrowserContext,
    direct_collection_failed: directFailed
  }));
})().catch((error) => {
  console.error(error && error.message ? error.message : String(error));
  process.exit(1);
});
