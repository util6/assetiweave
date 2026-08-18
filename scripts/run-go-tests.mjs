import { spawnSync } from "node:child_process";
import { mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const goCache = resolve(repositoryRoot, "target", "go-build");
mkdirSync(goCache, { recursive: true });

const result = spawnSync(
  "go",
  ["test", "-C", "cli", "./..."],
  {
    cwd: repositoryRoot,
    env: { ...process.env, GOCACHE: goCache },
    stdio: "inherit",
  },
);

if (result.error) {
  console.error(`failed to start go test: ${result.error.message}`);
  process.exit(1);
}

process.exit(result.status ?? 1);
