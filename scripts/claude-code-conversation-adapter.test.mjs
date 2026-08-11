import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";

const repositoryRoot = path.resolve(import.meta.dirname, "..");
const adapterPath = path.join(repositoryRoot, "builtin-assets/adapters/claude-code/adapter.mjs");

test("Claude Code normalizes a Read execution in-place and removes its successful body", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-claude-read-"));
  try {
    const sessionPath = path.join(fixtureRoot, "session-read.jsonl");
    writeFileSync(sessionPath, [
      message("2026-08-05T00:00:00Z", "user", [{ type: "text", text: "读取入口文件" }]),
      message("2026-08-05T00:00:01Z", "assistant", [{
        type: "tool_use",
        id: "tool-read",
        name: "Read",
        input: { file_path: "/tmp/project/src/main.ts", offset: 10, limit: 20 },
      }]),
      message("2026-08-05T00:00:02Z", "user", [{
        type: "tool_result",
        tool_use_id: "tool-read",
        content: "10→const value = true;",
      }], { toolUseResult: { type: "text", file: { filePath: "/tmp/project/src/main.ts" } } }),
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    const parts = session.turns[0].parts;
    assert.equal(parts.length, 2);
    assert.deepEqual(parts.map((part) => part.source_execution_id), ["tool-read", "tool-read"]);
    assert.equal(parts[0].kind, "command");
    assert.equal(parts[0].content_card.kind, "claude-code.command");
    assert.equal(parts[0].command, "/tmp/project/src/main.ts:10-29");
    assert.equal(parts[0].status, "completed");
    assert.equal(parts[0].exit_code, 0);
    assert.equal(parts[1].text, null);
    assert.equal(JSON.parse(parts[1].metadata_json).execution_kind, "read");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Claude Code stores one canonical diff in the original Edit result Part", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-claude-edit-"));
  try {
    const sessionPath = path.join(fixtureRoot, "session-edit.jsonl");
    writeFileSync(sessionPath, [
      message("2026-08-05T00:00:00Z", "user", [{ type: "text", text: "修改入口文件" }]),
      message("2026-08-05T00:00:01Z", "assistant", [{
        type: "tool_use",
        id: "tool-edit",
        name: "Edit",
        input: { file_path: "/tmp/project/src/main.ts", old_string: "old", new_string: "new" },
      }]),
      message("2026-08-05T00:00:02Z", "user", [{
        type: "tool_result",
        tool_use_id: "tool-edit",
        content: "The file /tmp/project/src/main.ts has been updated successfully.",
      }], {
        toolUseResult: {
          filePath: "/tmp/project/src/main.ts",
          oldString: "old",
          newString: "new",
          structuredPatch: [{
            oldStart: 4,
            oldLines: 1,
            newStart: 4,
            newLines: 1,
            lines: ["-old", "+new"],
          }],
        },
      }),
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    const parts = session.turns[0].parts;
    assert.equal(parts.length, 2);
    assert.deepEqual(parts.map((part) => part.source_execution_id), ["tool-edit", "tool-edit"]);
    assert.equal(parts[1].kind, "file_change");
    assert.equal(parts[1].content_card.kind, "claude-code.file-change");
    assert.equal(parts[1].content_card.renderer, "diff");
    assert.match(parts[1].text, /^diff --git a\/tmp\/project\/src\/main\.ts b\/tmp\/project\/src\/main\.ts/m);
    assert.match(parts[1].text, /@@ -4,1 \+4,1 @@\n-old\n\+new/);
    assert.equal(parts[1].text.includes("updated successfully"), false);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

function message(timestamp, role, content, extra = {}) {
  return JSON.stringify({ timestamp, type: role, message: { role, content }, ...extra });
}

function readFixtureSession(fixtureRoot) {
  const result = spawnSync(process.execPath, [adapterPath], {
    encoding: "utf8",
    input: JSON.stringify({ method: "read_session", source: { location: fixtureRoot }, params: {} }),
  });
  assert.equal(result.status, 0, result.stderr);
  const messages = result.stdout.trim().split("\n").map((line) => JSON.parse(line));
  const session = messages.find((entry) => entry.type === "item")?.item?.session;
  assert.ok(session);
  return session;
}
