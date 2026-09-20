#!/usr/bin/env node
/**
 * @file login.js — Gemini Web 交互式登录引导脚本
 *
 * 功能：
 * 1. 以有头界面（普通浏览器窗口）拉起隔离的 Profile 浏览器 (~/.assetiweave/browser-profile)
 * 2. 自动打开 https://gemini.google.com/app 供用户完成登录
 * 3. 实时轮询检测登录态（检测 Cookie 中的 __Secure-1PSID / SAPISID 且处于 /app 页面）
 * 4. 登录完成后自动提取并持久化 Cookie 到 requests/auth-probe.json，并关闭登录窗口
 */
const path = require("path");
const { acquireCDPTarget, closeCDPTarget, saveCookiesToProbe, safeEvaluate } = require("./cdp-browser.cjs");

const siteURL = "https://gemini.google.com/app";
const root = process.env.ASSETIWEAVE_HARVESTER_DIR || path.resolve(__dirname, "..");

async function main() {
  console.log("=======================================================");
  console.log(" [Gemini Web] 交互式登录助手");
  console.log(" 正在拉起浏览器窗口...");
  console.log(" 请在弹出的浏览器窗口中登录您的 Google / Gemini 账号。");
  console.log(" 登录完成后将自动捕获凭据并保存。按 Ctrl+C 可取消。");
  console.log("=======================================================\n");

  let cdpHandle = null;
  const cleanup = async () => {
    if (cdpHandle) {
      console.log("\n正在关闭登录窗口...");
      await closeCDPTarget(cdpHandle);
      cdpHandle = null;
    }
  };

  process.on("SIGINT", async () => {
    await cleanup();
    process.exit(130);
  });
  process.on("SIGTERM", async () => {
    await cleanup();
    process.exit(143);
  });

  try {
    cdpHandle = await acquireCDPTarget({
      urlPattern: /^https:\/\/(?:[a-zA-Z0-9-]+\.)*(?:gemini\.google\.com|google\.com)(?:\/|$|\?)/,
      siteURL,
      allowLaunch: true,
      headless: false,
    });

    const { client } = cdpHandle;
    console.log("浏览器窗口已就绪，正在等待登录完成...");

    const checkExpression = String.raw`(() => {
      const url = window.location.href;
      const isApp = url.includes("gemini.google.com/app");
      const hasGeminiDomain = url.includes("gemini.google.com");
      return { url, isApp, hasGeminiDomain };
    })()`;

    const startTime = Date.now();
    const timeoutMs = 5 * 60 * 1000; // 5分钟等待登录超时

    while (Date.now() - startTime < timeoutMs) {
      try {
        const evaluated = await safeEvaluate(client, checkExpression, {
          maxRetries: 3,
          retryIntervalMs: 500,
          timeout: 5000,
        });
        const val = evaluated && evaluated.result && evaluated.result.value;

        if (val && val.hasGeminiDomain) {
          // 检查 Cookie 中是否已经包含核心会话标志
          let cookiesRes;
          try {
            cookiesRes = await client.send("Network.getCookies", { urls: [siteURL] });
          } catch {
            cookiesRes = await client.send("Storage.getCookies");
          }
          const cookies = cookiesRes && Array.isArray(cookiesRes.cookies) ? cookiesRes.cookies : [];
          const hasSessionCookie = cookies.some((c) =>
            c.name === "__Secure-1PSID" || c.name === "SAPISID" || c.name === "SSID"
          );

          if (hasSessionCookie && (val.isApp || cookies.length >= 3)) {
            console.log(`\n✓ 检测到 Gemini 账号已登录！`);
            const saved = await saveCookiesToProbe(client, siteURL);
            if (saved) {
              console.log("✓ 凭据已成功保存至 requests/auth-probe.json！");
              console.log("✓ 浏览器 Profile 已持久化至 ~/.assetiweave/browser-profile\n");
              console.log("Gemini Web 网页会话同步已就绪，可随时进行记录采集。");
            } else {
              console.warn("! 登录成功但 Cookie 持久化异常，请检查 requests/ 目录权限。");
            }
            await new Promise((r) => setTimeout(r, 1500));
            return;
          }
        }
      } catch {}

      process.stdout.write(".");
      await new Promise((r) => setTimeout(r, 2000));
    }

    throw new Error("登录超时 (5分钟内未检测到成功登录态)");
  } finally {
    await cleanup();
  }
}

main().catch((err) => {
  console.error(`\n[Gemini Web] 登录失败: ${err.message || err}`);
  process.exit(1);
});
