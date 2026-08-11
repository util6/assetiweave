/**
 * @file cdp-browser.cjs — 共享 CDP (Chrome DevTools Protocol)  WebSocket 客户端与浏览器自动探测模块
 *
 * 核心职责：
 *   1. 创建轻量级 CDP WebSocket 客户端 (createCDPClient)
 *   2. 多端口与后台进程探测 Debug 调试端点 (discoverCDPEndpoint)
 *   3. 根据 URL 正则表达式匹配目标浏览器标签页 (findCDPTarget)
 *   4. 无头模式自动启动调试端口浏览器 (launchCDPBrowser)
 *   5. 高级获取适配器对象 (acquireCDPTarget)，整合已有连接复用与自动化注入 Cookie
 *   6. 鉴权失败时的 Cookie / Auth 自动检测刷新机制 (tryRefreshAuth)
 *
 * 使用示例 (harvest.js 调用):
 *   const { acquireCDPTarget, createCDPClient } = require("./cdp-browser.cjs");
 *   const { client, target, launched } = await acquireCDPTarget({
 *     urlPattern: /^https:\/\/chatgpt\.com\/?/,
 *     siteURL: "https://chatgpt.com",
 *     endpointEnv: "ASSETIWEAVE_CHATGPT_CDP_ENDPOINT",
 *   });
 */
"use strict";

const { execSync, spawn } = require("child_process");
const path = require("path");
const os = require("os");

// ---------------------------------------------------------------------------
// CDP WebSocket 客户端实现
// ---------------------------------------------------------------------------

/**
 * 创建 CDP (Chrome DevTools Protocol) WebSocket 通信客户端
 *
 * @param {string} webSocketDebuggerURL - 目标标签页的 WebSocket 调试 URL (ws://...)
 * @returns {{ send: (method: string, params?: object) => Promise<any>, close: () => void }} CDP 交互客户端
 */
function createCDPClient(webSocketDebuggerURL) {
  let nextID = 1;
  const pending = new Map();
  const ws = new WebSocket(webSocketDebuggerURL);
  const opened = new Promise((resolve, reject) => {
    ws.onopen = resolve;
    ws.onerror = () => reject(new Error("连接 DevTools websocket 失败"));
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
    /**
     * 发送 CDP 命令并等待 JSON-RPC 响应结果
     * @param {string} method - CDP 方法名称 (如 "Network.enable", "Page.navigate")
     * @param {object} [params={}] - 方法参数对象
     * @returns {Promise<any>}
     */
    async send(method, params = {}) {
      await opened;
      const id = nextID++;
      const promise = new Promise((resolve, reject) => pending.set(id, { resolve, reject }));
      ws.send(JSON.stringify({ id, method, params }));
      return promise;
    },
    /**
     * 关闭 WebSocket 连接
     */
    close() {
      ws.close();
    }
  };
}

// ---------------------------------------------------------------------------
// DevTools HTTP JSON 端点请求辅助工具
// ---------------------------------------------------------------------------

/**
 * 请求 DevTools 暴露的 HTTP JSON 端点 (如 /json/version, /json/list)
 *
 * @param {string} url - 目标 HTTP URL
 * @param {number} [timeoutMs=3000] - 超时毫秒数
 * @returns {Promise<any>} JSON 响应体
 */
async function fetchJSON(url, timeoutMs = 3000) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const response = await fetch(url, { signal: controller.signal });
    if (!response.ok) throw new Error(`HTTP 状态码异常 ${response.status}`);
    return await response.json();
  } finally {
    clearTimeout(timer);
  }
}

// ---------------------------------------------------------------------------
// CDP endpoint discovery
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// CDP 端点探测与发现
// ---------------------------------------------------------------------------

/** 默认探测的 DevTools 调试端口 */
const DEFAULT_PORTS = [9222, 9333, 9229];

