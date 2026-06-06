import { mkdirSync, readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const cliRoot = join(root, "cli");
const exe = process.platform === "win32" ? ".exe" : "";
const packageJSON = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
const output =
  process.env.ASSETIWEAVE_CLI_OUTPUT ??
  join(root, "target", "debug", `assetiweave-cli${exe}`);

mkdirSync(dirname(output), { recursive: true });

function capture(command, args) {
  const result = spawnSync(command, args, {
    cwd: root,
    encoding: "utf8",
  });
  if (result.status !== 0 || result.error) {
    return "unknown";
  }
  return result.stdout.trim() || "unknown";
}

const buildCommit = capture("git", ["rev-parse", "--short=12", "HEAD"]);
const builtAt = process.env.SOURCE_DATE_EPOCH
  ? new Date(Number(process.env.SOURCE_DATE_EPOCH) * 1000).toISOString()
  : new Date().toISOString();
const ldflags = [
  `-X github.com/util6/assetiweave/internal/protocol.CLIVersion=${packageJSON.version}`,
  `-X github.com/util6/assetiweave/internal/protocol.CLIBuildCommit=${buildCommit}`,
  `-X github.com/util6/assetiweave/internal/protocol.CLIBuiltAt=${builtAt}`,
  `-X github.com/util6/assetiweave/internal/protocol.CLIBuildSource=script`,
].join(" ");

const result = spawnSync(
  "go",
  [
    "build",
    "-ldflags",
    ldflags,
    "-o",
    output,
    ".",
  ],
  {
    cwd: cliRoot,
    stdio: "inherit",
    env: {
      ...process.env,
      GOCACHE: join(root, "target", "go-build"),
    },
  },
);

if (result.error) {
  console.error(`go build failed: ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status ?? 1);
