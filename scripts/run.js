import { existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const exe = process.platform === "win32" ? ".exe" : "";

function findBinary(name, envName, required = true) {
  const envPath = process.env[envName];
  const candidates = [
    envPath,
    join(root, "target", "release", `${name}${exe}`),
    join(root, "target", "debug", `${name}${exe}`),
  ].filter(Boolean);
  const found = candidates.find((candidate) => existsSync(candidate));
  if (!found && required) {
    console.error(`${name} not found. Run pnpm cli:install, or set ${envName}.`);
    process.exit(1);
  }
  return found;
}

const cli = findBinary("assetiweave-cli", "ASSETIWEAVE_CLI");
const engine = findBinary("assetiweave-engine", "ASSETIWEAVE_ENGINE", false);
const env = { ...process.env };
if (!env.ASSETIWEAVE_ENGINE && engine) {
  env.ASSETIWEAVE_ENGINE = engine;
}
const args = process.argv.slice(2);
if (args[0] === "--") {
  args.shift();
}

const result = spawnSync(cli, args, {
  cwd: process.cwd(),
  stdio: "inherit",
  env,
});
if (result.error) {
  console.error(`assetiweave-cli failed: ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status ?? 1);
