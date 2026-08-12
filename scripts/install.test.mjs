import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, lstatSync, mkdtempSync, readlinkSync, rmSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { tmpdir } from "node:os";

const root = path.resolve(import.meta.dirname, "..");
const installScript = path.join(root, "scripts", "install.js");
const executableName = (name) => process.platform === "win32" ? `${name}.exe` : name;

test("cli installer refreshes the canonical CLI, aiwc alias, and Engine used by the upgrade chain", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-cli-install-"));
  const installDir = path.join(fixtureRoot, "bin");
  const appHome = path.join(fixtureRoot, "home");
  const pathEnv = [installDir, process.env.PATH].filter(Boolean).join(path.delimiter);
  try {
    const install = spawnSync(process.execPath, [installScript], {
      cwd: root,
      encoding: "utf8",
      env: {
        ...process.env,
        ASSETIWEAVE_INSTALL_DIR: installDir,
        ASSETIWEAVE_INSTALL_SKIP_BUILD: "1",
      },
    });
    assert.equal(install.status, 0, `${install.stdout}\n${install.stderr}`);

    const cliPath = path.join(installDir, executableName("assetiweave-cli"));
    const enginePath = path.join(installDir, executableName("assetiweave-engine"));
    const aliasPath = process.platform === "win32"
      ? path.join(installDir, "aiwc.cmd")
      : path.join(installDir, "aiwc");
    assert.ok(existsSync(cliPath));
    assert.ok(existsSync(enginePath));
    assert.ok(existsSync(aliasPath));
    if (process.platform !== "win32") {
      assert.equal(lstatSync(aliasPath).isSymbolicLink(), true);
      assert.equal(readlinkSync(aliasPath), "assetiweave-cli");
    }

    const command = spawnSync(process.platform === "win32" ? "aiwc.cmd" : "aiwc", [
      "c",
      "ad",
      "upgrade",
      "-d",
      "--dry-run",
    ], {
      cwd: root,
      encoding: "utf8",
      env: {
        ...process.env,
        ASSETIWEAVE_HOME: appHome,
        ASSETIWEAVE_DB_PATH: path.join(fixtureRoot, "app.db"),
        PATH: pathEnv,
      },
    });
    assert.equal(command.status, 0, `${command.stdout}\n${command.stderr}`);
    const response = JSON.parse(command.stdout);
    assert.equal(response.ok, true);
    assert.equal(response.meta.invocation.method, "conversation.adapter_package.upgrade_workspace");
    const antigravity = response.data.upgraded.find((item) => item.adapter_id === "antigravity");
    assert.equal(antigravity.version, "1.3.7");
    assert.equal(antigravity.dry_run, true);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});