/**
 * 探测并寻找当前系统中已开启的 CDP 调试端点
 *
 * 探测顺序：
 * 1. 检查环境变量 (如 ASSETIWEAVE_CHATGPT_CDP_ENDPOINT) 显式指定的端点
 * 2. 依次轮询默认端口 (9222, 9333, 9229)
 * 3. macOS/Linux 系统下使用 ps 扫描带有 --remote-debugging-port 参数的活跃浏览器进程
 *
 * @param {object} [options={}]
 * @param {string} [options.endpointEnv] - 环境变量名称
 * @param {number[]} [options.ports] - 自定义端口列表
 * @returns {Promise<string|null>} 成功返回端点 Base URL (如 "http://127.0.0.1:9222")，失败返回 null
 */
async function discoverCDPEndpoint(options = {}) {
  // 1. 优先使用环境变量指定的端点
  const envEndpoint = options.endpointEnv ? process.env[options.endpointEnv] : null;
  if (envEndpoint) {
    try {
      await fetchJSON(envEndpoint.replace(/\/$/, "") + "/json/version");
      return envEndpoint.replace(/\/$/, "");
    } catch {}
  }

  // 2. 尝试常见调试端口
  const ports = options.ports || DEFAULT_PORTS;
  for (const port of ports) {
    const base = `http://127.0.0.1:${port}`;
    try {
      await fetchJSON(base + "/json/version");
      return base;
    } catch {}
  }

  // 3. 在 macOS/Linux 上扫描系统中正在运行的浏览器进程
  if (os.platform() === "darwin" || os.platform() === "linux") {
    try {
      const scanned = scanRunningBrowserPorts();
      for (const port of scanned) {
        if (ports.includes(port)) continue; // 忽略已尝试过的端口
        const base = `http://127.0.0.1:${port}`;
        try {
          await fetchJSON(base + "/json/version");
          return base;
        } catch {}
      }
    } catch {}
  }

  return null;
}

/**
 * 使用 `ps` 命令检测运行中的 Chromium 类浏览器（Chrome, Edge, Brave等），
 * 提取其命令行参数中的 `--remote-debugging-port` 端口号
 *
 * @returns {number[]} 探测到的端口列表
 */
function scanRunningBrowserPorts() {
  try {
    const output = execSync(
      "ps aux 2>/dev/null | grep -E '(Chrome|Edge|Brave|Chromium|chrome|edge)' | grep -- '--remote-debugging-port=' | grep -v grep",
      { encoding: "utf8", timeout: 3000 }
    );
    const ports = [];
    for (const line of output.split("\n")) {
      const match = line.match(/--remote-debugging-port=(\d+)/);
      if (match) {
        const port = parseInt(match[1], 10);
        if (port > 0 && port < 65536 && !ports.includes(port)) ports.push(port);
      }
    }
    return ports;
  } catch {
    return [];
  }
}

// ---------------------------------------------------------------------------
// 目标页面 (Tab) 发现
// ---------------------------------------------------------------------------

/**
 * 在目标 CDP 端点中匹配满足指定 URL 正则条件的标签页 Target
 *
 * @param {string} endpoint - CDP 端点 Base URL
 * @param {RegExp} urlPattern - 用于匹配标签页 URL 的正则表达式
 * @returns {Promise<object|null>} 匹配到的 Target 对象（包含 webSocketDebuggerUrl），若无匹配则返回 null
 */
async function findCDPTarget(endpoint, urlPattern) {
  const targets = await fetchJSON(endpoint + "/json/list");
  return targets.find(
    (item) =>
      item.type === "page" &&
      typeof item.url === "string" &&
      urlPattern.test(item.url) &&
      item.webSocketDebuggerUrl
  ) || null;
}

// ---------------------------------------------------------------------------
// 浏览器无头模式自动拉起
// ---------------------------------------------------------------------------

/** macOS 下常见的 Chromium 衍生浏览器可执行文件安装路径 */
const BROWSER_PATHS_MACOS = [
  "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
  "/Applications/Chromium.app/Contents/MacOS/Chromium",
];

/** Linux 下常见的 Chromium 衍生浏览器可执行文件安装路径 */
const BROWSER_PATHS_LINUX = [
  "/usr/bin/microsoft-edge",
  "/usr/bin/microsoft-edge-stable",
  "/usr/bin/google-chrome",
  "/usr/bin/google-chrome-stable",
  "/usr/bin/brave-browser",
  "/usr/bin/chromium-browser",
  "/usr/bin/chromium",
];

