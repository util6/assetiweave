/**
 * cdp-browser.cjs — Shared CDP WebSocket client and browser auto-discovery.
 *
 * Responsibilities:
 *   1. CDP WebSocket client (createCDPClient)
 *   2. Multi-port endpoint discovery (discoverCDPEndpoint)
 *   3. Target page matching by URL pattern (findCDPTarget)
 *   4. Auto-launch headless browser with debug port (launchCDPBrowser)
 *   5. High-level acquire helper (acquireCDPTarget)
 *
 * Usage from harvest.js:
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
// CDP WebSocket client
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// HTTP helper for DevTools JSON endpoints
// ---------------------------------------------------------------------------

async function fetchJSON(url, timeoutMs = 3000) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const response = await fetch(url, { signal: controller.signal });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    return await response.json();
  } finally {
    clearTimeout(timer);
  }
}

// ---------------------------------------------------------------------------
// CDP endpoint discovery
// ---------------------------------------------------------------------------

const DEFAULT_PORTS = [9222, 9333, 9229];

/**
 * Try to discover an active CDP endpoint by probing known ports.
 * Returns the first endpoint URL that responds, or null.
 */
async function discoverCDPEndpoint(options = {}) {
  // 1. Explicit override from environment
  const envEndpoint = options.endpointEnv ? process.env[options.endpointEnv] : null;
  if (envEndpoint) {
    try {
      await fetchJSON(envEndpoint.replace(/\/$/, "") + "/json/version");
      return envEndpoint.replace(/\/$/, "");
    } catch {}
  }

  // 2. Probe known ports
  const ports = options.ports || DEFAULT_PORTS;
  for (const port of ports) {
    const base = `http://127.0.0.1:${port}`;
    try {
      await fetchJSON(base + "/json/version");
      return base;
    } catch {}
  }

  // 3. Scan for running browsers with debug port (macOS/Linux)
  if (os.platform() === "darwin" || os.platform() === "linux") {
    try {
      const scanned = scanRunningBrowserPorts();
      for (const port of scanned) {
        if (ports.includes(port)) continue; // already tried
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
 * Use `ps` to find running Chromium-based browser processes with
 * --remote-debugging-port and extract their port numbers.
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
// Target page discovery
// ---------------------------------------------------------------------------

/**
 * Find a CDP target (browser tab) matching the given URL pattern.
 * @param {string} endpoint - CDP endpoint base URL
 * @param {RegExp} urlPattern - Pattern to match against target URL
 * @returns {object|null} - Target object with webSocketDebuggerUrl, or null
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
// Browser auto-launch
// ---------------------------------------------------------------------------

const BROWSER_PATHS_MACOS = [
  "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
  "/Applications/Chromium.app/Contents/MacOS/Chromium",
];

const BROWSER_PATHS_LINUX = [
  "/usr/bin/microsoft-edge",
  "/usr/bin/microsoft-edge-stable",
  "/usr/bin/google-chrome",
  "/usr/bin/google-chrome-stable",
  "/usr/bin/brave-browser",
  "/usr/bin/chromium-browser",
  "/usr/bin/chromium",
];

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
 * Launch a headless browser with --remote-debugging-port.
 * Uses the user's real browser profile so login sessions are preserved.
 *
 * @param {object} options
 * @param {string} options.siteURL - URL to navigate to after launch
 * @param {number} [options.port=9222] - Debug port
 * @param {number} [options.waitMs=5000] - Time to wait for browser to start
 * @returns {object} { endpoint, process }
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
      process.stderr.write(`[cdp-browser] warning: failed to override user-agent: ${uaErr.message || uaErr}\n`);
    }
    for (const cookie of cookies) {
      // 避免个别特殊 cookie 校验失败导致全部失败
      try {
        await client.send("Network.setCookie", cookie);
      } catch (cookieErr) {
        process.stderr.write(`[cdp-browser] warning: failed to set cookie ${cookie.name}: ${cookieErr.message || cookieErr}\n`);
      }
    }
    process.stderr.write(`[cdp-browser] successfully injected cookies into browser context for ${hostname}\n`);
  } catch (err) {
    process.stderr.write(`[cdp-browser] failed to inject cookies: ${err.message || err}\n`);
  }
}

/**
 * Acquire a connected CDP client for a specific web page.
 *
 * Strategy:
 *   1. Discover an existing CDP endpoint (multi-port probe + ps scan)
 *   2. If found, look for a matching target tab
 *   3. If no endpoint or no matching tab, launch a headless browser
 *   4. Navigate to siteURL and wait for the page to be ready
 *   5. Return { client, target, launched }
 *
 * @param {object} options
 * @param {RegExp} options.urlPattern - Pattern to match target tab URL
 * @param {string} options.siteURL - URL to navigate to if launching
 * @param {string} [options.endpointEnv] - Env var for explicit endpoint override
 * @param {boolean} [options.allowLaunch=true] - Allow auto-launching a browser
 * @returns {{ client, target, launched }}
 */
async function acquireCDPTarget(options) {
  const { urlPattern, siteURL, endpointEnv, allowLaunch = true } = options;

  // Step 1: Try to discover existing endpoint
  let endpoint = await discoverCDPEndpoint({ endpointEnv });
  let launched = false;

  if (endpoint) {
    // Step 2: Look for matching target
    const target = await findCDPTarget(endpoint, urlPattern);
    if (target) {
      const client = createCDPClient(target.webSocketDebuggerUrl);
      await injectCookiesFromProbe(client, siteURL);
      return { client, target, launched: false };
    }
    // Endpoint exists but no matching tab — try to open the URL in existing browser
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
      `No CDP browser with ${siteURL} found. Start Chrome/Edge with ` +
      `--remote-debugging-port=9222 and open ${siteURL}.`
    );
  }

  // Step 3: Launch a new browser
  const result = await launchCDPBrowser({ siteURL, port: 9222 });
  endpoint = result.endpoint;
  launched = true;

  // Step 4: Wait for target page to appear
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
    `Browser launched but ${siteURL} page was not found. ` +
    `The harvester browser profile may need to be logged in first.`
  );
}

/**
 * Open a new tab in an existing browser via CDP.
 */
async function navigateNewTab(endpoint, url, urlPattern) {
  try {
    const result = await fetchJSON(
      endpoint + "/json/new?" + encodeURIComponent(url),
      10000
    );
    // Wait a moment for navigation
    await new Promise((r) => setTimeout(r, 3000));
    return await findCDPTarget(endpoint, urlPattern);
  } catch {
    return null;
  }
}

// ---------------------------------------------------------------------------
// Auth-detect auto-refresh helper
// ---------------------------------------------------------------------------

/**
 * Attempt to refresh auth credentials by invoking assetiweave-cli auth-detect.
 * Returns true if the command succeeded.
 *
 * @param {string} harvesterDir - Absolute path to harvester directory
 * @param {string} domain - Cookie domain (e.g., "chatgpt.com", "google.com")
 * @param {object} [extraFlags] - Additional CLI flags
 * @returns {boolean}
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
      `[cdp-browser] auth-detect refresh failed: ${error.message || error}\n`
    );
    return false;
  }
}

function shellQuote(s) {
  if (/^[a-zA-Z0-9._/:-]+$/.test(s)) return s;
  return "'" + s.replace(/'/g, "'\\''") + "'";
}

// ---------------------------------------------------------------------------
// Exports
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
