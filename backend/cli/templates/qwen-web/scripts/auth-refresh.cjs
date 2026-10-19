/**
 * @file auth-refresh.cjs — Qwen Web 自动身份验证刷新模块
 *
 * 职责：当 Qwen 会话抓取由于 Token 或 Cookie 过期返回 401/403 错误时，
 * 调用 CLI 命令自动尝试侦测与刷新本地系统的 Browser Cookie。
 */
"use strict";

const childProcess = require("child_process");

/**
 * 尝试使用 assetiweave-cli auth-detect 刷新指定域名的凭据
 *
 * @param {string} harvesterDir - Harvester 绝对路径目录
 * @param {string} domain - 域名 (如 "tongyi.aliyun.com" 或 "qwen.ai")
 * @param {object} [options={}] - 自定义 CLI 路径或执行函数
 * @returns {boolean} 是否刷新成功
 */
function tryRefreshAuth(harvesterDir, domain, options = {}) {
  const cliPath = options.cliPath || process.env.ASSETIWEAVE_CLI_PATH || "assetiweave-cli";
  const execFileSync = options.execFileSync || childProcess.execFileSync;
  const args = [
    "conversation", "web", "auth-detect", harvesterDir,
    "--domain", domain,
    "--credential", "cookie"
  ];
  if (options.probeURL) {
    args.push("--probe-url", options.probeURL);
  }
  try {
    execFileSync(cliPath, args, {
      encoding: "utf8",
      timeout: 30000,
      stdio: "pipe",
      shell: false
    });
    return true;
  } catch (error) {
    process.stderr.write(`[auth-refresh] auth-detect 刷新凭据失败: ${error.message || error}\n`);
    return false;
  }
}

module.exports = { tryRefreshAuth };
