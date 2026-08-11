#!/usr/bin/env node
/**
 * @file harvest.js — 通义千问 (Qwen) Web 会话增量抓取与归一化导出脚本
 *
 * 架构设计与抓取流水线：
 * 1. 从 requests/auth-probe.json 中提取 Cookie 与 Cookie "cna" 值构造 `ut` 签名参数
 * 2. 分页请求 `api/v1/session/list` 接口获取所有会话摘要列表
 * 3. 比较本地 sessions.json 中的 updated_at，跳过未更新的会话
 * 4. 对更新或新会话，调用 `api/v1/session/msg/list` 提取具体 Round 消息明细并格式化为 Turn/Part
 * 5. 将会话导出至 output/normalized/sessions.json
 */
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
const forceFullReparse = process.env.ASSETIWEAVE_FULL_REPARSE === "1";

/** 本地已有会话缓存映射 external_id -> session */
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

/** 递归创建 0700 权限目录 */
function mkdirp(dir) {
  fs.mkdirSync(dir, { recursive: true, mode: 0o700 });
}

/** 读取解析 JSON 文件 */
function readJSON(file) {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

/** 写入 0600 权限格式化 JSON */
function writeJSON(file, value) {
  mkdirp(path.dirname(file));
  fs.writeFileSync(file, JSON.stringify(value, null, 2) + "\n", { mode: 0o600 });
}

/** 从 Cookie Header 中提取特定 Cookie 项的值 */
function cookieValue(cookieHeader, name) {
  for (const part of String(cookieHeader || "").split(";")) {
    const index = part.indexOf("=");
    if (index <= 0) continue;
    if (part.slice(0, index).trim() === name) return part.slice(index + 1).trim();
  }
  return "";
}

/** 文本格式化 */
function text(value) {
  return typeof value === "string" ? value.trim() : "";
}

/** 构造 Qwen API 通用 URL 查询参数 (包含 ut 追踪签名、biz_id等) */
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

/** 安全的 HTTP JSON 请求包装 */
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
    throw new Error("Qwen Cookie 登录状态缺失。请运行: assetiweave-cli conversation web auth-detect " + root + " --domain qianwen.com --credential cookie");
  }
  const ut = cookieValue(cookie, "cna");
  if (!ut) {
    throw new Error("Qwen cna cookie 缺失，无法构造请求参数 ut。");
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

  // 1. 分页获取全部 Session 列表
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
    if (snapshot.status_code !== 200) throw new Error(`Qwen 列表第 ${page} 页请求失败，状态码 ${snapshot.status_code}`);
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

  // 2. 依次抓取详情与转译
  for (let index = 0; index < listItems.length; index++) {
    const item = listItems[index];
    const sessionID = text(item.session_id);
    const updatedAt = text(item.update_time) || null;
    const existing = existingSessions.get(sessionID);
    if (!forceFullReparse && existing && existing.updated_at === updatedAt) {
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
      if (snapshot.status_code !== 200) throw new Error(`Qwen 详情 ${sessionID} 第 ${page} 页请求失败，状态码 ${snapshot.status_code}`);
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

  // 3. 写入归一化文件
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
