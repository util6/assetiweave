import { spawnSync } from "node:child_process";
import {
  chmodSync,
  copyFileSync,
  existsSync,
  lstatSync,
  mkdirSync,
  renameSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: root,
    stdio: "inherit",
    env: {
      ...process.env,
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

const exe = process.platform === "win32" ? ".exe" : "";
const installDir = resolveInstallDir();
const cliName = `assetiweave-cli${exe}`;
const engineName = `assetiweave-engine${exe}`;
const cliSource = join(root, "target", "debug", cliName);
const engineSource = join(root, "target", "debug", engineName);

if (process.env.ASSETIWEAVE_INSTALL_SKIP_BUILD !== "1") {
  run("cargo", ["build", "-p", "assetiweave", "--bin", "assetiweave-engine"]);
  run(process.execPath, [join(root, "scripts", "build-cli.js")]);
}

if (!existsSync(cliSource) || !existsSync(engineSource)) {
  throw new Error(`built CLI or Engine is missing under ${join(root, "target", "debug")}`);
}

mkdirSync(installDir, { recursive: true });
installExecutable(cliSource, join(installDir, cliName));
installExecutable(engineSource, join(installDir, engineName));
if (process.platform === "win32") {
  installTextFile(join(installDir, "aiwc.cmd"), `@echo off\r\n"%~dp0${cliName}" %*\r\n`);
} else {
  installSymlink(cliName, join(installDir, "aiwc"));
}

console.log(`Installed ${cliName}, aiwc, and ${engineName} to ${installDir}`);

function resolveInstallDir() {
  const override = process.env.ASSETIWEAVE_INSTALL_DIR?.trim();
  if (override) {
    return resolve(override.startsWith("~/") ? join(homedir(), override.slice(2)) : override);
  }
  if (process.platform === "win32") {
    const localAppData = process.env.LOCALAPPDATA?.trim();
    return join(localAppData || homedir(), "AssetIWeave", "bin");
  }
  return join(homedir(), ".local", "bin");
}

function installExecutable(source, target) {
  const temporary = `${target}.tmp-${process.pid}`;
  removeInstallTarget(temporary);
  copyFileSync(source, temporary);
  if (process.platform !== "win32") chmodSync(temporary, 0o755);
  replaceInstallTarget(temporary, target);
}

function installTextFile(target, contents) {
  const temporary = `${target}.tmp-${process.pid}`;
  removeInstallTarget(temporary);
  writeFileSync(temporary, contents, { mode: 0o755 });
  replaceInstallTarget(temporary, target);
}

function installSymlink(targetName, target) {
  const temporary = `${target}.tmp-${process.pid}`;
  removeInstallTarget(temporary);
  symlinkSync(targetName, temporary);
  replaceInstallTarget(temporary, target);
}

function replaceInstallTarget(temporary, target) {
  removeInstallTarget(target);
  renameSync(temporary, target);
}

function removeInstallTarget(target) {
  if (!existsSync(target) && !isSymlink(target)) return;
  if (lstatSync(target).isDirectory()) {
    throw new Error(`refusing to replace directory: ${target}`);
  }
  rmSync(target, { force: true });
}

function isSymlink(target) {
  try {
    return lstatSync(target).isSymbolicLink();
  } catch {
    return false;
  }
}
