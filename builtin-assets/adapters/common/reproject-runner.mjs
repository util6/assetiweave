#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { reprojectStoredSession } from "./payload-policy.mjs";

function main() {
  const raw = readFileSync(0, "utf8");
  const trimmed = raw.trim();
  if (!trimmed) {
    process.stdout.write(JSON.stringify({ sessions: [] }) + "\n");
    return;
  }
  const input = JSON.parse(trimmed);
  if (Array.isArray(input.sessions)) {
    const sessions = input.sessions.map((session) =>
      reprojectStoredSession(session, {
        adapterId: session?.adapter_id,
        workspacePath: session?.workspace_path,
      }),
    );
    process.stdout.write(JSON.stringify({ sessions }) + "\n");
    return;
  }
  const session = reprojectStoredSession(input, {
    adapterId: input?.adapter_id,
    workspacePath: input?.workspace_path,
  });
  process.stdout.write(JSON.stringify({ session }) + "\n");
}

main();
