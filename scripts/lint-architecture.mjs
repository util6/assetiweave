#!/usr/bin/env node
import { spawnSync } from "node:child_process";

const result = spawnSync("pnpm", ["exec", "eslint", "frontend/src"], {
  stdio: "inherit",
  env: {
    ...process.env,
    ESLINT_ARCHITECTURE_ONLY: "1",
  },
});

process.exit(result.status ?? 1);
