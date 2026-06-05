import { mkdirSync, readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const exe = process.platform === "win32" ? ".exe" : "";
const packageJSON = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
const output =
  process.env.ASSETIWEAVE_CLI_OUTPUT ??
  join(root, "target", "debug", `assetiweave-cli${exe}`);

mkdirSync(dirname(output), { recursive: true });

const result = spawnSync(
  "go",
  [
    "build",
    "-ldflags",
    `-X github.com/util6/assetiweave/internal/protocol.CLIVersion=${packageJSON.version}`,
    "-o",
    output,
    ".",
  ],
  {
    cwd: root,
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
