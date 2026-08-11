"use strict";

const childProcess = require("child_process");

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
    process.stderr.write(`[auth-refresh] auth-detect failed: ${error.message || error}\n`);
    return false;
  }
}

module.exports = { tryRefreshAuth };