/**
 * 寻找本地系统中安装的可用 Chromium 浏览器可执行路径
 *
 * @returns {string|null} 找到返回绝对路径，否则返回 null
 */
function findBrowserExecutable() {
  const fs = require("fs");
  const paths = os.platform() === "darwin" ? BROWSER_PATHS_MACOS : BROWSER_PATHS_LINUX;
  for (const p of paths) {
    try {
      if (fs.existsSync(p)) return p;
    } catch {}
  }
  return null;
}

/**
 * 从 auth-probe.json 中提取 User-Agent 字符串，保证网络请求的指纹一致性
 *
 * @returns {string} 提取到的 User-Agent 字符串或默认 Chrome UA
 */
function readUAFromProbe() {
  const rootDir = process.env.ASSETIWEAVE_HARVESTER_DIR || process.cwd();
  const authProbePath = path.join(rootDir, "requests", "auth-probe.json");
  if (fs.existsSync(authProbePath)) {
    try {
      const authProbe = JSON.parse(fs.readFileSync(authProbePath, "utf8"));
      const ua = authProbe.headers && (authProbe.headers["User-Agent"] || authProbe.headers["user-agent"]);
      if (ua) return ua;
    } catch {}
  }
  return "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";
}

/**
 * 自动拉起带有 --remote-debugging-port 的无头浏览器
 *
 * @param {object} [options={}]
 * @param {string} [options.siteURL] - 启动后自动导航到的初始 URL
 * @param {number} [options.port=9222] - 调试端口
 * @param {number} [options.waitMs=6000] - 等待浏览器就绪的超时时间
 * @returns {Promise<{ endpoint: string, process: any, launched: boolean }>}
 */
async function launchCDPBrowser(options = {}) {
  const browserPath = findBrowserExecutable();
  if (!browserPath) {
    throw new Error(
      "No supported browser found. Install Chrome, Edge, or Brave and ensure it is in the standard location."
    );
  }

  const port = options.port || 9222;
  const siteURL = options.siteURL || "about:blank";

  // Use a dedicated profile for harvesting to avoid locking the user's active profile.
  // This profile inherits nothing by default — but we can copy cookies into it if needed.
  // For now, we try the user's default profile first. If it's locked, we fall back to
  // a harvester-dedicated profile.
  const harvesterProfileDir = path.join(os.homedir(), ".assetiweave", "browser-profile");

  const args = [
    `--remote-debugging-port=${port}`,
    "--headless=new",
    `--user-agent=${readUAFromProbe()}`,
    "--disable-gpu",
    "--no-first-run",
    "--no-default-browser-check",
    "--disable-extensions",
    `--user-data-dir=${harvesterProfileDir}`,
    siteURL
  ];

  const child = spawn(browserPath, args, {
    detached: true,
    stdio: "ignore",
  });
  child.unref();

  // Wait for the browser to be ready
  const waitMs = options.waitMs || 6000;
  const startTime = Date.now();
  const endpoint = `http://127.0.0.1:${port}`;
  while (Date.now() - startTime < waitMs) {
    try {
      await fetchJSON(endpoint + "/json/version");
      return { endpoint, process: child, launched: true };
    } catch {}
    await new Promise((r) => setTimeout(r, 300));
  }
  // Cleanup on failure
  try { child.kill(); } catch {}
  throw new Error(`Browser failed to start within ${waitMs}ms on port ${port}`);
}

// ---------------------------------------------------------------------------
// High-level: acquire a CDP target for a specific site
// ---------------------------------------------------------------------------

const fs = require("fs");

/**
 * 将标准的 Cookie 字符串 (name=value; name2=value2) 解析转换为 CDP Network.setCookie 所需的结构对象
 *
 * @param {string} cookieStr - 原始 Header 中的 Cookie 字符串
 * @param {string} defaultDomain - 默认域名 (如 "chatgpt.com")
 * @param {string} siteURL - 站点绝对 URL
 * @returns {object[]} CDP 规范的 Cookie 对象数组
 */
