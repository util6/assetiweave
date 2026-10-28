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

test("OpenCode keeps an aggregated shell Part without persisted display projection", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-opencode-shell-projection-"));
  try {
    const dbPath = path.join(fixtureRoot, "opencode.db");
    const command = [
      "printf '%s\\n' '--- inspect ---'",
      "rg 'quoted && value' ./src | sed 's/;/|/'",
      "git status --short > /tmp/status.txt",
    ].join(" && ");
    runSqlite(dbPath, [
      "CREATE TABLE session (id TEXT, title TEXT, project TEXT, updated_at TEXT);",
      "CREATE TABLE message (id TEXT, session_id TEXT, data TEXT);",
      "CREATE TABLE part (id TEXT, message_id TEXT, session_id TEXT, data TEXT);",
      "INSERT INTO session VALUES ('session-shell', 'Shell fixture', '/tmp/project', '2026-08-06T00:00:02Z');",
      `INSERT INTO message VALUES ('message-user', 'session-shell', '${sqlString(JSON.stringify({ role: "user", time: { created: "2026-08-06T00:00:00Z" } }))}');`,
      `INSERT INTO part VALUES ('part-user', 'message-user', 'session-shell', '${sqlString(JSON.stringify({ type: "text", text: "检查工作区" }))}');`,
      `INSERT INTO message VALUES ('message-assistant', 'session-shell', '${sqlString(JSON.stringify({ role: "assistant", time: { created: "2026-08-06T00:00:01Z" } }))}');`,
      `INSERT INTO part VALUES ('part-shell', 'message-assistant', 'session-shell', '${sqlString(JSON.stringify({ type: "tool", command, output: "Error: failed", callID: "opencode-shell-projection", status: "failed", exit_code: 1 }))}');`,
    ].join("\n"));

    const session = readFixtureSession(dbPath);
    const parts = session.turns[0].parts;
    assert.equal(parts.length, 2);
    assert.deepEqual(parts.map((part) => part.source_execution_id), [
      "opencode-shell-projection",
      "opencode-shell-projection",
    ]);
    assert.equal(parts[0].command, command);
    assert.equal(parts[1].text, "Error: failed");
    assert.equal(JSON.parse(parts[0].metadata_json).shell_execution_projection, undefined);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("OpenCode keeps a simple shell Part without a persisted projection", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-opencode-simple-shell-"));
  try {
    const dbPath = path.join(fixtureRoot, "opencode.db");
    runSqlite(dbPath, [
      "CREATE TABLE session (id TEXT, title TEXT, project TEXT, updated_at TEXT);",
      "CREATE TABLE message (id TEXT, session_id TEXT, data TEXT);",
      "CREATE TABLE part (id TEXT, message_id TEXT, session_id TEXT, data TEXT);",
      "INSERT INTO session VALUES ('session-simple', 'Simple shell fixture', '/tmp/project', '2026-08-06T00:00:02Z');",
      `INSERT INTO message VALUES ('message-user', 'session-simple', '${sqlString(JSON.stringify({ role: "user", time: { created: "2026-08-06T00:00:00Z" } }))}');`,
      `INSERT INTO part VALUES ('part-user', 'message-user', 'session-simple', '${sqlString(JSON.stringify({ type: "text", text: "查看状态" }))}');`,
      `INSERT INTO message VALUES ('message-assistant', 'session-simple', '${sqlString(JSON.stringify({ role: "assistant", time: { created: "2026-08-06T00:00:01Z" } }))}');`,
      `INSERT INTO part VALUES ('part-shell', 'message-assistant', 'session-simple', '${sqlString(JSON.stringify({ type: "tool", tool: "Bash", command: "git status --short", output: "clean", callID: "opencode-simple-shell" }))}');`,
    ].join("\n"));

    const parts = readFixtureSession(dbPath).turns[0].parts;
    assert.equal(parts.length, 2);
    assert.equal(parts[0].command, "git status --short");
    assert.deepEqual(parts.map((part) => part.source_execution_id), [
      "opencode-simple-shell",
      "opencode-simple-shell",
    ]);
    assert.equal(JSON.parse(parts[0].metadata_json).shell_execution_projection, undefined);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("OpenCode skips large before/after fields with no patch and keeps valid patch entries", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-opencode-large-diff-"));
  try {
    const dbPath = path.join(fixtureRoot, "opencode.db");
    const hugeBefore = "A".repeat(200000);
    const hugeAfter = "B".repeat(200000);
    const summary = JSON.stringify({
      role: "user",
      time: { created: "2026-08-06T00:00:00Z" },
      summary: {
        diffs: [
          {
            file: "large-unused.ts",
            before: hugeBefore,
            after: hugeAfter,
            status: "modified",
          },
          {
            file: "src/valid.ts",
            before: hugeBefore,
            after: hugeAfter,
            status: "modified",
            patch: "@@ -1 +1 @@\n-old line\n+new line",
          },
          {
            file: "empty-patch.ts",
            status: "modified",
            patch: "   ",
          },
          {
            file: "src/second.ts",
            status: "added",
            patch: "@@ -0,0 +1 @@\n+added line",
          },
        ],
      },
    });
    runSqlite(dbPath, [
      "CREATE TABLE session (id TEXT, title TEXT, project TEXT, updated_at TEXT);",
      "CREATE TABLE message (id TEXT, session_id TEXT, data TEXT);",
      "CREATE TABLE part (id TEXT, message_id TEXT, session_id TEXT, data TEXT);",
      "INSERT INTO session VALUES ('session-large', 'Large fixture', '/tmp/project', '2026-08-06T00:00:02Z');",
      `INSERT INTO message VALUES ('message-user', 'session-large', '${sqlString(summary)}');`,
      `INSERT INTO part VALUES ('part-user', 'message-user', 'session-large', '${sqlString(JSON.stringify({ type: "text", text: "测试超大摘要" }))}');`,
      `INSERT INTO message VALUES ('message-assistant', 'session-large', '${sqlString(JSON.stringify({ role: "assistant", time: { created: "2026-08-06T00:00:01Z" } }))}');`,
      `INSERT INTO part VALUES ('part-patch', 'message-assistant', 'session-large', '${sqlString(JSON.stringify({ type: "patch", hash: "patch-hash", files: ["src/valid.ts", "src/second.ts"] }))}');`,
    ].join("\n"));

    const session = readFixtureSession(dbPath);
    const parts = session.turns[0].parts;
    const fileChanges = parts.filter((part) => part.kind === "file_change");
    assert.ok(fileChanges.length > 0);
    for (const part of parts) {
      assert.ok(!part.text?.includes("AAAAAA"), "Unused before data must not appear in output");
      assert.ok(!part.text?.includes("BBBBBB"), "Unused after data must not appear in output");
    }
    const allDiffText = fileChanges.map((p) => p.text).join("\n");
    assert.match(allDiffText, /diff --git a\/src\/valid\.ts b\/src\/valid\.ts/);
    assert.match(allDiffText, /diff --git a\/src\/second\.ts b\/src\/second\.ts/);
    assert.ok(!allDiffText.includes("large-unused.ts"));
    assert.ok(!allDiffText.includes("empty-patch.ts"));
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("OpenCode degrades oversized patch payloads exceeding budget to file paths, statistics, and warnings", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-opencode-budget-"));
  try {
    const dbPath = path.join(fixtureRoot, "opencode.db");
    const patchContent = "@@ -1 +1 @@\n" + "+line\n".repeat(200);
    const summary = JSON.stringify({
      role: "user",
      time: { created: "2026-08-06T00:00:00Z" },
      summary: {
        diffs: [
          { file: "src/a.ts", status: "modified", patch: patchContent },
          { file: "src/b.ts", status: "modified", patch: patchContent },
        ],
      },
    });
    runSqlite(dbPath, [
      "CREATE TABLE session (id TEXT, title TEXT, project TEXT, updated_at TEXT);",
      "CREATE TABLE message (id TEXT, session_id TEXT, data TEXT);",
      "CREATE TABLE part (id TEXT, message_id TEXT, session_id TEXT, data TEXT);",
      "INSERT INTO session VALUES ('session-budget', 'Budget fixture', '/tmp/project', '2026-08-06T00:00:02Z');",
      `INSERT INTO message VALUES ('message-user', 'session-budget', '${sqlString(summary)}');`,
      `INSERT INTO part VALUES ('part-user', 'message-user', 'session-budget', '${sqlString(JSON.stringify({ type: "text", text: "测试超载降级" }))}');`,
      `INSERT INTO message VALUES ('message-assistant', 'session-budget', '${sqlString(JSON.stringify({ role: "assistant", time: { created: "2026-08-06T00:00:01Z" } }))}');`,
      `INSERT INTO part VALUES ('part-patch', 'message-assistant', 'session-budget', '${sqlString(JSON.stringify({ type: "patch", hash: "patch-hash", files: ["src/a.ts", "src/b.ts"] }))}');`,
    ].join("\n"));

    const result = spawnSync(process.execPath, [adapterPath], {
      encoding: "utf8",
      env: { ...process.env, ASSETIWEAVE_PATCH_BUDGET_BYTES: "500" },
      input: JSON.stringify({ method: "read_session", source: { location: dbPath }, params: {} }),
    });
    assert.equal(result.status, 0, result.stderr);
    const messages = result.stdout.trim().split("\n").map((line) => JSON.parse(line));
    const warnings = messages.filter((entry) => entry.type === "warning");
    assert.ok(warnings.length > 0, "Must emit structured warning on truncation");
    assert.match(warnings[0].message, /OpenCode summary diff content reduced/);

    const session = messages.find((entry) => entry.type === "item")?.item?.session;
    assert.ok(session);
    const part = session.turns[0].parts.find((p) => p.kind === "file_change");
    assert.ok(part);
    assert.match(part.text, /# File changes reduced: patch payload exceeded safety budget/);
    assert.match(part.text, /- src\/a\.ts/);
    assert.match(part.text, /- src\/b\.ts/);
    assert.match(part.text, /\[Content truncated\]/);

    const meta = JSON.parse(part.metadata_json);
    assert.equal(meta.truncated, true);
    assert.equal(meta.content_reduced, true);
    assert.equal(meta.original_file_count, 2);
    assert.ok(meta.original_bytes > 500);
    assert.equal(meta.retained_representation, "file_paths_and_statistics");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("OpenCode inlines child session subagent turns, emits continuation turn, and excludes child sessions from list_sessions", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-opencode-subagent-"));
  try {
    const dbPath = path.join(fixtureRoot, "opencode.db");
    runSqlite(dbPath, [
      "CREATE TABLE session (id TEXT, title TEXT, project TEXT, updated_at TEXT, parent_id TEXT);",
      "CREATE TABLE message (id TEXT, session_id TEXT, data TEXT);",
      "CREATE TABLE part (id TEXT, message_id TEXT, session_id TEXT, data TEXT);",
      "INSERT INTO session VALUES ('session-parent-1', '主任务会话', '/tmp/project', '2026-08-06T00:00:10Z', NULL);",
      "INSERT INTO session VALUES ('session-sub-1', '代码审查子代理', '/tmp/project', '2026-08-06T00:00:04Z', 'session-parent-1');",
      `INSERT INTO message VALUES ('message-p1', 'session-parent-1', '${sqlString(JSON.stringify({ role: "user", time: { created: "2026-08-06T00:00:00Z" } }))}');`,
      `INSERT INTO part VALUES ('part-p1', 'message-p1', 'session-parent-1', '${sqlString(JSON.stringify({ type: "text", text: "派发子任务审查代码" }))}');`,
      `INSERT INTO message VALUES ('message-p2', 'session-parent-1', '${sqlString(JSON.stringify({ role: "assistant", time: { created: "2026-08-06T00:00:05Z" } }))}');`,
      `INSERT INTO part VALUES ('part-p2', 'message-p2', 'session-parent-1', '${sqlString(JSON.stringify({ type: "text", text: "主智能体继续：综合子任务审查，PR符合规范。" }))}');`,
      `INSERT INTO message VALUES ('message-s1', 'session-sub-1', '${sqlString(JSON.stringify({ role: "user", time: { created: "2026-08-06T00:00:01Z" } }))}');`,
      `INSERT INTO part VALUES ('part-s1', 'message-s1', 'session-sub-1', '${sqlString(JSON.stringify({ type: "text", text: "逐行审查 src/main.ts 的变更" }))}');`,
      `INSERT INTO message VALUES ('message-s2', 'session-sub-1', '${sqlString(JSON.stringify({ role: "assistant", time: { created: "2026-08-06T00:00:03Z" } }))}');`,
      `INSERT INTO part VALUES ('part-s2', 'message-s2', 'session-sub-1', '${sqlString(JSON.stringify({ type: "text", text: "审查完成：没有发现类型安全问题。" }))}');`,
    ].join("\n"));

    // 1. Verify list_sessions strictly excludes child session
    const listResult = spawnSync(process.execPath, [adapterPath], {
      encoding: "utf8",
      input: JSON.stringify({ method: "list_sessions", source: { location: dbPath }, params: {} }),
    });
    assert.equal(listResult.status, 0, listResult.stderr);
    const listMessages = listResult.stdout.trim().split("\n").map((line) => JSON.parse(line));
    const items = listMessages.filter((entry) => entry.type === "item");
    assert.equal(items.length, 1, "Child sessions must be excluded from list_sessions");
    assert.equal(items[0].item.external_id, "session-parent-1");

    // 2. Verify read_session on parent session inlines subagent turn and continuation turn
    const parentSession = readFixtureSession(dbPath);
    assert.equal(parentSession.external_id, "session-parent-1");
    assert.equal(parentSession.turns.length, 3, "Expected anchor, subagent, and continuation turns");

    const [anchorTurn, subTurn, contTurn] = parentSession.turns;

    // Anchor turn
    assert.equal(anchorTurn.external_id, "message-p1");
    assert.equal(anchorTurn.user_text, "派发子任务审查代码");
    assert.equal(anchorTurn.turn_kind, undefined);

    // Subagent turn
    assert.equal(subTurn.turn_kind, "subagent");
    assert.equal(subTurn.agent_role, "代码审查子代理");
    assert.equal(subTurn.parent_turn_id, anchorTurn.external_id);
    assert.equal(subTurn.user_text, "逐行审查 src/main.ts 的变更");
    assert.ok(subTurn.parts.some((p) => p.text?.includes("没有发现类型安全问题")));

    // Continuation turn
    assert.equal(contTurn.turn_kind, "continuation");
    assert.equal(contTurn.parent_turn_id, anchorTurn.external_id);
    assert.equal(contTurn.user_text, "Continue following subagent execution");
    assert.ok(contTurn.parts.some((p) => p.text?.includes("主智能体继续")));

    // 3. Verify read_session directly on child session marks it as subagent and not user visible
    const childResult = spawnSync(process.execPath, [adapterPath], {
      encoding: "utf8",
      input: JSON.stringify({ method: "read_session", source: { location: dbPath }, params: { session_id: "session-sub-1" } }),
    });
    assert.equal(childResult.status, 0, childResult.stderr);
    const childMessages = childResult.stdout.trim().split("\n").map((line) => JSON.parse(line));
    const childSession = childMessages.find((entry) => entry.type === "item")?.item?.session;
    assert.ok(childSession);
    assert.equal(childSession.execution_origin, "subagent");
    assert.equal(childSession.user_visible, false);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("OpenCode multi-turn session keeps subsequent question as standard user turn", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-opencode-multiturn-"));
  try {
    const dbPath = path.join(fixtureRoot, "opencode-multi.db");
    runSqlite(dbPath, [
      "CREATE TABLE session (id TEXT, title TEXT, project TEXT, updated_at TEXT, parent_id TEXT);",
      "CREATE TABLE message (id TEXT, session_id TEXT, data TEXT);",
      "CREATE TABLE part (id TEXT, message_id TEXT, session_id TEXT, data TEXT);",
      // Sessions
      "INSERT INTO session VALUES ('session-multi', 'Multi-turn session', '/tmp/project', '2026-08-06T00:10:00Z', NULL);",
      "INSERT INTO session VALUES ('session-child', 'Subtask worker', '/tmp/project', '2026-08-06T00:01:00Z', 'session-multi');",
      // Question 1
      `INSERT INTO message VALUES ('msg-q1', 'session-multi', '${sqlString(JSON.stringify({ role: "user", time: { created: "2026-08-06T00:00:00Z" } }))}');`,
      `INSERT INTO part VALUES ('part-q1', 'msg-q1', 'session-multi', '${sqlString(JSON.stringify({ type: "text", text: "Question 1: Fix bug" }))}');`,
      // Subtask child messages
      `INSERT INTO message VALUES ('msg-c1', 'session-child', '${sqlString(JSON.stringify({ role: "user", time: { created: "2026-08-06T00:00:30Z" } }))}');`,
      `INSERT INTO part VALUES ('part-c1', 'msg-c1', 'session-child', '${sqlString(JSON.stringify({ type: "text", text: "Subtask execution details" }))}');`,
      `INSERT INTO message VALUES ('msg-c2', 'session-child', '${sqlString(JSON.stringify({ role: "assistant", time: { created: "2026-08-06T00:00:50Z" } }))}');`,
      `INSERT INTO part VALUES ('part-c2', 'msg-c2', 'session-child', '${sqlString(JSON.stringify({ type: "text", text: "Subtask finished" }))}');`,
      // Main agent continuation for Q1
      `INSERT INTO message VALUES ('msg-a1', 'session-multi', '${sqlString(JSON.stringify({ role: "assistant", time: { created: "2026-08-06T00:01:00Z" } }))}');`,
      `INSERT INTO part VALUES ('part-a1', 'msg-a1', 'session-multi', '${sqlString(JSON.stringify({ type: "text", text: "Question 1 resolved" }))}');`,
      // Question 2
      `INSERT INTO message VALUES ('msg-q2', 'session-multi', '${sqlString(JSON.stringify({ role: "user", time: { created: "2026-08-06T00:05:00Z" } }))}');`,
      `INSERT INTO part VALUES ('part-q2', 'msg-q2', 'session-multi', '${sqlString(JSON.stringify({ type: "text", text: "Question 2: What is the weather today in Tokyo?" }))}');`,
      `INSERT INTO message VALUES ('msg-a2', 'session-multi', '${sqlString(JSON.stringify({ role: "assistant", time: { created: "2026-08-06T00:05:30Z" } }))}');`,
      `INSERT INTO part VALUES ('part-a2', 'msg-a2', 'session-multi', '${sqlString(JSON.stringify({ type: "text", text: "Tokyo is sunny." }))}');`,
    ].join("\n"));

    const session = readFixtureSession(dbPath);
    const q2Turn = session.turns.find((t) => t.user_text && t.user_text.includes("Question 2: What is the weather today in Tokyo?"));
    assert.ok(q2Turn, "Question 2 turn must exist");
    assert.equal(q2Turn.turn_kind, undefined, "Question 2 turn_kind must be undefined");
    assert.equal(q2Turn.parent_turn_id, undefined, "Question 2 parent_turn_id must be undefined");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("OpenCode falls back to valid string when subagent prompt is whitespace-only", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-opencode-ws-"));
  try {
    const dbPath = path.join(fixtureRoot, "opencode-ws.db");
    runSqlite(dbPath, [
      "CREATE TABLE session (id TEXT, title TEXT, project TEXT, updated_at TEXT, parent_id TEXT);",
      "CREATE TABLE message (id TEXT, session_id TEXT, data TEXT);",
      "CREATE TABLE part (id TEXT, message_id TEXT, session_id TEXT, data TEXT);",
      "INSERT INTO session VALUES ('session-ws-parent', 'Parent session', '/tmp/project', '2026-08-06T00:10:00Z', NULL);",
      "INSERT INTO session VALUES ('session-ws-child', '   ', '/tmp/project', '2026-08-06T00:01:00Z', 'session-ws-parent');",
      `INSERT INTO message VALUES ('msg-p1', 'session-ws-parent', '${sqlString(JSON.stringify({ role: "user", time: { created: "2026-08-06T00:00:00Z" } }))}');`,
      `INSERT INTO part VALUES ('part-p1', 'msg-p1', 'session-ws-parent', '${sqlString(JSON.stringify({ type: "text", text: "Parent task" }))}');`,
      `INSERT INTO message VALUES ('msg-c1', 'session-ws-child', '${sqlString(JSON.stringify({ role: "user", time: { created: "2026-08-06T00:00:30Z" } }))}');`,
      `INSERT INTO part VALUES ('part-c1', 'msg-c1', 'session-ws-child', '${sqlString(JSON.stringify({ type: "text", text: "   \t\n  " }))}');`,
    ].join("\n"));

    const session = readFixtureSession(dbPath);
    const subTurn = session.turns.find((t) => t.turn_kind === "subagent");
    assert.ok(subTurn, "Subagent turn must exist");
    assert.equal(subTurn.user_text, "Subagent execution");
    assert.ok(subTurn.user_text.trim().length > 0);
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

