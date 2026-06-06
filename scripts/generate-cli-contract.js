import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const exe = process.platform === "win32" ? ".exe" : "";
const engine =
  process.env.ASSETIWEAVE_ENGINE ??
  join(root, "target", "debug", `assetiweave-engine${exe}`);
const output = join(root, "cli", "internal", "schema", "contract.json");

if (!existsSync(engine)) {
  console.error(`assetiweave-engine not found at ${engine}; run pnpm engine:build first`);
  process.exit(1);
}

function callEngine(request) {
  const result = spawnSync(engine, [], {
    cwd: root,
    encoding: "utf8",
    input: JSON.stringify(request),
  });
  if (result.error) {
    throw new Error(`failed to run assetiweave-engine: ${result.error.message}`);
  }
  if (result.status !== 0) {
    throw new Error(result.stderr || `assetiweave-engine exited with ${result.status}`);
  }
  const envelope = JSON.parse(result.stdout);
  if (!envelope.ok || !envelope.data) {
    throw new Error(`Engine request failed: ${JSON.stringify(envelope.error ?? envelope)}`);
  }
  return envelope.data;
}

let contract;
try {
  const version = callEngine({ id: "version", method: "system.version", params: {} });
  contract = callEngine({
    id: "contract",
    method: "schema.list",
    params: {},
    protocol_version: version.protocol_version,
    contract_version: version.contract_version,
  });
} catch (error) {
  console.error(`failed to read command contract: ${error.message}`);
  process.exit(1);
}

mkdirSync(dirname(output), { recursive: true });
writeFileSync(output, `${JSON.stringify(contract, null, 2)}\n`);
console.log(`wrote ${output}`);