function parseCookieString(cookieStr, defaultDomain, siteURL) {
  return cookieStr.split(";").map(pair => {
    const trimmed = pair.trim();
    const idx = trimmed.indexOf("=");
    if (idx === -1) return null;
    const name = trimmed.substring(0, idx).trim();
    const value = trimmed.substring(idx + 1).trim();
    
    const isHostCookie = name.startsWith("__Host-");
    const cookieObj = {
      name,
      value,
      url: siteURL,
      path: "/",
      secure: true
    };
    
    if (!isHostCookie) {
      let domain = defaultDomain;
      if (!domain.startsWith(".")) {
        domain = "." + domain;
      }
      cookieObj.domain = domain;
    }
    
    return cookieObj;
  }).filter(Boolean);
}

/**
 * 从 auth-probe.json 文件读取抓取探针录制的 Cookie 与 User-Agent，并通过 CDP 命令动态注入到当前浏览器上下文中
 *
 * @param {object} client - CDP 客户端句柄
 * @param {string} siteURL - 目标站点 URL
 */
async function injectCookiesFromProbe(client, siteURL) {
  const rootDir = process.env.ASSETIWEAVE_HARVESTER_DIR || process.cwd();
  const authProbePath = path.join(rootDir, "requests", "auth-probe.json");
  if (!fs.existsSync(authProbePath)) return;

  try {
    const authProbe = JSON.parse(fs.readFileSync(authProbePath, "utf8"));
    const cookieHeader = authProbe.headers && (authProbe.headers.Cookie || authProbe.headers.cookie);
    if (!cookieHeader) return;

    const hostname = new URL(siteURL).hostname;
    const cookies = parseCookieString(cookieHeader, hostname, siteURL);

    await client.send("Network.enable");
    try {
      await client.send("Network.setUserAgentOverride", { userAgent: readUAFromProbe() });
    } catch (uaErr) {
      process.stderr.write(`[cdp-browser] warning: override user-agent 失败: ${uaErr.message || uaErr}\n`);
    }
    for (const cookie of cookies) {
      // 避免个别特殊 cookie 校验失败导致全部失败
      try {
        await client.send("Network.setCookie", cookie);
      } catch (cookieErr) {
        process.stderr.write(`[cdp-browser] warning: 设置 cookie ${cookie.name} 失败: ${cookieErr.message || cookieErr}\n`);
      }
    }
    process.stderr.write(`[cdp-browser] 成功为 ${hostname} 注入 Cookie 到浏览器上下文中\n`);
  } catch (err) {
    process.stderr.write(`[cdp-browser] 注入 Cookie 过程异常: ${err.message || err}\n`);
  }
}

/**
 * 获取或连接到特定网站的 CDP 页面 Target
 *
 * 获取策略流水线：
 * Step 1: 尝试探测已经开启的 CDP 调试端点（轮询端口 + ps 扫描）
 * Step 2: 若端点存在，查找匹配 urlPattern 的已打开标签页；若不存在标签页则尝试通过 CDP 新开标签页
 * Step 3: 若允许拉起且上述均未找到，自动拉起一个新的无头 Chrome 实例
 * Step 4: 等待目标页面就绪，并调用 injectCookiesFromProbe 注入凭据
 *
 * @param {object} options
 * @param {RegExp} options.urlPattern - 用于匹配目标标签页 URL 的正则表达式
 * @param {string} options.siteURL - 网站 URL
 * @param {string} [options.endpointEnv] - 指定 CDP 端点的环境变量名
 * @param {boolean} [options.allowLaunch=true] - 当找不到已有浏览器时是否允许自动拉起
 * @returns {Promise<{ client: any, target: any, launched: boolean }>}
 */
