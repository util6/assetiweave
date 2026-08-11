import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";

const repositoryRoot = path.resolve(import.meta.dirname, "..");
const adapterPath = path.join(repositoryRoot, "builtin-assets/adapters/opencode/adapter.mjs");

test("OpenCode stores message summary diffs once in the existing patch Part", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-opencode-diff-"));
  try {
    const dbPath = path.join(fixtureRoot, "opencode.db");
    const summary = JSON.stringify({
      role: "user",
      time: { created: "2026-08-06T00:00:00Z" },
      summary: {
        diffs: [{
          file: "src/main.ts",
          additions: 1,
          deletions: 1,
          status: "modified",
          patch: "@@ -1 +1 @@\n-old\n+new",
        }],
      },
    });
    runSqlite(dbPath, [
      "CREATE TABLE session (id TEXT, title TEXT, project TEXT, updated_at TEXT);",
      "CREATE TABLE message (id TEXT, session_id TEXT, data TEXT);",
      "CREATE TABLE part (id TEXT, message_id TEXT, session_id TEXT, data TEXT);",
      "INSERT INTO session VALUES ('session-1', 'Fixture', '/tmp/project', '2026-08-06T00:00:02Z');",
      `INSERT INTO message VALUES ('message-user', 'session-1', '${sqlString(summary)}');`,
      `INSERT INTO part VALUES ('part-user', 'message-user', 'session-1', '${sqlString(JSON.stringify({ type: "text", text: "修改入口文件" }))}');`,
      `INSERT INTO message VALUES ('message-assistant', 'session-1', '${sqlString(JSON.stringify({ role: "assistant", time: { created: "2026-08-06T00:00:01Z" } }))}');`,
      `INSERT INTO part VALUES ('part-todo', 'message-assistant', 'session-1', '${sqlString(JSON.stringify({ type: "tool", tool: "todowrite", callID: "todo-1", state: { status: "completed", output: "更新任务列表", title: "1 todos" } }))}');`,
      `INSERT INTO part VALUES ('part-patch', 'message-assistant', 'session-1', '${sqlString(JSON.stringify({ type: "patch", hash: "patch-hash", files: ["src/main.ts"] }))}');`,
    ].join("\n"));

    const session = readFixtureSession(dbPath);
    const parts = session.turns[0].parts;
    assert.equal(parts.length, 2);
    assert.equal(parts[0].kind, "tool");
    assert.equal(parts[0].content_card.renderer, "terminal_output");
    assert.equal(JSON.parse(parts[0].metadata_json).execution_kind, "unclassified");
    assert.equal(parts[1].kind, "file_change");
    assert.equal(parts[1].content_card.kind, "opencode.file-change");
    assert.equal(parts[1].content_card.renderer, "diff");
    assert.match(parts[1].text, /^diff --git a\/src\/main\.ts b\/src\/main\.ts/m);
    assert.match(parts[1].text, /@@ -1 \+1 @@\n-old\n\+new/);
    assert.equal(parts.filter((part) => part.kind === "file_change").length, 1);
    assert.equal(parts.filter((part) => part.text?.includes("diff --git")).length, 1);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

function readFixtureSession(dbPath) {
  const result = spawnSync(process.execPath, [adapterPath], {
    encoding: "utf8",
    input: JSON.stringify({ method: "read_session", source: { location: dbPath }, params: {} }),
  });
  assert.equal(result.status, 0, result.stderr);
  const messages = result.stdout.trim().split("\n").map((line) => JSON.parse(line));
  const session = messages.find((entry) => entry.type === "item")?.item?.session;
  assert.ok(session);
  return session;
}

function runSqlite(dbPath, sql) {
  const result = spawnSync("sqlite3", [dbPath], { encoding: "utf8", input: sql });
  assert.equal(result.status, 0, result.stderr);
}

function sqlString(value) {
  return value.replaceAll("'", "''");
}
