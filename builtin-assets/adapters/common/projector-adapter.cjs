#!/usr/bin/env node

const fs = require("node:fs");
const { projectCommandParts } = require("./shell-projector.cjs");

function write(value) {
  process.stdout.write(`${JSON.stringify(value)}\n`);
}

try {
  const input = JSON.parse(fs.readFileSync(0, "utf8"));
  if (input?.method !== "project_command_parts") {
    throw new Error(`unsupported method: ${input?.method ?? "missing"}`);
  }
  const projections = projectCommandParts(input?.params?.parts);
  for (const projection of projections) {
    write({
      type: "item",
      item: { kind: "command_projection", ...projection },
    });
  }
  write({
    type: "complete",
    item: {
      projection_count: projections.length,
      projector_version: projections[0]?.projector_version ?? "shell-projector-v1",
    },
  });
} catch (error) {
  write({
    type: "error",
    message: error instanceof Error ? error.message : String(error),
  });
  write({ type: "complete", item: {} });
}
