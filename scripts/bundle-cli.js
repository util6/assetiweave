import { copyFileSync, mkdirSync, readdirSync, rmSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const exe = process.platform === "win32" ? ".exe" : "";
const bundleDir = join(root, "src-tauri", "bundled-cli", "cli");
const cliOutput = join(bundleDir, `assetiweave-cli${exe}`);
const engineOutput = join(bundleDir, `assetiweave-engine${exe}`);
const releaseEngine = join(root, "target", "release", `assetiweave-engine${exe}`);

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? root,
    stdio: "inherit",
    env: {
      ...process.env,
      GOCACHE: join(root, "target", "go-build"),
      ...options.env,
    },
  });
  if (result.error) {
    console.error(`${command} failed: ${result.error.message}`);
    process.exit(1);
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

mkdirSync(bundleDir, { recursive: true });
for (const entry of readdirSync(bundleDir)) {
  if (entry !== ".gitkeep") {
    rmSync(join(bundleDir, entry), { recursive: true, force: true });
  }
}

run("cargo", ["build", "--release", "-p", "assetiweave", "--bin", "assetiweave-engine"]);
run("node", ["scripts/build-cli.js"], {
  env: {
    ASSETIWEAVE_CLI_OUTPUT: cliOutput,
  },
});
copyFileSync(releaseEngine, engineOutput);
