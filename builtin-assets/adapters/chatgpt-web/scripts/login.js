#!/usr/bin/env node
/**
 * @file login.js — ChatGPT Web 交互式登录引导脚本
 *
 * 功能：
 * 1. 以有头界面（普通浏览器窗口）拉起隔离的 Profile 浏览器 (~/.assetiweave/browser-profile)
 * 2. 自动打开 https://chatgpt.com 供用户完成登录
 * 3. 实时轮询检测登录态（检测 /api/auth/session 的 accessToken）
 * 4. 登录完成后自动提取并持久化 Cookie 到 requests/auth-probe.json，并关闭登录窗口
 */
const path = require("path");
const { acquireCDPTarget, closeCDPTarget, saveCookiesToProbe, safeEvaluate } = require("./cdp-browser.cjs");

const siteURL = "https://chatgpt.com";
const root = process.env.ASSETIWEAVE_HARVESTER_DIR || path.resolve(__dirname, "..");

async function main() {
  console.log("=======================================================");
  console.log(" [ChatGPT Web] 交互式登录助手");
  console.log(" 正在拉起浏览器窗口...");
  console.log(" 请在弹出的浏览器窗口中登录您的 ChatGPT 账号。");
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
      urlPattern: /^https:\/\/(?:[a-zA-Z0-9-]+\.)*chatgpt\.com(?:\/|$|\?)/,
      siteURL,
      allowLaunch: true,
      headless: false,
    });

    const { client } = cdpHandle;
    console.log("浏览器窗口已就绪，正在等待登录完成...");

    const checkExpression = String.raw`(async () => {
      try {
        const resp = await fetch("/api/auth/session", { credentials: "include" });
        if (!resp.ok) return { loggedIn: false, status: resp.status };
        const data = await resp.json();
        if (data && typeof data.accessToken === "string" && data.accessToken.length > 10) {
          return { loggedIn: true, user: (data.user && data.user.email) || "ChatGPT User" };
        }
        return { loggedIn: false };
      } catch (e) {
        return { loggedIn: false, error: e.message };
      }
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
        if (val && val.loggedIn) {
          console.log(`\n✓ 检测到登录成功！用户: ${val.user}`);
          const saved = await saveCookiesToProbe(client, siteURL);
          if (saved) {
            console.log("✓ 凭据已成功保存至 requests/auth-probe.json！");
            console.log("✓ 浏览器 Profile 已持久化至 ~/.assetiweave/browser-profile\n");
            console.log("ChatGPT Web 网页会话同步已就绪，可随时进行记录采集。");
          } else {
            console.warn("! 登录成功但 Cookie 持久化异常，请检查 requests/ 目录权限。");
          }
          await new Promise((r) => setTimeout(r, 1500));
          return;
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
  console.error(`\n[ChatGPT Web] 登录失败: ${err.message || err}`);
  process.exit(1);
});