async function acquireCDPTarget(options) {
  const { urlPattern, siteURL, endpointEnv, allowLaunch = true } = options;

  // 步骤 1: 寻找现有端点
  let endpoint = await discoverCDPEndpoint({ endpointEnv });
  let launched = false;

  if (endpoint) {
    // 步骤 2: 查找已打开的匹配 Target
    const target = await findCDPTarget(endpoint, urlPattern);
    if (target) {
      const client = createCDPClient(target.webSocketDebuggerUrl);
      await injectCookiesFromProbe(client, siteURL);
      return { client, target, launched: false };
    }
    // 端点存在但标签页未打开，尝试在现有浏览器中打开新标签页
    try {
      const newTarget = await navigateNewTab(endpoint, siteURL, urlPattern);
      if (newTarget) {
        const client = createCDPClient(newTarget.webSocketDebuggerUrl);
        await injectCookiesFromProbe(client, siteURL);
        return { client, target: newTarget, launched: false };
      }
    } catch {}
  }

  if (!allowLaunch) {
    throw new Error(
      `未找到包含 ${siteURL} 的 CDP 浏览器。请先带 --remote-debugging-port=9222 参数启动 Chrome/Edge。`
    );
  }

  // 步骤 3: 启动新的浏览器进程
  const result = await launchCDPBrowser({ siteURL, port: 9222 });
  endpoint = result.endpoint;
  launched = true;

  // 步骤 4: 等待目标页面就绪
  const deadline = Date.now() + 15000;
  while (Date.now() < deadline) {
    const target = await findCDPTarget(endpoint, urlPattern).catch(() => null);
    if (target) {
      const client = createCDPClient(target.webSocketDebuggerUrl);
      await injectCookiesFromProbe(client, siteURL);
      return { client, target, launched: true };
    }
    await new Promise((r) => setTimeout(r, 500));
  }

  throw new Error(
    `浏览器已拉起但未找到 ${siteURL} 页面。可能需要先登录该站点的浏览器 Profile。`
  );
}

/**
 * 通过 DevTools /json/new HTTP 接口在现有浏览器实例中新开一个标签页并导航到目标 URL
 *
 * @param {string} endpoint - CDP Base URL
 * @param {string} url - 目标导航 URL
 * @param {RegExp} urlPattern - 匹配正则
 * @returns {Promise<object|null>} 匹配到的 Target
 */
async function navigateNewTab(endpoint, url, urlPattern) {
  try {
    const result = await fetchJSON(
      endpoint + "/json/new?" + encodeURIComponent(url),
      10000
    );
    // 等待导航加载
    await new Promise((r) => setTimeout(r, 3000));
    return await findCDPTarget(endpoint, urlPattern);
  } catch {
    return null;
  }
}

// ---------------------------------------------------------------------------
// 认证探测自动刷新工具
// ---------------------------------------------------------------------------

/**
 * 当收到 401/403 认证错误时，尝试通过 `assetiweave-cli conversation web auth-detect` 自动重新探测刷新凭据 Cookie
 *
 * @param {string} harvesterDir - Harvester 所在的绝对路径目录
 * @param {string} domain - 目标 Cookie 域名 (如 "chatgpt.com")
 * @param {object} [extraFlags={}] - 额外 CLI 参数
 * @returns {boolean} 刷新是否成功
 */
function tryRefreshAuth(harvesterDir, domain, extraFlags = {}) {
  try {
    const args = [
      "conversation", "web", "auth-detect",
      harvesterDir,
      "--domain", domain,
      "--credential", "cookie",
    ];
    if (extraFlags.probeURL) {
      args.push("--probe-url", extraFlags.probeURL);
    }
    execSync("assetiweave-cli " + args.map(shellQuote).join(" "), {
      encoding: "utf8",
      timeout: 30000,
      stdio: "pipe",
    });
    return true;
  } catch (error) {
    process.stderr.write(
      `[cdp-browser] auth-detect 凭据刷新失败: ${error.message || error}\n`
    );
    return false;
  }
}

/**
 * 转义 Shell 命令行参数以防止脚本注入
 *
 * @param {string} s - 待转义的参数
 * @returns {string} 安全转义后的字符串
 */
function shellQuote(s) {
  if (/^[a-zA-Z0-9._/:-]+$/.test(s)) return s;
  return "'" + s.replace(/'/g, "'\\''") + "'";
}

// ---------------------------------------------------------------------------
// 模块导出接口
// ---------------------------------------------------------------------------

module.exports = {
  createCDPClient,
  fetchJSON,
  discoverCDPEndpoint,
  findCDPTarget,
  launchCDPBrowser,
  acquireCDPTarget,
  tryRefreshAuth,
  scanRunningBrowserPorts,
  findBrowserExecutable,
};
