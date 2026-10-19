import { copyFileSync, linkSync, mkdirSync, rmSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const exe = process.platform === "win32" ? ".exe" : "";
const output = process.env.ASSETIWEAVE_CLI_OUTPUT
  ? resolve(root, process.env.ASSETIWEAVE_CLI_OUTPUT)
  : join(root, "target", "debug", `assetiweave-cli${exe}`);

mkdirSync(dirname(output), { recursive: true });

const profile =
  process.env.CARGO_PROFILE === "release" ||
  process.env.NODE_ENV === "production"
    ? "release"
    : "debug";
const cargoArgs = ["build", "-p", "cli", "--bin", "aiwc"];
if (profile === "release") {
  cargoArgs.push("--release");
}

const result = spawnSync("cargo", cargoArgs, {
  cwd: root,
  stdio: "inherit",
  env: process.env,
});
if (result.error) {
  console.error(`cargo build failed: ${result.error.message}`);
  process.exit(1);
}
if (result.status !== 0) {
  process.exit(result.status ?? 1);
}

const builtAiwc = join(root, "target", profile, `aiwc${exe}`);
if (output !== builtAiwc) {
  rmSync(output, { force: true });
  try {
    linkSync(builtAiwc, output);
  } catch {
    copyFileSync(builtAiwc, output);
  }
}

const aliasOutput = join(dirname(output), `aiwc${exe}`);
if (aliasOutput !== output && aliasOutput !== builtAiwc) {
  rmSync(aliasOutput, { force: true });
  try {
    linkSync(builtAiwc, aliasOutput);
  } catch {
    copyFileSync(builtAiwc, aliasOutput);
  }
}
