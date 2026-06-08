import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const cliRoot = join(root, "cli");
const staticOnly = process.argv.includes("--static-only");

function read(path) {
  return readFileSync(join(root, path), "utf8");
}

function readJSON(path) {
  return JSON.parse(read(path));
}

function fail(message) {
  console.error(`release audit failed: ${message}`);
  process.exit(1);
}

function requireEqual(label, actual, expected) {
  if (actual !== expected) {
    fail(`${label} = ${actual}, want ${expected}`);
  }
}

function requireIncludes(label, text, needle) {
  if (!text.includes(needle)) {
    fail(`${label} must include ${needle}`);
  }
}

function requireNotIncludes(label, text, needle) {
  if (text.includes(needle)) {
    fail(`${label} must not include ${needle}`);
  }
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? root,
    stdio: "inherit",
    env: {
      ...process.env,
      GOCACHE: join(root, "target", "go-build"),
    },
  });
  if (result.error) {
    fail(`${command} failed: ${result.error.message}`);
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

const packageJSON = readJSON("package.json");
const tauriConfig = readJSON("src-tauri/tauri.conf.json");
const contract = readJSON("cli/internal/schema/contract.json");
const cargoToml = read("src-tauri/Cargo.toml");
const cargoVersion = cargoToml.match(/^\[package\][\s\S]*?^version = "([^"]+)"/m)?.[1];

requireEqual("src-tauri/Cargo.toml version", cargoVersion, packageJSON.version);
requireEqual("src-tauri/tauri.conf.json version", tauriConfig.version, packageJSON.version);
requireEqual("cli/internal/schema/contract.json engine_version", contract.engine_version, packageJSON.version);

const ciWorkflow = read(".github/workflows/ci.yml");
requireIncludes("CI workflow", ciWorkflow, "go vet -C cli ./...");
requireIncludes("CI workflow", ciWorkflow, "go test -C cli -race ./...");
requireIncludes("CI workflow", ciWorkflow, "pnpm cli:test:e2e");

const releaseWorkflow = read(".github/workflows/release.yml");
requireIncludes("release workflow", releaseWorkflow, "node scripts/release-audit.js --static-only");
requireIncludes("release workflow", releaseWorkflow, "scripts/build-cli.js");
requireIncludes("release workflow", releaseWorkflow, ".sha256");
requireIncludes("release workflow", releaseWorkflow, "createHash('sha256')");
requireNotIncludes(
  "release workflow",
  releaseWorkflow,
  "go build -ldflags \"-X github.com/util6/assetiweave/internal/protocol.CLIVersion=",
);

const rootCommand = read("cli/cmd/root.go");
requireIncludes("root command", rootCommand, "setupNotices");
requireIncludes("root command", rootCommand, "composePendingNotice");
requireIncludes("root command", rootCommand, "SkipRuntime");
requireIncludes("root command", rootCommand, "isCompletionCommandArgs");
requireIncludes("root command", rootCommand, "ASSETIWEAVE_CLI_HIDE_PROFILES");
requireIncludes("root command", rootCommand, "HideProfiles");
requireIncludes("root command", rootCommand, "applyProfileVisibility");
requireIncludes("root command", rootCommand, "installCobraValidation");

const cobraErrors = read("cli/cmd/cobra_errors.go");
requireIncludes("Cobra validation", cobraErrors, "SubtypeUnknownCommand");
requireIncludes("Cobra validation", cobraErrors, "SubtypeUnknownFlag");
requireIncludes("Cobra validation", cobraErrors, "SubtypeMissingRequiredFlag");
requireIncludes("Cobra validation", cobraErrors, "MarkPureGroup");
requireIncludes("Cobra validation", cobraErrors, "suggest.Closest");

const cliE2E = read("cli/tests/cli_e2e/cli_e2e_test.go");
requireIncludes("CLI e2e", cliE2E, "TestRealCLIClassifiesCobraUsageErrors");
requireIncludes("CLI e2e", cliE2E, "TestRealCLISettingsShortcuts");

const settingsCommand = read("cli/cmd/settings.go");
requireIncludes("settings command", settingsCommand, "newCmdSettings");
requireIncludes("settings command", settingsCommand, "MethodSettingsGet");
requireIncludes("settings command", settingsCommand, "MethodSettingsSave");
requireIncludes("settings command", settingsCommand, "SubtypeInvalidJSON");
requireIncludes("CLI contract", JSON.stringify(contract), "assetiweave-cli settings show");
requireIncludes("CLI contract", JSON.stringify(contract), "assetiweave-cli settings save --json <json>");

const skillCommand = read("cli/cmd/skill.go");
requireIncludes("skill command", skillCommand, "newCmdSkillSearch");
requireIncludes("skill command", skillCommand, "newCmdSkillAcquire");
requireIncludes("skill group command", skillCommand, "newCmdSkillGroupCreate");
requireIncludes("skill group command", skillCommand, "newCmdSkillGroupMembersSet");
requireIncludes("skill group command", skillCommand, "newCmdSkillGroupExclusiveApply");
requireIncludes("CLI contract", JSON.stringify(contract), "assetiweave-cli skill search --query <query>");
requireIncludes("CLI contract", JSON.stringify(contract), "assetiweave-cli skill acquire --url <github-url> --yes");
requireIncludes("CLI contract", JSON.stringify(contract), "assetiweave-cli skill group create --name <name>");
requireIncludes("CLI contract", JSON.stringify(contract), "assetiweave-cli skill group members set <group-id> --asset <asset-id>");
requireIncludes("CLI contract", JSON.stringify(contract), "assetiweave-cli skill group exclusive apply --group <group-id> --profile <profile-id> --yes");

const outputContract = read("cli/internal/output/output.go");
requireIncludes("output envelope", outputContract, 'json:"_notice,omitempty"');
requireIncludes("output envelope", outputContract, "PendingNotice");

const updateChecker = read("cli/internal/update/check.go");
requireIncludes("update checker", updateChecker, "CheckCached");
requireIncludes("update checker", updateChecker, "RefreshCache");
requireIncludes("update checker", updateChecker, "ASSETIWEAVE_CLI_NO_UPDATE_NOTIFIER");

const selfUpdater = read("cli/internal/selfupdate/selfupdate.go");
requireIncludes("self updater", selfUpdater, "assetiweave-tools-");
requireIncludes("self updater", selfUpdater, "packageAsset");
requireIncludes("self updater", selfUpdater, "ChecksumURL");
requireIncludes("self updater", selfUpdater, "manual_required");
const selfUpdateApply = read("cli/internal/selfupdate/apply.go");
requireIncludes("self updater", selfUpdateApply, "verifySHA256");
requireIncludes("self updater", selfUpdateApply, "rollbackInstalled");
requireIncludes("self updater", read("cli/cmd/update.go"), "\"yes\"");

if (!staticOnly) {
  run("go", ["test", "./internal/errlint", "./internal/cmdlint", "./internal/schema", "./internal/update", "./internal/selfupdate", "./cmd", "-count=1"], { cwd: cliRoot });
}

console.log(staticOnly ? "release static audit passed" : "release audit passed");
