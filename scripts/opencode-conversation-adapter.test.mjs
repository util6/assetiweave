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

test("OpenCode models a dispatched subagent as an anchor Part plus a hidden child session", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-opencode-subagent-"));
  try {
    const dbPath = path.join(fixtureRoot, "opencode.db");
    runSqlite(dbPath, [
      "CREATE TABLE session (id TEXT, title TEXT, project TEXT, updated_at TEXT, parent_id TEXT);",
      "CREATE TABLE message (id TEXT, session_id TEXT, data TEXT);",
      "CREATE TABLE part (id TEXT, message_id TEXT, session_id TEXT, data TEXT);",
      "INSERT INTO session VALUES ('session-parent-1', '主任务会话', '/tmp/project', '2026-08-06T00:00:10Z', NULL);",
      "INSERT INTO session VALUES ('session-sub-1', '代码审查子代理 (@general subagent)', '/tmp/project', '2026-08-06T00:00:04Z', 'session-parent-1');",
      `INSERT INTO message VALUES ('message-p1', 'session-parent-1', '${sqlString(JSON.stringify({ role: "user", time: { created: "2026-08-06T00:00:00Z" } }))}');`,
      `INSERT INTO part VALUES ('part-p1', 'message-p1', 'session-parent-1', '${sqlString(JSON.stringify({ type: "text", text: "派发子任务审查代码" }))}');`,
      `INSERT INTO message VALUES ('message-p2', 'session-parent-1', '${sqlString(JSON.stringify({ role: "assistant", time: { created: "2026-08-06T00:00:01Z" } }))}');`,
      `INSERT INTO part VALUES ('part-p2', 'message-p2', 'session-parent-1', '${sqlString(JSON.stringify({ type: "text", text: "正在派发子代理。" }))}');`,
      `INSERT INTO message VALUES ('message-p3', 'session-parent-1', '${sqlString(JSON.stringify({ role: "assistant", time: { created: "2026-08-06T00:00:02Z" } }))}');`,
      `INSERT INTO part VALUES ('part-p3', 'message-p3', 'session-parent-1', '${sqlString(JSON.stringify({
        type: "tool",
        tool: "task",
        callID: "call-task-1",
        state: {
          status: "completed",
          input: { description: "代码审查子代理", prompt: "逐行审查 src/main.ts 的变更", subagent_type: "general" },
          output: "子代理已完成。",
          metadata: { sessionId: "session-sub-1" },
        },
      }))}');`,
      `INSERT INTO message VALUES ('message-p4', 'session-parent-1', '${sqlString(JSON.stringify({ role: "assistant", time: { created: "2026-08-06T00:00:05Z" } }))}');`,
      `INSERT INTO part VALUES ('part-p4', 'message-p4', 'session-parent-1', '${sqlString(JSON.stringify({ type: "text", text: "主智能体继续：综合子任务审查，PR符合规范。" }))}');`,
      `INSERT INTO message VALUES ('message-s1', 'session-sub-1', '${sqlString(JSON.stringify({ role: "user", time: { created: "2026-08-06T00:00:01Z" } }))}');`,
      `INSERT INTO part VALUES ('part-s1', 'message-s1', 'session-sub-1', '${sqlString(JSON.stringify({ type: "text", text: "逐行审查 src/main.ts 的变更" }))}');`,
      `INSERT INTO message VALUES ('message-s2', 'session-sub-1', '${sqlString(JSON.stringify({ role: "assistant", time: { created: "2026-08-06T00:00:03Z" } }))}');`,
      `INSERT INTO part VALUES ('part-s2a', 'message-s2', 'session-sub-1', '${sqlString(JSON.stringify({ type: "tool", tool: "read", callID: "read-1", state: { status: "completed", input: { filePath: "src/main.ts" }, output: "file body" } }))}');`,
      `INSERT INTO part VALUES ('part-s2b', 'message-s2', 'session-sub-1', '${sqlString(JSON.stringify({ type: "text", text: "审查完成：没有发现类型安全问题。" }))}');`,
    ].join("\n"));

    const sessions = readFixtureSessions(dbPath);
    assert.equal(sessions.length, 2, "父会话只产出一份 bundle：父会话 + 直接子会话");
    const parent = sessions.find((session) => session.external_id === "session-parent-1");
    const child = sessions.find((session) => session.external_id === "session-sub-1");
    assert.ok(parent, "父会话必须存在");
    assert.ok(child, "子代理必须产出独立子会话");

    // 父会话：1 次人类提问 = 1 个真实轮次，子代理只留锚点。
    assert.equal(parent.user_visible, true);
    assert.equal(parent.execution_origin, "opencode");
    assert.equal(parent.turns.length, 1);
    const [turn] = parent.turns;
    assert.equal(turn.turn_index, 0);
    assert.equal(turn.user_text, "派发子任务审查代码");
    assert.equal(turn.turn_kind, undefined);
    assert.equal(turn.parent_turn_id, undefined);
    assert.deepEqual(turn.parts.map((part) => part.kind), ["text", "subagent", "text"]);
    assert.equal(turn.parts.at(-1).text, "主智能体继续：综合子任务审查，PR符合规范。");

    const anchor = turn.parts[1];
    assert.equal(anchor.role, "assistant");
    assert.equal(anchor.status, "completed");
    assert.equal(anchor.language, null);
    assert.equal(anchor.command, null);
    assert.equal(anchor.cwd, null);
    assert.equal(anchor.exit_code, null);
    assert.equal(anchor.source_execution_id, "call-task-1");
    assert.equal(anchor.content_card, undefined, "锚点不得自带 content_card");
    assert.equal(
      anchor.text,
      "[委派任务] 代码审查子代理\n[汇报结论] 审查完成：没有发现类型安全问题。",
    );
    const anchorMeta = JSON.parse(anchor.metadata_json);
    assert.equal(anchorMeta.child_external_id, child.external_id);
    assert.equal(anchorMeta.child_external_id, "session-sub-1");
    assert.equal(anchorMeta.agent_role, "general");
    assert.equal(anchorMeta.title, "代码审查子代理");
    assert.equal(anchorMeta.status, "completed");
    assert.equal(anchorMeta.step_count, 1);
    assert.equal(anchorMeta.spawn_anchor, "call-task-1");
    assert.equal(anchorMeta.report_summary, "审查完成：没有发现类型安全问题。");
    assert.equal(anchorMeta.content_card, undefined);

    // 子会话：隐藏、独立从属、#T1 承载全量委派 Prompt。
    assert.equal(child.user_visible, false);
    assert.equal(child.execution_origin, "subagent");
    assert.equal(child.title, "代码审查子代理");
    assert.equal(child.turns.length, 1);
    assert.equal(child.turns[0].turn_index, 0);
    assert.equal(child.turns[0].turn_kind, undefined);
    assert.equal(child.turns[0].user_text, "逐行审查 src/main.ts 的变更");
    assert.ok(child.turns[0].parts.some((part) => part.text?.includes("没有发现类型安全问题")));
    assert.deepEqual(JSON.parse(child.execution_purpose), {
      parent_external_id: "session-parent-1",
      parent_turn_index: 0,
      agent_role: "general",
      spawn_anchor: "call-task-1",
    });

    // list_sessions 必须同时给出父/子描述符（顶层过滤交给后端 user_visible）。
    const listResult = spawnSync(process.execPath, [adapterPath], {
      encoding: "utf8",
      input: JSON.stringify({ method: "list_sessions", source: { location: dbPath }, params: {} }),
    });
    assert.equal(listResult.status, 0, listResult.stderr);
    const listMessages = listResult.stdout.trim().split("\n").map((line) => JSON.parse(line));
    const listedIds = listMessages
      .filter((entry) => entry.type === "item")
      .map((entry) => entry.item.external_id)
      .sort();
    assert.deepEqual(listedIds, ["session-parent-1", "session-sub-1"]);

    // 直接读取子会话：返回子会话本身，溯源信息与打包时一致。
    const directSessions = readFixtureSessions(dbPath, { session_id: "session-sub-1" });
    assert.equal(directSessions.length, 1);
    const direct = directSessions[0];
    assert.equal(direct.external_id, "session-sub-1");
    assert.equal(direct.user_visible, false);
    assert.equal(direct.execution_origin, "subagent");
    assert.deepEqual(JSON.parse(direct.execution_purpose), JSON.parse(child.execution_purpose));
    assert.equal(direct.turns.length, 1);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("OpenCode keeps subsequent questions as standard turns after a subagent dispatch", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-opencode-multiturn-"));
  try {
    const dbPath = path.join(fixtureRoot, "opencode-multi.db");
    runSqlite(dbPath, [
      "CREATE TABLE session (id TEXT, title TEXT, project TEXT, updated_at TEXT, parent_id TEXT);",
      "CREATE TABLE message (id TEXT, session_id TEXT, data TEXT);",
      "CREATE TABLE part (id TEXT, message_id TEXT, session_id TEXT, data TEXT);",
      "INSERT INTO session VALUES ('session-multi', 'Multi-turn session', '/tmp/project', '2026-08-06T00:10:00Z', NULL);",
      "INSERT INTO session VALUES ('session-child', 'Subtask worker (@reviewer subagent)', '/tmp/project', '2026-08-06T00:01:00Z', 'session-multi');",
      `INSERT INTO message VALUES ('msg-q1', 'session-multi', '${sqlString(JSON.stringify({ role: "user", time: { created: "2026-08-06T00:00:00Z" } }))}');`,
      `INSERT INTO part VALUES ('part-q1', 'msg-q1', 'session-multi', '${sqlString(JSON.stringify({ type: "text", text: "Question 1: Fix bug" }))}');`,
      `INSERT INTO message VALUES ('msg-t1', 'session-multi', '${sqlString(JSON.stringify({ role: "assistant", time: { created: "2026-08-06T00:00:20Z" } }))}');`,
      `INSERT INTO part VALUES ('part-t1', 'msg-t1', 'session-multi', '${sqlString(JSON.stringify({
        type: "tool",
        tool: "task",
        callID: "call-task-q1",
        state: {
          status: "completed",
          input: { description: "Subtask worker", prompt: "Subtask execution details", subagent_type: "reviewer" },
          output: "Subtask finished",
          metadata: { sessionId: "session-child" },
        },
      }))}');`,
      `INSERT INTO message VALUES ('msg-c1', 'session-child', '${sqlString(JSON.stringify({ role: "user", time: { created: "2026-08-06T00:00:30Z" } }))}');`,
      `INSERT INTO part VALUES ('part-c1', 'msg-c1', 'session-child', '${sqlString(JSON.stringify({ type: "text", text: "Subtask execution details" }))}');`,
      `INSERT INTO message VALUES ('msg-c2', 'session-child', '${sqlString(JSON.stringify({ role: "assistant", time: { created: "2026-08-06T00:00:50Z" } }))}');`,
      `INSERT INTO part VALUES ('part-c2', 'msg-c2', 'session-child', '${sqlString(JSON.stringify({ type: "text", text: "Subtask finished" }))}');`,
      `INSERT INTO message VALUES ('msg-a1', 'session-multi', '${sqlString(JSON.stringify({ role: "assistant", time: { created: "2026-08-06T00:01:00Z" } }))}');`,
      `INSERT INTO part VALUES ('part-a1', 'msg-a1', 'session-multi', '${sqlString(JSON.stringify({ type: "text", text: "Question 1 resolved" }))}');`,
      `INSERT INTO message VALUES ('msg-q2', 'session-multi', '${sqlString(JSON.stringify({ role: "user", time: { created: "2026-08-06T00:05:00Z" } }))}');`,
      `INSERT INTO part VALUES ('part-q2', 'msg-q2', 'session-multi', '${sqlString(JSON.stringify({ type: "text", text: "Question 2: What is the weather today in Tokyo?" }))}');`,
      `INSERT INTO message VALUES ('msg-a2', 'session-multi', '${sqlString(JSON.stringify({ role: "assistant", time: { created: "2026-08-06T00:05:30Z" } }))}');`,
      `INSERT INTO part VALUES ('part-a2', 'msg-a2', 'session-multi', '${sqlString(JSON.stringify({ type: "text", text: "Tokyo is sunny." }))}');`,
    ].join("\n"));

    const session = readFixtureSession(dbPath);
    assert.equal(session.external_id, "session-multi");
    assert.equal(session.turns.length, 2, "两次人类提问只能产生两个真实轮次");

    const [q1Turn, q2Turn] = session.turns;
    assert.equal(q1Turn.user_text, "Question 1: Fix bug");
    assert.deepEqual(q1Turn.parts.map((part) => part.kind), ["subagent", "text"]);
    assert.equal(JSON.parse(q1Turn.parts[0].metadata_json).child_external_id, "session-child");
    assert.equal(q1Turn.parts.at(-1).text, "Question 1 resolved");

    assert.equal(q2Turn.turn_index, 1);
    assert.equal(q2Turn.user_text, "Question 2: What is the weather today in Tokyo?");
    assert.equal(q2Turn.turn_kind, undefined);
    assert.equal(q2Turn.parent_turn_id, undefined);
    assert.deepEqual(q2Turn.parts.map((part) => part.kind), ["text"]);

    const child = readFixtureSessions(dbPath).find((entry) => entry.external_id === "session-child");
    assert.ok(child);
    const purpose = JSON.parse(child.execution_purpose);
    assert.equal(purpose.parent_turn_index, 0);
    assert.equal(purpose.spawn_anchor, "call-task-q1");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("OpenCode keeps child #T1 prompt non-empty when the delegation prompt is whitespace-only", () => {
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
      `INSERT INTO message VALUES ('msg-p2', 'session-ws-parent', '${sqlString(JSON.stringify({ role: "assistant", time: { created: "2026-08-06T00:00:10Z" } }))}');`,
      `INSERT INTO part VALUES ('part-p2', 'msg-p2', 'session-ws-parent', '${sqlString(JSON.stringify({
        type: "tool",
        tool: "task",
        callID: "call-ws",
        state: { status: "completed", input: { description: "   ", prompt: "   \t\n  " }, output: "子代理已结束。", metadata: { sessionId: "session-ws-child" } },
      }))}');`,
      `INSERT INTO message VALUES ('msg-c1', 'session-ws-child', '${sqlString(JSON.stringify({ role: "user", time: { created: "2026-08-06T00:00:30Z" } }))}');`,
      `INSERT INTO part VALUES ('part-c1', 'msg-c1', 'session-ws-child', '${sqlString(JSON.stringify({ type: "text", text: "   \t\n  " }))}');`,
    ].join("\n"));

    const sessions = readFixtureSessions(dbPath);
    const parent = sessions.find((entry) => entry.external_id === "session-ws-parent");
    const child = sessions.find((entry) => entry.external_id === "session-ws-child");
    assert.ok(parent);
    assert.ok(child);
    const anchor = parent.turns[0].parts.find((part) => part.kind === "subagent");
    assert.ok(anchor, "派发步必须留下锚点");
    assert.equal(anchor.source_execution_id, "call-ws");
    assert.equal(JSON.parse(anchor.metadata_json).child_external_id, "session-ws-child");
    assert.ok(child.turns.length > 0);
    assert.ok(child.turns[0].user_text && child.turns[0].user_text.trim().length > 0);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("OpenCode derives agent role from the subagent title suffix and keeps the task description as title", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-opencode-role-"));
  try {
    const dbPath = path.join(fixtureRoot, "opencode-role.db");
    runSqlite(dbPath, [
      "CREATE TABLE session (id TEXT, title TEXT, project TEXT, updated_at TEXT, parent_id TEXT);",
      "CREATE TABLE message (id TEXT, session_id TEXT, data TEXT);",
      "CREATE TABLE part (id TEXT, message_id TEXT, session_id TEXT, data TEXT);",
      "INSERT INTO session VALUES ('session-role-parent', 'Parent session', '/tmp/project', '2026-08-06T00:10:00Z', NULL);",
      "INSERT INTO session VALUES ('session-role-child', '查找PDF中的医保代码 (@general subagent)', '/tmp/project', '2026-08-06T00:01:00Z', 'session-role-parent');",
      `INSERT INTO message VALUES ('msg-rp', 'session-role-parent', '${sqlString(JSON.stringify({ role: "user", time: { created: "2026-08-06T00:00:00Z" } }))}');`,
      `INSERT INTO part VALUES ('part-rp', 'msg-rp', 'session-role-parent', '${sqlString(JSON.stringify({ type: "text", text: "Parent task" }))}');`,
      `INSERT INTO message VALUES ('msg-rt', 'session-role-parent', '${sqlString(JSON.stringify({ role: "assistant", time: { created: "2026-08-06T00:00:10Z" } }))}');`,
      `INSERT INTO part VALUES ('part-rt', 'msg-rt', 'session-role-parent', '${sqlString(JSON.stringify({
        type: "tool",
        tool: "task",
        callID: "call-role",
        state: { status: "completed", input: { description: "查找PDF中的医保代码", prompt: "查找PDF中的医保代码" }, output: "找到 3 处", metadata: { sessionId: "session-role-child" } },
      }))}');`,
      `INSERT INTO message VALUES ('msg-rc', 'session-role-child', '${sqlString(JSON.stringify({ role: "user", time: { created: "2026-08-06T00:00:30Z" } }))}');`,
      `INSERT INTO part VALUES ('part-rc', 'msg-rc', 'session-role-child', '${sqlString(JSON.stringify({ type: "text", text: "查找PDF中的医保代码" }))}');`,
    ].join("\n"));

    const sessions = readFixtureSessions(dbPath);
    const parent = sessions.find((entry) => entry.external_id === "session-role-parent");
    const child = sessions.find((entry) => entry.external_id === "session-role-child");
    const anchor = parent.turns[0].parts.find((part) => part.kind === "subagent");
    assert.ok(anchor);
    const anchorMeta = JSON.parse(anchor.metadata_json);
    assert.equal(anchorMeta.agent_role, "general", "角色来自标题里的 (@type subagent)");
    assert.equal(anchorMeta.title, "查找PDF中的医保代码");
    assert.equal(child.title, "查找PDF中的医保代码");
    const purpose = JSON.parse(child.execution_purpose);
    assert.equal(purpose.agent_role, "general");
    assert.equal(purpose.parent_external_id, "session-role-parent");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("OpenCode leaves agent role empty when subagent title carries no type suffix", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-opencode-notype-"));
  try {
    const dbPath = path.join(fixtureRoot, "opencode-notype.db");
    runSqlite(dbPath, [
      "CREATE TABLE session (id TEXT, title TEXT, project TEXT, updated_at TEXT, parent_id TEXT);",
      "CREATE TABLE message (id TEXT, session_id TEXT, data TEXT);",
      "CREATE TABLE part (id TEXT, message_id TEXT, session_id TEXT, data TEXT);",
      "INSERT INTO session VALUES ('session-nt-parent', 'Parent session', '/tmp/project', '2026-08-06T00:10:00Z', NULL);",
      "INSERT INTO session VALUES ('session-nt-child', '自由命名的子任务', '/tmp/project', '2026-08-06T00:01:00Z', 'session-nt-parent');",
      `INSERT INTO message VALUES ('msg-ntp', 'session-nt-parent', '${sqlString(JSON.stringify({ role: "user", time: { created: "2026-08-06T00:00:00Z" } }))}');`,
      `INSERT INTO part VALUES ('part-ntp', 'msg-ntp', 'session-nt-parent', '${sqlString(JSON.stringify({ type: "text", text: "Parent task" }))}');`,
      `INSERT INTO message VALUES ('msg-ntt', 'session-nt-parent', '${sqlString(JSON.stringify({ role: "assistant", time: { created: "2026-08-06T00:00:10Z" } }))}');`,
      `INSERT INTO part VALUES ('part-ntt', 'msg-ntt', 'session-nt-parent', '${sqlString(JSON.stringify({
        type: "tool",
        tool: "task",
        callID: "call-notype",
        state: { status: "completed", input: { description: "自由命名的子任务", prompt: "自由命名的子任务" }, output: "已完成", metadata: { sessionId: "session-nt-child" } },
      }))}');`,
      `INSERT INTO message VALUES ('msg-ntc', 'session-nt-child', '${sqlString(JSON.stringify({ role: "user", time: { created: "2026-08-06T00:00:30Z" } }))}');`,
      `INSERT INTO part VALUES ('part-ntc', 'msg-ntc', 'session-nt-child', '${sqlString(JSON.stringify({ type: "text", text: "自由命名的子任务" }))}');`,
    ].join("\n"));

    const sessions = readFixtureSessions(dbPath);
    const parent = sessions.find((entry) => entry.external_id === "session-nt-parent");
    const child = sessions.find((entry) => entry.external_id === "session-nt-child");
    const anchor = parent.turns[0].parts.find((part) => part.kind === "subagent");
    assert.ok(anchor);
    const anchorMeta = JSON.parse(anchor.metadata_json);
    assert.equal(anchorMeta.agent_role, undefined, "取不到角色时必须留空，不能伪造常量角色");
    assert.equal(anchorMeta.title, "自由命名的子任务");
    assert.equal(child.title, "自由命名的子任务");
    assert.equal(JSON.parse(child.execution_purpose).agent_role, undefined);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("OpenCode keeps every child round as a real turn inside the hidden child session", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-opencode-merge-"));
  try {
    const dbPath = path.join(fixtureRoot, "opencode-merge.db");
    runSqlite(dbPath, [
      "CREATE TABLE session (id TEXT, title TEXT, project TEXT, updated_at TEXT, parent_id TEXT);",
      "CREATE TABLE message (id TEXT, session_id TEXT, data TEXT);",
      "CREATE TABLE part (id TEXT, message_id TEXT, session_id TEXT, data TEXT);",
      "INSERT INTO session VALUES ('session-merge-parent', '主任务', '/tmp/project', '2026-08-06T00:10:00Z', NULL);",
      "INSERT INTO session VALUES ('session-merge-child', '核对迁移脚本 (@reviewer subagent)', '/tmp/project', '2026-08-06T00:05:00Z', 'session-merge-parent');",
      `INSERT INTO message VALUES ('msg-p1', 'session-merge-parent', '${sqlString(JSON.stringify({ role: "user", time: { created: "2026-08-06T00:00:00Z" } }))}');`,
      `INSERT INTO part VALUES ('part-p1', 'msg-p1', 'session-merge-parent', '${sqlString(JSON.stringify({ type: "text", text: "派发子代理核对迁移脚本" }))}');`,
      `INSERT INTO message VALUES ('msg-p2', 'session-merge-parent', '${sqlString(JSON.stringify({ role: "assistant", time: { created: "2026-08-06T00:00:10Z" } }))}');`,
      `INSERT INTO part VALUES ('part-p2', 'msg-p2', 'session-merge-parent', '${sqlString(JSON.stringify({
        type: "tool",
        tool: "task",
        callID: "call-merge",
        state: { status: "completed", input: { description: "核对迁移脚本", prompt: "先核对索引", subagent_type: "reviewer" }, output: "已启动", metadata: { sessionId: "session-merge-child" } },
      }))}');`,
      `INSERT INTO message VALUES ('msg-c1', 'session-merge-child', '${sqlString(JSON.stringify({ role: "user", time: { created: "2026-08-06T00:01:00Z" } }))}');`,
      `INSERT INTO part VALUES ('part-c1', 'msg-c1', 'session-merge-child', '${sqlString(JSON.stringify({ type: "text", text: "先核对索引" }))}');`,
      `INSERT INTO message VALUES ('msg-c2', 'session-merge-child', '${sqlString(JSON.stringify({ role: "assistant", time: { created: "2026-08-06T00:02:00Z" } }))}');`,
      `INSERT INTO part VALUES ('part-c2', 'msg-c2', 'session-merge-child', '${sqlString(JSON.stringify({ type: "text", text: "索引核对完成。" }))}');`,
      `INSERT INTO message VALUES ('msg-c3', 'session-merge-child', '${sqlString(JSON.stringify({ role: "user", time: { created: "2026-08-06T00:03:00Z" } }))}');`,
      `INSERT INTO part VALUES ('part-c3', 'msg-c3', 'session-merge-child', '${sqlString(JSON.stringify({ type: "text", text: "再核对外键" }))}');`,
      `INSERT INTO message VALUES ('msg-c4', 'session-merge-child', '${sqlString(JSON.stringify({ role: "assistant", time: { created: "2026-08-06T00:04:00Z" } }))}');`,
      `INSERT INTO part VALUES ('part-c4', 'msg-c4', 'session-merge-child', '${sqlString(JSON.stringify({ type: "text", text: "外键核对完成：无违规。" }))}');`,
    ].join("\n"));

    const sessions = readFixtureSessions(dbPath);
    const parent = sessions.find((entry) => entry.external_id === "session-merge-parent");
    const child = sessions.find((entry) => entry.external_id === "session-merge-child");
    assert.equal(parent.turns.length, 1, "子会话的多轮不能外溢成父会话的伪轮次");
    assert.equal(parent.turns[0].parts.filter((part) => part.kind === "subagent").length, 1);

    assert.deepEqual(
      child.turns.map((turn) => turn.user_text),
      ["先核对索引", "再核对外键"],
      "子会话内部保持真实轮次，不折叠成 subagent Turn",
    );
    assert.deepEqual(
      child.turns.map((turn) => turn.turn_index),
      [0, 1],
    );
    for (const turn of child.turns) {
      assert.equal(turn.turn_kind, undefined);
      assert.equal(turn.parent_turn_id, undefined);
    }

    const anchorMeta = JSON.parse(parent.turns[0].parts.find((part) => part.kind === "subagent").metadata_json);
    assert.equal(anchorMeta.agent_role, "reviewer");
    assert.equal(anchorMeta.report_summary, "外键核对完成：无违规。");
    assert.equal(anchorMeta.step_count, 0);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("OpenCode pairs task steps with child sessions in dispatch order when session id metadata is absent", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-opencode-order-"));
  try {
    const dbPath = path.join(fixtureRoot, "opencode-order.db");
    runSqlite(dbPath, [
      "CREATE TABLE session (id TEXT, title TEXT, project TEXT, updated_at TEXT, parent_id TEXT);",
      "CREATE TABLE message (id TEXT, session_id TEXT, data TEXT);",
      "CREATE TABLE part (id TEXT, message_id TEXT, session_id TEXT, data TEXT);",
      "INSERT INTO session VALUES ('session-order-parent', '顺序派发', '/tmp/project', '2026-08-06T00:10:00Z', NULL);",
      "INSERT INTO session VALUES ('session-order-a', '第一个子任务 (@general subagent)', '/tmp/project', '2026-08-06T00:02:00Z', 'session-order-parent');",
      "INSERT INTO session VALUES ('session-order-b', '第二个子任务 (@general subagent)', '/tmp/project', '2026-08-06T00:04:00Z', 'session-order-parent');",
      `INSERT INTO message VALUES ('msg-op', 'session-order-parent', '${sqlString(JSON.stringify({ role: "user", time: { created: "2026-08-06T00:00:00Z" } }))}');`,
      `INSERT INTO part VALUES ('part-op', 'msg-op', 'session-order-parent', '${sqlString(JSON.stringify({ type: "text", text: "并行派发两个子任务" }))}');`,
      `INSERT INTO message VALUES ('msg-ot1', 'session-order-parent', '${sqlString(JSON.stringify({ role: "assistant", time: { created: "2026-08-06T00:00:10Z" } }))}');`,
      `INSERT INTO part VALUES ('part-ot1', 'msg-ot1', 'session-order-parent', '${sqlString(JSON.stringify({ type: "tool", tool: "task", callID: "call-first", state: { status: "completed", input: { description: "第一个子任务", prompt: "任务一" }, output: "完成一" } }))}');`,
      `INSERT INTO message VALUES ('msg-ot2', 'session-order-parent', '${sqlString(JSON.stringify({ role: "assistant", time: { created: "2026-08-06T00:00:20Z" } }))}');`,
      `INSERT INTO part VALUES ('part-ot2', 'msg-ot2', 'session-order-parent', '${sqlString(JSON.stringify({ type: "tool", tool: "task", callID: "call-second", state: { status: "completed", input: { description: "第二个子任务", prompt: "任务二" }, output: "完成二" } }))}');`,
      `INSERT INTO message VALUES ('msg-oa1', 'session-order-a', '${sqlString(JSON.stringify({ role: "user", time: { created: "2026-08-06T00:01:00Z" } }))}');`,
      `INSERT INTO part VALUES ('part-oa1', 'msg-oa1', 'session-order-a', '${sqlString(JSON.stringify({ type: "text", text: "任务一" }))}');`,
      `INSERT INTO message VALUES ('msg-oa2', 'session-order-a', '${sqlString(JSON.stringify({ role: "assistant", time: { created: "2026-08-06T00:01:30Z" } }))}');`,
      `INSERT INTO part VALUES ('part-oa2', 'msg-oa2', 'session-order-a', '${sqlString(JSON.stringify({ type: "text", text: "结果一" }))}');`,
      `INSERT INTO message VALUES ('msg-ob1', 'session-order-b', '${sqlString(JSON.stringify({ role: "user", time: { created: "2026-08-06T00:03:00Z" } }))}');`,
      `INSERT INTO part VALUES ('part-ob1', 'msg-ob1', 'session-order-b', '${sqlString(JSON.stringify({ type: "text", text: "任务二" }))}');`,
      `INSERT INTO message VALUES ('msg-ob2', 'session-order-b', '${sqlString(JSON.stringify({ role: "assistant", time: { created: "2026-08-06T00:03:30Z" } }))}');`,
      `INSERT INTO part VALUES ('part-ob2', 'msg-ob2', 'session-order-b', '${sqlString(JSON.stringify({ type: "text", text: "结果二" }))}');`,
    ].join("\n"));

    const sessions = readFixtureSessions(dbPath);
    const parent = sessions.find((entry) => entry.external_id === "session-order-parent");
    const anchors = parent.turns[0].parts.filter((part) => part.kind === "subagent");
    assert.equal(anchors.length, 2, "两个派发步必须各留一个锚点");

    const firstMeta = JSON.parse(anchors[0].metadata_json);
    const secondMeta = JSON.parse(anchors[1].metadata_json);
    assert.equal(firstMeta.child_external_id, "session-order-a");
    assert.equal(firstMeta.spawn_anchor, "call-first");
    assert.equal(firstMeta.report_summary, "结果一");
    assert.equal(secondMeta.child_external_id, "session-order-b");
    assert.equal(secondMeta.spawn_anchor, "call-second");
    assert.equal(secondMeta.report_summary, "结果二");

    const childA = sessions.find((entry) => entry.external_id === "session-order-a");
    const childB = sessions.find((entry) => entry.external_id === "session-order-b");
    assert.equal(firstMeta.child_external_id, childA.external_id);
    assert.equal(secondMeta.child_external_id, childB.external_id);
    assert.equal(JSON.parse(childA.execution_purpose).spawn_anchor, "call-first");
    assert.equal(JSON.parse(childB.execution_purpose).spawn_anchor, "call-second");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("OpenCode extracts usage event with turn_id, external_event_id and millisecond UTC timestamp from table with time_created", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-opencode-usage-col-"));
  try {
    const dbPath = path.join(fixtureRoot, "opencode.db");
    const msgData = JSON.stringify({
      role: "assistant",
      modelID: "claude-3-7-sonnet",
      providerID: "anthropic",
      cost: 0.0035,
      tokens: {
        input: 100,
        output: 50,
        reasoning: 10,
        cache: { read: 20, write: 5 },
        total: 185,
      },
    });
    runSqlite(dbPath, [
      "CREATE TABLE session (id TEXT, title TEXT, project TEXT, updated_at TEXT);",
      "CREATE TABLE message (id TEXT, session_id TEXT, time_created INTEGER, data TEXT);",
      "INSERT INTO session VALUES ('sess-100', 'Test Session', '/tmp/proj', '2026-10-30T04:00:00Z');",
      `INSERT INTO message VALUES ('msg-42', 'sess-100', 1761806400000, '${sqlString(msgData)}');`,
    ].join("\n"));

    const { events, complete } = readFixtureUsage(dbPath);
    assert.equal(complete.snapshot_complete, true);
    assert.equal(complete.usage_event_count, 1);
    assert.equal(complete.decoder_profile, "opencode-sqlite-v1");
    assert.equal(events.length, 1);

    const event = events[0];
    assert.equal(event.session_id, "sess-100");
    assert.equal(event.turn_id, "msg-42");
    assert.equal(event.external_event_id, "sess-100-msg-42");
    assert.equal(event.logical_request_id, "sess-100-msg-42");
    assert.equal(event.attempt_index, 0);
    assert.equal(event.provider, "anthropic");
    assert.equal(event.model, "claude-3-7-sonnet");
    assert.equal(event.timestamp, new Date(1761806400000).toISOString());
    assert.equal(event.input_tokens, 100);
    assert.equal(event.output_tokens, 50);
    assert.equal(event.reasoning_tokens, 10);
    assert.equal(event.cache_read_tokens, 20);
    assert.equal(event.cache_write_tokens, 5);
    assert.equal(event.total_tokens, 185);
    assert.equal(event.cost, 0.0035);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("OpenCode safely queries usage when time_created column is missing by probing table_info and reading data.time.created", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-opencode-usage-nocol-"));
  try {
    const dbPath = path.join(fixtureRoot, "opencode.db");
    const msgData = JSON.stringify({
      role: "assistant",
      time: { created: "2026-10-30T04:30:00.123Z" },
      modelID: "gpt-4o",
      providerID: "openai",
      tokens: {
        input: 80,
        output: 40,
        total: 120,
      },
    });
    runSqlite(dbPath, [
      "CREATE TABLE session (id TEXT, title TEXT, project TEXT, updated_at TEXT);",
      // Schema without time_created column
      "CREATE TABLE message (id TEXT, session_id TEXT, data TEXT);",
      "INSERT INTO session VALUES ('sess-200', 'Test Session', '/tmp/proj', '2026-10-30T04:30:00Z');",
      `INSERT INTO message VALUES ('msg-99', 'sess-200', '${sqlString(msgData)}');`,
    ].join("\n"));

    const { events, complete } = readFixtureUsage(dbPath);
    assert.equal(complete.usage_event_count, 1);
    assert.equal(events.length, 1);

    const event = events[0];
    assert.equal(event.session_id, "sess-200");
    assert.equal(event.turn_id, "msg-99");
    assert.equal(event.external_event_id, "sess-200-msg-99");
    assert.equal(event.timestamp, "2026-10-30T04:30:00.123Z");
    assert.equal(event.input_tokens, 80);
    assert.equal(event.output_tokens, 40);
    assert.equal(event.total_tokens, 120);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("OpenCode suppresses messages where totalTokens <= 0", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-opencode-usage-zero-"));
  try {
    const dbPath = path.join(fixtureRoot, "opencode.db");
    const zeroTokenMsg = JSON.stringify({
      role: "assistant",
      tokens: { input: 0, output: 0, total: 0 },
    });
    const validTokenMsg = JSON.stringify({
      role: "assistant",
      tokens: { input: 50, output: 25, total: 75 },
    });
    runSqlite(dbPath, [
      "CREATE TABLE session (id TEXT, title TEXT, project TEXT, updated_at TEXT);",
      "CREATE TABLE message (id TEXT, session_id TEXT, time_created INTEGER, data TEXT);",
      "INSERT INTO session VALUES ('sess-zero', 'Test', '/tmp', '2026-10-30T05:00:00Z');",
      `INSERT INTO message VALUES ('msg-zero', 'sess-zero', 1761807000000, '${sqlString(zeroTokenMsg)}');`,
      `INSERT INTO message VALUES ('msg-valid', 'sess-zero', 1761807001000, '${sqlString(validTokenMsg)}');`,
    ].join("\n"));

    const { events, complete } = readFixtureUsage(dbPath);
    assert.equal(complete.usage_event_count, 1);
    assert.equal(events.length, 1);
    assert.equal(events[0].turn_id, "msg-valid");
    assert.equal(events[0].external_event_id, "sess-zero-msg-valid");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("OpenCode supports incremental cursor queries using last_time_created", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-opencode-usage-cursor-"));
  try {
    const dbPath = path.join(fixtureRoot, "opencode.db");
    const msg1 = JSON.stringify({ role: "assistant", tokens: { input: 10, output: 10, total: 20 } });
    const msg2 = JSON.stringify({ role: "assistant", tokens: { input: 20, output: 20, total: 40 } });
    runSqlite(dbPath, [
      "CREATE TABLE session (id TEXT, title TEXT, project TEXT, updated_at TEXT);",
      "CREATE TABLE message (id TEXT, session_id TEXT, time_created INTEGER, data TEXT);",
      "INSERT INTO session VALUES ('sess-cur', 'Test', '/tmp', '2026-10-30T05:00:00Z');",
      `INSERT INTO message VALUES ('msg-1', 'sess-cur', 1000, '${sqlString(msg1)}');`,
    ].join("\n"));

    const firstRun = readFixtureUsage(dbPath);
    assert.equal(firstRun.events.length, 1);
    const cursor = JSON.parse(firstRun.complete.next_cursor);
    assert.equal(cursor.last_time_created, 1000);

    // Insert message 2 with time_created 2000
    runSqlite(dbPath, [
      `INSERT INTO message VALUES ('msg-2', 'sess-cur', 2000, '${sqlString(msg2)}');`,
    ].join("\n"));

    const secondRun = readFixtureUsage(dbPath, { cursor: firstRun.complete.next_cursor });
    assert.equal(secondRun.events.length, 1);
    assert.equal(secondRun.events[0].turn_id, "msg-2");
    const nextCursor = JSON.parse(secondRun.complete.next_cursor);
    assert.equal(nextCursor.last_time_created, 2000);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

function readFixtureSessions(dbPath, params = {}) {
  const result = spawnSync(process.execPath, [adapterPath], {
    encoding: "utf8",
    input: JSON.stringify({ method: "read_session", source: { location: dbPath }, params }),
  });
  assert.equal(result.status, 0, result.stderr);
  const messages = result.stdout.trim().split("\n").map((line) => JSON.parse(line));
  return messages.filter((entry) => entry.type === "item").map((entry) => entry.item.session);
}

function readFixtureSession(dbPath, params = {}) {
  const sessions = readFixtureSessions(dbPath, params);
  const session = sessions[0];
  assert.ok(session);
  return session;
}

function readFixtureUsage(location, params = {}) {
  const result = spawnSync(process.execPath, [adapterPath], {
    encoding: "utf8",
    input: JSON.stringify({
      method: "read_usage",
      source: { location },
      params,
    }),
  });
  assert.equal(result.status, 0, result.stderr);
  const messages = result.stdout.trim().split("\n").filter(Boolean).map((line) => JSON.parse(line));
  const events = messages
    .filter((message) => message.type === "usage_event")
    .map((message) => message.usage_event);
  const complete = messages.find((message) => message.type === "complete")?.item ?? null;
  return { messages, events, complete };
}

function runSqlite(dbPath, sql) {
  const result = spawnSync("sqlite3", [dbPath], { encoding: "utf8", input: sql });
  assert.equal(result.status, 0, result.stderr);
}

function sqlString(value) {
  return value.replaceAll("'", "''");
}

