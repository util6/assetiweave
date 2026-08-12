import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { PAYLOAD_POLICY_VERSION } from "../builtin-assets/adapters/codex/payload-policy.mjs";

const repositoryRoot = path.resolve(import.meta.dirname, "..");
const adapterPath = path.join(repositoryRoot, "builtin-assets/adapters/codex/adapter.mjs");

test("Codex adapter keeps Skill injection out of the user question and emits one Skill path card", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-skill-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    const skillPath = "/Users/test/.codex/skills/session-exporter/SKILL.md";
    writeFileSync(rolloutPath, [
      event("2026-07-27T00:00:00Z", "user", `[$session-exporter](${skillPath}) 归档 SESSION`),
      event("2026-07-27T00:00:01Z", "user", [
        "<skill>",
        "<name>session-exporter</name>",
        `<path>${skillPath}</path>`,
        "---",
        "name: session-exporter",
        "description: Export a session.",
        "</skill>",
      ].join("\n")),
      event("2026-07-27T00:00:02Z", "assistant", "归档完成。"),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('session-1', '${sqlString(rolloutPath)}', 'Fixture', '2026-07-27T00:00:02Z');`,
    ].join("\n"));

    const result = spawnSync(process.execPath, [adapterPath], {
      encoding: "utf8",
      input: JSON.stringify({ method: "read_session", source: { location: fixtureRoot }, params: {} }),
    });
    assert.equal(result.status, 0, result.stderr);
    const messages = result.stdout.trim().split("\n").map((line) => JSON.parse(line));
    const session = messages.find((message) => message.type === "item")?.item?.session;

    assert.ok(session);
    assert.equal(session.turns.length, 1);
    assert.equal(session.turns[0].user_text, "归档 SESSION");
    assert.deepEqual(
      session.turns[0].parts.map((part) => ({
        kind: part.content_card?.kind,
        renderer: part.content_card?.renderer,
        role: part.role,
        text: part.text,
      })),
      [
        { kind: "codex.skill", renderer: "path", role: "system", text: skillPath },
        { kind: "codex.answer", renderer: "markdown", role: "assistant", text: "归档完成。" },
      ],
    );
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter separates a SKILL.md read from the command result", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-skill-read-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    const skillPath = "/Users/test/.codex/skills/session-exporter/SKILL.md";
    const skillContent = [
      "---",
      "name: session-exporter",
      "description: Export a session.",
      "---",
      "",
      "# Session Exporter",
      "",
      "Export a local Codex session.",
    ].join("\n");
    writeFileSync(rolloutPath, [
      event("2026-07-27T00:00:00Z", "user", "导出当前会话"),
      JSON.stringify({
        timestamp: "2026-07-27T00:00:01Z",
        type: "response_item",
        payload: { type: "exec", command: `cat ${skillPath}` },
      }),
      JSON.stringify({
        timestamp: "2026-07-27T00:00:02Z",
        type: "response_item",
        payload: { type: "tool_result", name: "exec", output: skillContent },
      }),
      event("2026-07-27T00:00:03Z", "assistant", "已读取 Skill。"),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('session-1', '${sqlString(rolloutPath)}', 'Fixture', '2026-07-27T00:00:03Z');`,
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    assert.equal(session.turns[0].user_text, "导出当前会话");
    assert.deepEqual(
      session.turns[0].parts.map((part) => ({
        command: part.command,
        kind: part.content_card?.kind,
        renderer: part.content_card?.renderer,
        role: part.role,
        text: part.text,
      })),
      [
        { command: skillPath, kind: "codex.command", renderer: "command", role: "tool", text: null },
        { command: null, kind: "codex.skill", renderer: "path", role: "system", text: skillPath },
        { command: null, kind: "codex.answer", renderer: "markdown", role: "assistant", text: "已读取 Skill。" },
      ],
    );
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter recognizes a SKILL.md read from a modern custom exec call", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-custom-skill-read-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    const skillPath = "/Users/test/.codex/skills/using-agent-skills/SKILL.md";
    const workdir = "/Users/test/code-space/assetiweave";
    const skillContent = [
      "---",
      "name: using-agent-skills",
      "description: Discover and invoke agent skills.",
      "---",
      "",
      "# Using Agent Skills",
    ].join("\n");
    writeFileSync(rolloutPath, [
      event("2026-07-31T00:00:00Z", "user", "读取 Skill"),
      JSON.stringify({
        timestamp: "2026-07-31T00:00:01Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call",
          name: "exec",
          call_id: "call-skill-read",
          input: [
            `const r = await tools.exec_command(${JSON.stringify({ cmd: `cat ${skillPath}`, workdir })});`,
            "text(r.output);",
          ].join("\n"),
        },
      }),
      JSON.stringify({
        timestamp: "2026-07-31T00:00:02Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call_output",
          call_id: "call-skill-read",
          output: [
            { type: "input_text", text: "Script completed\nWall time 0.1 seconds\nOutput:\n" },
            { type: "input_text", text: skillContent },
          ],
        },
      }),
      event("2026-07-31T00:00:03Z", "assistant", "已读取 Skill。"),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('session-1', '${sqlString(rolloutPath)}', 'Fixture', '2026-07-31T00:00:03Z');`,
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    assert.deepEqual(
      session.turns[0].parts.map((part) => ({
        command: part.command,
        cwd: part.cwd,
        kind: part.content_card?.kind,
        renderer: part.content_card?.renderer,
        role: part.role,
        text: part.text,
      })),
      [
        {
          command: skillPath,
          cwd: workdir,
          kind: "codex.command",
          renderer: "command",
          role: "tool",
          text: null,
        },
        {
          command: null,
          cwd: null,
          kind: "codex.skill",
          renderer: "path",
          role: "system",
          text: skillPath,
        },
        {
          command: null,
          cwd: null,
          kind: "codex.answer",
          renderer: "markdown",
          role: "assistant",
          text: "已读取 Skill。",
        },
      ],
    );
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter preserves source execution IDs across interleaved command results", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-executions-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    writeFileSync(rolloutPath, [
      event("2026-08-02T00:00:00Z", "user", "并行运行检查"),
      JSON.stringify({
        timestamp: "2026-08-02T00:00:01Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call",
          name: "exec",
          call_id: "call-typecheck",
          input: "const r = await tools.exec_command({\"cmd\":\"pnpm typecheck\"}); text(r.output);",
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-02T00:00:02Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call",
          name: "exec",
          call_id: "call-test",
          input: "const r = await tools.exec_command({\"cmd\":\"pnpm test\"}); text(r.output);",
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-02T00:00:03Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call_output",
          call_id: "call-test",
          output: [{ type: "input_text", text: "Error: tests failed" }],
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-02T00:00:04Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call_output",
          call_id: "call-typecheck",
          output: [{ type: "input_text", text: "Error: typecheck failed" }],
        },
      }),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('session-1', '${sqlString(rolloutPath)}', 'Fixture', '2026-08-02T00:00:04Z');`,
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    assert.deepEqual(
      session.turns[0].parts.map((part) => ({
        kind: part.content_card?.kind,
        sourceExecutionId: part.source_execution_id,
      })),
      [
        { kind: "codex.command", sourceExecutionId: "call-typecheck" },
        { kind: "codex.command", sourceExecutionId: "call-test" },
        { kind: "codex.result", sourceExecutionId: "call-test" },
        { kind: "codex.result", sourceExecutionId: "call-typecheck" },
      ],
    );
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter omits decorative printf separators from aggregated command cards", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-command-separators-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    const command = [
      "printf '%s\\n' '--- status ---'",
      "git status --short",
      "printf '%s\\n' '--- staged diff stat ---'",
      "git diff --cached --stat",
    ].join("; ");
    writeFileSync(rolloutPath, [
      event("2026-08-10T00:00:00Z", "user", "检查工作区"),
      JSON.stringify({
        timestamp: "2026-08-10T00:00:01Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call",
          name: "exec",
          call_id: "call-status",
          status: "completed",
          input: `const r = await tools.exec_command(${JSON.stringify({ cmd: command, workdir: "/tmp/project" })}); text(r.output);`,
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-10T00:00:02Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call_output",
          call_id: "call-status",
          output: "Script completed\nWall time 0.1 seconds\nOutput:\nclean",
        },
      }),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('session-1', '${sqlString(rolloutPath)}', 'Fixture', '2026-08-10T00:00:02Z');`,
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    const parts = session.turns[0].parts;
    assert.deepEqual(parts.map((part) => part.command), [
      "git status --short",
      "git diff --cached --stat",
    ]);
    assert.deepEqual(parts.map((part) => part.command_label ?? null), [
      "status",
      "staged diff stat",
    ]);
    assert.deepEqual(parts.map((part) => part.source_execution_id), [
      "call-status:command:1",
      "call-status:command:2",
    ]);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter removes successful structured execution output", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-result-text-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    writeFileSync(rolloutPath, [
      event("2026-08-02T00:00:00Z", "user", "运行检查"),
      JSON.stringify({
        timestamp: "2026-08-02T00:00:01Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call",
          name: "exec",
          call_id: "call-check",
          input: "const r = await tools.exec_command({\"cmd\":\"printf '\\\\n'\"}); text(r.output);",
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-02T00:00:02Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call_output",
          call_id: "call-check",
          output: JSON.stringify([
            { type: "input_text", text: "Script completed\nWall time 0.1 seconds\nOutput:\n" },
            { type: "input_text", text: "\u001b[32mcheck passed\u001b[0m" },
          ]),
        },
      }),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('session-1', '${sqlString(rolloutPath)}', 'Fixture', '2026-08-02T00:00:02Z');`,
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    const [command] = session.turns[0].parts;
    assert.equal(command.command, "printf '\\n'");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter omits successful command results whose payload is an empty object", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-empty-result-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    writeFileSync(rolloutPath, [
      event("2026-08-02T00:00:00Z", "user", "运行无输出检查"),
      JSON.stringify({
        timestamp: "2026-08-02T00:00:01Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call",
          name: "exec",
          call_id: "call-empty-result",
          input: "const r = await tools.exec_command({\"cmd\":\"true\"}); text(r.output);",
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-02T00:00:02Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call_output",
          call_id: "call-empty-result",
          output: [
            { type: "input_text", text: "Script completed\nWall time 0.1 seconds\nOutput:\n" },
            { type: "input_text", text: "{}" },
          ],
        },
      }),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('session-1', '${sqlString(rolloutPath)}', 'Fixture', '2026-08-02T00:00:02Z');`,
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    assert.deepEqual(
      session.turns[0].parts.map((part) => part.content_card?.kind),
      ["codex.command"],
    );
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter omits unpaired script results whose payload is an empty object", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-empty-script-result-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    writeFileSync(rolloutPath, [
      event("2026-08-02T00:00:00Z", "user", "更新执行计划"),
      JSON.stringify({
        timestamp: "2026-08-02T00:00:01Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call",
          name: "exec",
          call_id: "call-empty-script-result",
          input: "const r = await tools.update_plan({plan: []}); text(r);",
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-02T00:00:02Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call_output",
          call_id: "call-empty-script-result",
          output: [
            { type: "input_text", text: "Script completed\nWall time 0.1 seconds\nOutput:\n" },
            { type: "input_text", text: "{}" },
          ],
        },
      }),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('session-1', '${sqlString(rolloutPath)}', 'Fixture', '2026-08-02T00:00:02Z');`,
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    assert.deepEqual(
      session.turns[0].parts.filter((part) => part.content_card?.kind === "codex.result"),
      [],
    );
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter preserves and budgets unpaired structured output as unclassified", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-large-result-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    const terminalOutput = `visible output line\n${"detail line\n".repeat(3_000)}`;
    writeFileSync(rolloutPath, [
      event("2026-08-03T00:00:00Z", "user", "运行大型检查"),
      JSON.stringify({
        timestamp: "2026-08-03T00:00:01Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call_output",
          call_id: "call-large-check",
          output: JSON.stringify([
            { type: "input_text", text: "Script completed\nWall time 0.1 seconds\nOutput:\n" },
            { type: "input_text", text: terminalOutput },
          ]),
        },
      }),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('session-1', '${sqlString(rolloutPath)}', 'Fixture', '2026-08-03T00:00:01Z');`,
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    const [result] = session.turns[0].parts;
    assert.ok(result.text.startsWith("visible output line\n"));
    assert.equal(result.text.includes("input_text"), false);
    assert.equal(result.text.startsWith("[{"), false);
    assert.equal(JSON.parse(result.metadata_json).execution_kind, "unclassified");
    assert.equal(result.content_card?.renderer, "terminal_output");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter hides pending runner output and wait controls without shifting Part positions", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-control-noise-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    writeFileSync(rolloutPath, [
      event("2026-08-03T00:00:00Z", "user", "等待脚本完成"),
      JSON.stringify({
        timestamp: "2026-08-03T00:00:01Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call_output",
          call_id: "call-running-check",
          output: [{
            type: "input_text",
            text: "Script running with cell ID 7\nWall time 11.0 seconds\nOutput:\n",
          }],
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-03T00:00:02Z",
        type: "response_item",
        payload: {
          type: "function_call",
          name: "wait",
          arguments: JSON.stringify({ cell_id: "7", yield_time_ms: 30_000 }),
        },
      }),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('session-1', '${sqlString(rolloutPath)}', 'Fixture', '2026-08-03T00:00:02Z');`,
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    assert.equal(session.turns[0].parts.length, 2);
    assert.equal(session.turns[0].parts[0].text, null);
    assert.equal(session.turns[0].parts[0].content_card?.kind, "codex.result");
    assert.equal(session.turns[0].parts[1].text, null);
    assert.equal(session.turns[0].parts[1].content_card, undefined);
    assert.equal(session.turns[0].parts[1].metadata_json, null);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter hides successful result cards while retaining command state", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-payload-policy-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    writeFileSync(rolloutPath, [
      event("2026-08-04T00:00:00Z", "user", "运行测试"),
      JSON.stringify({
        timestamp: "2026-08-04T00:00:01Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call",
          name: "exec",
          call_id: "call-tests",
          status: "completed",
          input: "const r = await tools.exec_command({\"cmd\":\"pnpm test\"}); text(r.output);",
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-04T00:00:02Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call_output",
          call_id: "call-tests",
          output: "Script completed\nWall time 0.2 seconds\nOutput:\n388 tests passed",
        },
      }),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('session-1', '${sqlString(rolloutPath)}', 'Fixture', '2026-08-04T00:00:02Z');`,
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    const parts = session.turns[0].parts;
    assert.equal(parts.length, 1);
    assert.deepEqual(parts.map((part) => part.source_execution_id), ["call-tests"]);
    assert.equal(parts[0].status, "completed");
    assert.equal(parts[0].exit_code, 0);
    assert.equal(JSON.parse(parts[0].metadata_json).payload_policy_version, PAYLOAD_POLICY_VERSION);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter stores patch_apply_end changes as one file-change Part per file", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-patch-diff-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    const projectPath = "/tmp/project";
    writeFileSync(rolloutPath, [
      JSON.stringify({
        timestamp: "2026-08-09T00:00:00Z",
        type: "session_meta",
        payload: { cwd: projectPath },
      }),
      event("2026-08-09T00:00:01Z", "user", "修改四个文件"),
      JSON.stringify({
        timestamp: "2026-08-09T00:00:02Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call",
          name: "apply_patch",
          call_id: "call-patch",
          input: [
            "*** Begin Patch",
            "*** Update File: /tmp/project/src/main.ts",
            "@@ -1 +1 @@",
            "-const oldValue = 1;",
            "+const command = \"npm test\";",
            "*** End Patch",
          ].join("\n"),
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-09T00:00:03Z",
        type: "event_msg",
        payload: {
          type: "patch_apply_end",
          call_id: "call-patch",
          success: true,
          status: "completed",
          changes: {
            "/tmp/project/src/main.ts": {
              type: "update",
              unified_diff: "@@ -1 +1 @@\n-const oldValue = 1;\n+const command = \"npm test\";\n",
              move_path: null,
            },
            "/tmp/project/src/new.ts": {
              type: "add",
              content: "export const created = true;\n",
            },
            "/tmp/project/src/old.ts": {
              type: "delete",
              content: "export const removed = true;\n",
            },
            "/tmp/project/src/from.ts": {
              type: "update",
              unified_diff: "@@ -1 +1 @@\n-export const name = 'from';\n+export const name = 'to';\n",
              move_path: "/tmp/project/src/to.ts",
            },
          },
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-09T00:00:04Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call_output",
          call_id: "call-patch",
          output: "Exit code: 0\nOutput:\nSuccess. Updated four files.",
        },
      }),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('session-1', '${sqlString(rolloutPath)}', 'Fixture', '2026-08-09T00:00:04Z');`,
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    const parts = session.turns[0].parts;
    assert.equal(parts.length, 5);
    assert.equal(parts[0].command, ["src/main.ts", "src/new.ts", "src/old.ts", "src/to.ts"].join("\n"));
    assert.equal(parts[0].command_label, "Edit");
    assert.equal(parts[0].source_execution_id, "call-patch");
    const fileParts = parts.slice(1);
    assert.deepEqual(fileParts.map((part) => part.source_execution_id), [
      "call-patch",
      "call-patch",
      "call-patch",
      "call-patch",
    ]);
    assert.deepEqual(fileParts.map((part) => JSON.parse(part.metadata_json).file_path), [
      "src/main.ts",
      "src/new.ts",
      "src/old.ts",
      "src/to.ts",
    ]);
    for (const [index, part] of fileParts.entries()) {
      assert.equal(part.kind, "file_change");
      assert.equal(part.content_card?.kind, "codex.file-change");
      assert.equal(part.content_card?.renderer, "diff");
      assert.equal(part.text.match(/^diff --git /gm)?.length, 1);
      const metadata = JSON.parse(part.metadata_json);
      assert.equal(metadata.file_change_index, index + 1);
      assert.equal(metadata.file_change_count, 4);
    }
    assert.match(fileParts[0].text, /diff --git a\/src\/main\.ts b\/src\/main\.ts/);
    assert.match(fileParts[1].text, /new file mode 100644[\s\S]*\+export const created = true;/);
    assert.match(fileParts[2].text, /deleted file mode 100644[\s\S]*-export const removed = true;/);
    assert.match(fileParts[3].text, /rename from src\/from\.ts[\s\S]*rename to src\/to\.ts/);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter correlates a nested apply_patch event with its outer exec call", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-nested-patch-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    writeFileSync(rolloutPath, [
      JSON.stringify({
        timestamp: "2026-08-09T00:00:00Z",
        type: "session_meta",
        payload: { cwd: "/tmp/project" },
      }),
      event("2026-08-09T00:00:01Z", "user", "修改入口文件"),
      JSON.stringify({
        timestamp: "2026-08-09T00:00:02Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call",
          name: "exec",
          call_id: "call-outer-exec",
          input: "const patch = `*** Begin Patch\\n*** Update File: src/main.ts\\n@@\\n-old\\n+new\\n*** End Patch`; text(await tools.apply_patch(patch));",
          internal_chat_message_metadata_passthrough: { turn_id: "turn-runtime" },
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-09T00:00:03Z",
        type: "event_msg",
        payload: {
          type: "patch_apply_end",
          call_id: "exec-inner-patch",
          turn_id: "turn-runtime",
          success: true,
          changes: {
            "/tmp/project/src/main.ts": {
              type: "update",
              unified_diff: "@@ -1 +1 @@\n-old\n+new\n",
            },
          },
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-09T00:00:04Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call_output",
          call_id: "call-outer-exec",
          output: [{ type: "input_text", text: "Script completed\nOutput:\n{}" }],
        },
      }),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('session-1', '${sqlString(rolloutPath)}', 'Fixture', '2026-08-09T00:00:04Z');`,
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    const parts = session.turns[0].parts;
    assert.equal(parts.length, 2);
    assert.equal(parts[0].command, "src/main.ts");
    assert.equal(parts[0].command_label, "Edit");
    assert.equal(parts[0].source_execution_id, "call-outer-exec");
    assert.equal(parts[1].source_execution_id, "call-outer-exec");
    assert.equal(parts[1].kind, "file_change");
    assert.equal(parts[1].content_card?.kind, "codex.file-change");
    assert.equal(parts[1].content_card?.renderer, "diff");
    assert.match(parts[1].text, /^diff --git a\/src\/main\.ts b\/src\/main\.ts/m);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

function readFixtureSession(fixtureRoot) {
  const result = spawnSync(process.execPath, [adapterPath], {
    encoding: "utf8",
    input: JSON.stringify({ method: "read_session", source: { location: fixtureRoot }, params: {} }),
  });
  assert.equal(result.status, 0, result.stderr);
  const messages = result.stdout.trim().split("\n").map((line) => JSON.parse(line));
  const session = messages.find((message) => message.type === "item")?.item?.session;
  assert.ok(session);
  return session;
}

function event(timestamp, role, text) {
  return JSON.stringify({
    timestamp,
    type: "response_item",
    payload: {
      type: "message",
      role,
      content: [{ type: role === "assistant" ? "output_text" : "input_text", text }],
    },
  });
}

function runSqlite(directory, sql) {
  const result = spawnSync("sqlite3", [path.join(directory, "state_5.sqlite")], {
    encoding: "utf8",
    input: sql,
  });
  assert.equal(result.status, 0, result.stderr);
}

function sqlString(value) {
  return value.replaceAll("'", "''");
}
